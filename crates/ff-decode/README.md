# ff-decode

Safe, high-level video and audio decoding — no `unsafe` code required.

This crate provides frame-by-frame video/audio decoding, efficient seeking, and thumbnail generation. All APIs are **safe** — FFmpeg internals are fully encapsulated so you never need to write `unsafe` code.

## Features

- **Video Decoding**: Frame-by-frame decoding with Iterator pattern
- **Audio Decoding**: Sample-level audio extraction
- **Seeking**: Fast keyframe and exact seeking without file re-open
- **Thumbnails**: Efficient thumbnail generation for timelines
- **Hardware Acceleration**: Optional NVDEC, QSV, AMF, VideoToolbox, VAAPI support
- **Frame Pooling**: Memory reuse for reduced allocation overhead

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
ff-decode = "0.1"
ff-format = "0.1"
```

## Usage Examples

### Video Decoding

```rust
use ff_decode::{VideoDecoder, SeekMode};
use ff_format::PixelFormat;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open a video file and create decoder
    let mut decoder = VideoDecoder::open("video.mp4")?
        .output_format(PixelFormat::Rgba)
        .build()?;

    // Get basic info
    println!("Duration: {:?}", decoder.duration());
    println!("Resolution: {}x{}", decoder.width(), decoder.height());
    println!("FPS: {:.2}", decoder.frame_rate());

    // Decode frames sequentially using iterator
    for frame in decoder.frames().take(100) {
        let frame = frame?;
        println!("Frame at {:?}", frame.timestamp().as_duration());
        // Access frame data with frame.planes()
    }

    // Seek to specific position
    decoder.seek(Duration::from_secs(30), SeekMode::Keyframe)?;

    // Decode one frame after seeking
    if let Some(frame) = decoder.decode_one()? {
        println!("Frame at {:?}", frame.timestamp().as_duration());
    }

    Ok(())
}
```

### Audio Decoding

```rust
use ff_decode::AudioDecoder;
use ff_format::SampleFormat;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = AudioDecoder::open("audio.mp3")?
        .output_format(SampleFormat::F32)
        .output_sample_rate(48000)
        .build()?;

    // Get audio info
    println!("Sample rate: {} Hz", decoder.sample_rate());
    println!("Channels: {}", decoder.channels());
    println!("Duration: {:?}", decoder.duration());

    // Decode all audio frames
    for frame in decoder.frames().take(100) {
        let frame = frame?;
        println!("Audio frame with {} samples", frame.samples());
    }

    Ok(())
}
```

### Hardware Acceleration

```rust
use ff_decode::{VideoDecoder, HardwareAccel};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Auto-detect GPU and use hardware acceleration
    let decoder = VideoDecoder::open("video.mp4")?
        .hardware_accel(HardwareAccel::Auto)
        .build()?;

    // Or specify a specific accelerator
    let nvdec_decoder = VideoDecoder::open("video.mp4")?
        .hardware_accel(HardwareAccel::Nvdec)  // NVIDIA NVDEC
        .build()?;

    Ok(())
}
```

### Seeking

```rust
use ff_decode::{VideoDecoder, SeekMode};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = VideoDecoder::open("video.mp4")?.build()?;

    // Fast keyframe seek (may be slightly inaccurate)
    decoder.seek(Duration::from_secs(30), SeekMode::Keyframe)?;

    // Exact seek (slower but frame-accurate)
    decoder.seek(Duration::from_secs(30), SeekMode::Exact)?;

    // Backward seek (seeks to keyframe at or before target)
    decoder.seek(Duration::from_secs(30), SeekMode::Backward)?;

    Ok(())
}
```

### Thumbnail Generation

```rust
use ff_decode::VideoDecoder;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = VideoDecoder::open("video.mp4")?.build()?;

    // Generate a single thumbnail at specific time
    let thumbnail = decoder.thumbnail_at(
        Duration::from_secs(5),
        320,  // width
        180,  // height
    )?;

    // Generate multiple thumbnails for timeline
    let thumbnails = decoder.thumbnails(
        10,    // count
        160,   // width
        90,    // height
    )?;

    println!("Generated {} thumbnails", thumbnails.len());

    Ok(())
}
```

### Frame Pooling

```rust
use ff_decode::{VideoDecoder, SimpleFramePool};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a frame pool for memory reuse (32 frames)
    let pool = SimpleFramePool::new(32);

    let mut decoder = VideoDecoder::open("video.mp4")?
        .frame_pool(pool)
        .build()?;

    // Frames are automatically acquired from and returned to the pool
    for frame in decoder.frames().take(100) {
        let frame = frame?;
        // Process frame...
    }

    Ok(())
}
```

## Performance

### Benchmarking

Run performance benchmarks with:

```bash
# Run all benchmarks
cargo bench -p ff-decode

# Run specific benchmark group
cargo bench -p ff-decode -- seek

# Quick benchmarks (faster, less precise)
cargo bench -p ff-decode -- --quick
```

### Benchmark Results

Typical performance on modern hardware (actual results may vary):

| Operation | Time | Notes |
|-----------|------|-------|
| Keyframe seek | ~12ms | Fast seeking to nearest keyframe |
| Exact seek | ~20ms | Frame-accurate seeking |
| Single frame decode | ~0.3ms | H.264 1080p |
| Thumbnail (320x180) | ~12ms | Including seek and scale |
| Sequential decode (100 frames) | ~33ms | ~0.33ms per frame |

### Seek Performance

One of the key features of `ff-decode` is **efficient seeking without file re-opening**. The benchmark `seek_repeated/scrubbing_5_positions` demonstrates this - 5 seeks complete in ~100ms (20ms per seek), which would be much slower if the file was being re-opened each time.

## Platform Support

| Platform | Status | Hardware Acceleration |
|----------|--------|----------------------|
| Windows | ✅ Tested | NVDEC, QSV, AMF |
| macOS | ✅ Tested | VideoToolbox |
| Linux | ✅ Tested | VAAPI, NVDEC, QSV |

## Error Handling

All operations return `Result<T, DecodeError>`:

```rust
use ff_decode::{VideoDecoder, DecodeError};

fn decode_video(path: &str) -> Result<(), DecodeError> {
    let mut decoder = VideoDecoder::open(path)?
        .build()?;

    match decoder.decode_one()? {
        Some(frame) => println!("Got frame: {}x{}", frame.width(), frame.height()),
        None => println!("End of stream"),
    }

    Ok(())
}
```

Error types include:
- `DecodeError::FileNotFound` - File doesn't exist
- `DecodeError::NoVideoStream` - No video stream in file
- `DecodeError::NoAudioStream` - No audio stream in file
- `DecodeError::UnsupportedCodec` - Codec not supported
- `DecodeError::DecodingFailed` - Decoding error
- `DecodeError::SeekFailed` - Seek operation failed
- `DecodeError::HwAccelUnavailable` - Hardware acceleration unavailable
- `DecodeError::EndOfStream` - End of stream reached

## Module Structure

- `video` - Video decoder for extracting video frames
- `audio` - Audio decoder for extracting audio frames
- `error` - Error types for decoding operations
- `pool` - Frame pool trait for memory reuse

## Architecture

This crate is part of the `ff-*` suite of crates that provide a safe Rust wrapper around FFmpeg:

- **ff-sys** - Low-level FFI bindings (internal)
- **ff-format** - Common types (VideoFrame, AudioFrame, etc.)
- **ff-probe** - Metadata extraction
- **ff-decode** - Decoding (this crate)
- **ff-encode** - Encoding
- **ff-filter** - Filters and effects

## Performance Characteristics

Typical performance on modern hardware (see benchmarks with `cargo bench -p ff-decode`):

- **Frame decode**: ~0.3ms for 1080p H.264 (software decoding)
- **Seek (keyframe)**: ~12ms per seek (no file re-open)
- **Seek (exact)**: ~20ms per seek (includes frame skipping)
- **Thumbnail generation**: ~12ms per thumbnail (320x180)
- **Scrubbing (5 positions)**: ~100ms total (~20ms per position)

Hardware acceleration can significantly improve decode performance.

## License

Licensed under either of:

- MIT license
- Apache License, Version 2.0

at your option.
