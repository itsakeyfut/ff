# ff-encode

Safe, high-level video and audio encoding — no `unsafe` code required, LGPL-compliant by default.

## Overview

All APIs are **safe** — FFmpeg internals are fully encapsulated so you never need to write `unsafe` code.

## ⚖️ License & Commercial Use

**This crate is designed for commercial use without licensing fees.**

### Default Behavior (LGPL-Compatible) ✅

By default, `ff-encode` only uses LGPL-compatible encoders, making it **safe for commercial use**:

- ✅ **Free for commercial use** - No licensing fees
- ✅ **No royalty payments** required
- ✅ **Safe for proprietary software**
- ✅ **No GPL contamination**

### How It Works

When you request H.264 or H.265 encoding, the crate automatically selects encoders in this priority:

1. **Hardware encoders** (LGPL-compatible):
   - NVIDIA NVENC (`h264_nvenc`, `hevc_nvenc`)
   - Intel Quick Sync Video (`h264_qsv`, `hevc_qsv`)
   - AMD AMF/VCE (`h264_amf`, `hevc_amf`)
   - Apple VideoToolbox (`h264_videotoolbox`, `hevc_videotoolbox`)
   - VA-API (`h264_vaapi`, `hevc_vaapi`) - Linux

2. **Fallback to royalty-free codecs**:
   - H.264 → VP9 (libvpx-vp9)
   - H.265 → AV1 (libaom-av1)

### GPL Feature ⚠️

The `gpl` feature enables libx264/libx265 software encoders, which **require licensing fees** for commercial distribution:

```toml
# WARNING: Requires GPL compliance and MPEG LA licensing for commercial use
ff-encode = { version = "0.1", features = ["gpl"] }
```

**Only enable `gpl` if:**
- ✅ You have appropriate licenses from MPEG LA, **or**
- ✅ Your software is GPL-licensed (open source), **or**
- ✅ For non-commercial/educational use only

## 📦 Installation

```toml
[dependencies]
# Default: LGPL-compatible (commercial use OK)
ff-encode = "0.1"

# With GPU acceleration (recommended)
ff-encode = { version = "0.1", features = ["hwaccel"] }

# With GPL codecs (requires licensing)
ff-encode = { version = "0.1", features = ["gpl"] }
```

## 🚀 Quick Start

```rust
use ff_encode::{VideoEncoder, VideoCodec, AudioCodec};

// Create encoder - will automatically use LGPL-compatible encoder
let mut encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::H264)  // Will use hardware or VP9
    .audio(48000, 2)
    .audio_codec(AudioCodec::Aac)
    .build()?;

// Verify LGPL compliance
println!("Encoder: {}", encoder.actual_video_codec());
println!("LGPL compliant: {}", encoder.is_lgpl_compliant());

// Encode frames
for frame in video_frames {
    encoder.push_video(&frame)?;
}

encoder.finish()?;
```

## 🔍 Checking Compliance at Runtime

```rust
let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::H264)
    .build()?;

if encoder.is_lgpl_compliant() {
    println!("✓ Safe for commercial use: {}", encoder.actual_video_codec());
} else {
    println!("⚠ GPL encoder (requires licensing): {}", encoder.actual_video_codec());
}
```

Example outputs:
- `✓ Safe for commercial use: h264_nvenc` - NVIDIA hardware encoder
- `✓ Safe for commercial use: libvpx-vp9` - VP9 fallback
- `⚠ GPL encoder: libx264` - Requires licensing (only with `gpl` feature)

## 📚 Features

### Video Codecs

- **H.264/AVC** - Most compatible (auto-selects hardware or VP9 fallback)
- **H.265/HEVC** - High compression (auto-selects hardware or AV1 fallback)
- **VP9** - Google's royalty-free codec (LGPL-compatible)
- **AV1** - Next-gen royalty-free codec (LGPL-compatible)
- **ProRes** - Apple's professional codec
- **DNxHD** - Avid's professional codec

### Audio Codecs

- **AAC** - Most compatible
- **Opus** - High quality, low latency
- **MP3** - Universal compatibility
- **FLAC** - Lossless
- **PCM** - Uncompressed

### Hardware Acceleration

All hardware encoders are LGPL-compatible:

```rust
use ff_encode::HardwareEncoder;

let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 60.0)
    .hardware_encoder(HardwareEncoder::Nvenc)  // Force NVIDIA
    .build()?;

// Check available hardware
for hw in HardwareEncoder::available() {
    println!("Available: {:?}", hw);
}
```

## ⚡ Performance

### Encoding Presets

```rust
use ff_encode::Preset;

let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .preset(Preset::Fast)      // Faster encoding, larger file
    .build()?;
```

Available presets:
- `Ultrafast` - Fastest, lowest quality
- `Fast` / `Faster` - Good for real-time
- `Medium` - Default, balanced
- `Slow` / `Slower` / `Veryslow` - Best quality

### Quality Control

```rust
// Constant bitrate
let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .video_bitrate(8_000_000)  // 8 Mbps
    .build()?;

// Constant quality (CRF)
let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .video_quality(23)  // 0-51, lower = better
    .build()?;
```

## 📊 Progress Tracking

```rust
use ff_encode::Progress;

let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .on_progress(|progress: Progress| {
        println!("Progress: {:.1}%", progress.percent());
        println!("Frames: {} / {}",
            progress.frames_encoded,
            progress.total_frames.unwrap_or(0)
        );
    })
    .build()?;
```

## 🛡️ Safety & Error Handling

All operations use proper error types:

```rust
use ff_encode::EncodeError;

match VideoEncoder::create("output.mp4") {
    Ok(builder) => { /* ... */ },
    Err(EncodeError::CannotCreateFile { path }) => {
        eprintln!("Cannot create: {}", path.display());
    },
    Err(EncodeError::NoSuitableEncoder { codec, tried }) => {
        eprintln!("No encoder for {}, tried: {:?}", codec, tried);
    },
    Err(e) => eprintln!("Error: {}", e),
}
```

## 🔧 Advanced Usage

### Force VP9 (Always LGPL-Compatible)

```rust
let encoder = VideoEncoder::create("output.webm")?
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::Vp9)  // Explicit VP9
    .build()?;
```

### Disable Hardware Acceleration

```rust
let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .hardware_encoder(HardwareEncoder::None)  // Software only
    .build()?;
// Will use VP9 or AV1 (LGPL-compatible)
```

## 📜 License

This crate: MIT OR Apache-2.0

FFmpeg: LGPL 2.1+ (or GPL 2+ with `gpl` feature)

**Important**: The default configuration (without `gpl` feature) is LGPL-compliant and safe for commercial use without licensing fees.

## ❓ FAQ

### Q: Can I use this in my commercial product?

**A:** Yes! By default (without the `gpl` feature), all encoders are LGPL-compatible and free for commercial use.

### Q: What if I need libx264/libx265?

**A:** Enable the `gpl` feature, but be aware:
- You must comply with GPL license terms, **or**
- Obtain commercial licenses from MPEG LA for H.264/H.265

### Q: What's the quality difference between hardware and software encoding?

**A:** Modern hardware encoders (NVENC, QSV) have excellent quality, often comparable to software encoders at similar bitrates. VP9 and AV1 provide better compression than H.264 but require more CPU time.

### Q: Which hardware encoder should I use?

**A:** Use `HardwareEncoder::Auto` (default) to automatically select the best available hardware encoder. The encoder will try NVENC → QSV → AMF → VideoToolbox in order.

## 🔗 See Also

- [ff-decode](../ff-decode) - Video decoding
- [ff-probe](../ff-probe) - Media metadata extraction
- [FFmpeg](https://ffmpeg.org/) - Underlying media framework
