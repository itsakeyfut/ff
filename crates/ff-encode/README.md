# ff-encode

Encode video and audio to any format with a builder chain. The encoder validates your codec, resolution, and bitrate settings before allocating any FFmpeg context — invalid configurations are caught as `Err`, not discovered at the first `push_video` call.

## Installation

```toml
[dependencies]
ff-encode = "0.6"
ff-format = "0.6"

# Enable GPL-licensed encoders (libx264, libx265).
# Requires GPL compliance or MPEG LA licensing in your project.
# ff-encode = { version = "0.6", features = ["gpl"] }
```

By default, only LGPL-compatible encoders are enabled.

## Quick Start

```rust
use ff_encode::{VideoEncoder, VideoCodec, AudioCodec, BitrateMode, Preset};

let mut encoder = VideoEncoder::create("output.mp4")
    .video(1920, 1080, 30.0)          // width, height, fps
    .video_codec(VideoCodec::H264)
    .bitrate_mode(BitrateMode::Cbr(4_000_000))
    .preset(Preset::Medium)
    .audio(48_000, 2)                 // sample_rate, channels
    .audio_codec(AudioCodec::Aac)
    .audio_bitrate(192_000)
    .build()?;

for frame in &video_frames {
    encoder.push_video(frame)?;
}
for frame in &audio_frames {
    encoder.push_audio(frame)?;
}
encoder.finish()?;
```

## Quality Modes

```rust
// Constant bitrate — predictable file size and bandwidth.
.bitrate_mode(BitrateMode::Cbr(4_000_000))  // 4 Mbps

// Constant rate factor — quality-driven; file size varies.
.bitrate_mode(BitrateMode::Crf(23))          // 0–51, lower = better

// Variable bitrate — target average with hard ceiling.
.bitrate_mode(BitrateMode::Vbr { target: 3_000_000, max: 6_000_000 })
```

## Hardware Encoding

```rust
use ff_encode::{VideoEncoder, VideoCodec, HardwareEncoder};

let mut encoder = VideoEncoder::create("output.mp4")
    .video(1920, 1080, 60.0)
    .video_codec(VideoCodec::H264)
    .hardware_encoder(HardwareEncoder::Auto)
    .build()?;
```

`HardwareEncoder::Auto` selects NVENC, QuickSync, AMF, or VideoToolbox based on what is available at runtime. If no hardware encoder is found, the builder falls back to a software encoder automatically.

## Progress Tracking

```rust
let mut encoder = VideoEncoder::create("output.mp4")
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::H264)
    .bitrate_mode(BitrateMode::Cbr(4_000_000))
    .on_progress(|p| {
        println!(
            "{:.1}% — {} frames, {:.1}s elapsed",
            p.percent(),
            p.frames_encoded,
            p.elapsed.as_secs_f64()
        );
    })
    .build()?;
```

## LGPL Compliance

By default, `ff-encode` only links encoders that are compatible with the LGPL license — hardware encoders (NVENC, QSV, AMF, VideoToolbox) or software encoders for VP9 and AV1.

Enable the `gpl` feature to add libx264 and libx265. This changes the license terms of your binary; ensure you comply with the GPL or hold an appropriate MPEG LA commercial license before distributing.

## Error Handling

| Variant                            | When it occurs                                     |
|------------------------------------|----------------------------------------------------|
| `EncodeError::InvalidConfig`       | Codec, resolution, or bitrate settings are invalid |
| `EncodeError::UnsupportedCodec`    | Requested codec not available in this FFmpeg build |
| `EncodeError::HardwareUnavailable` | Hardware encoder requested but no device found     |
| `EncodeError::Io`                  | Write error on the output file                     |
| `EncodeError::Encode`              | FFmpeg returned an error during frame encoding     |

## Feature Flags

| Flag | Description | Default |
|------|-------------|---------|
| `hwaccel` | Enables hardware encoder detection (NVENC, QSV, AMF, VideoToolbox, VA-API). | enabled |
| `gpl` | Enables GPL-licensed encoders (libx264, libx265). Requires GPL compliance or MPEG LA licensing. | disabled |
| `tokio` | Enables `AsyncVideoEncoder` and `AsyncAudioEncoder`. Each encoder runs a worker thread and exposes an async `push` / `finish` interface backed by a bounded `tokio::sync::mpsc` channel (capacity 8). Requires a tokio 1.x runtime. | disabled |

```toml
[dependencies]
ff-encode = { version = "0.6", features = ["tokio"] }
```

When the `tokio` feature is disabled, only the synchronous `VideoEncoder`, `AudioEncoder`, and `ImageEncoder` APIs are compiled. No tokio dependency is pulled in.

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0
