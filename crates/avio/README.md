# avio

[![Crates.io](https://img.shields.io/crates/v/avio.svg)](https://crates.io/crates/avio)
[![Docs.rs](https://docs.rs/avio/badge.svg)](https://docs.rs/avio)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

`avio` is the unified facade for the `ff-*` crate family. Depend on a single crate and opt in
to only the capabilities you need via feature flags.

## Installation

```toml
[dependencies]
# Default: probe + decode + encode
avio = "0.6"

# Add filtering
avio = { version = "0.6", features = ["filter"] }

# Full stack (implies filter + pipeline)
avio = { version = "0.6", features = ["stream"] }

# Async decode/encode (requires tokio runtime)
avio = { version = "0.6", features = ["tokio"] }
```

## Feature Flags

| Feature    | Enables                                        | Default |
|------------|------------------------------------------------|---------|
| `probe`    | `ff-probe` — read-only media metadata          | yes     |
| `decode`   | `ff-decode` — video and audio decoding         | yes     |
| `encode`   | `ff-encode` — video and audio encoding         | yes     |
| `filter`   | `ff-filter` — filter graph operations          | no      |
| `pipeline` | `ff-pipeline` — decode → filter → encode       | no      |
| `stream`   | `ff-stream` — HLS / DASH streaming output      | no      |
| `tokio`    | Async wrappers for decode and encode           | no      |

## Quick Start

### Probe

```rust
use avio::open;

let info = open("video.mp4")?;
if let Some(video) = info.primary_video() {
    println!("{}x{} @ {:.2} fps ({:?})", video.width(), video.height(), video.fps(), video.codec());
}
```

### Decode

```rust
use avio::{VideoDecoder, AudioDecoder, PixelFormat, SampleFormat};

// Video — request RGB24 output (FFmpeg converts internally)
let mut vdec = VideoDecoder::open("video.mp4")
    .output_format(PixelFormat::Rgb24)
    .build()?;
while let Some(frame) = vdec.decode_one()? {
    // frame.data contains raw pixel bytes
}

// Audio — resample to f32 at 44.1 kHz
let mut adec = AudioDecoder::open("audio.flac")
    .output_format(SampleFormat::F32)
    .output_sample_rate(44_100)
    .build()?;
while let Some(frame) = adec.decode_one()? {
    // frame.data contains audio samples
}
```

### Encode

```rust
use avio::{VideoEncoder, VideoCodec, AudioCodec, BitrateMode};

let mut encoder = VideoEncoder::create("output.mp4")
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::H264)
    .bitrate_mode(BitrateMode::Crf(23))
    .audio(48_000, 2)
    .audio_codec(AudioCodec::Aac)
    .build()?;

for frame in &video_frames {
    encoder.push_video(frame)?;
}
encoder.finish()?;
```

### Async (tokio feature)

```rust
use avio::{AsyncVideoDecoder, AsyncVideoEncoder, VideoEncoder, VideoCodec};
use futures::StreamExt;

let mut encoder = AsyncVideoEncoder::from_builder(
    VideoEncoder::create("output.mp4")
        .video(1920, 1080, 30.0)
        .video_codec(VideoCodec::H264),
)?;

let stream = AsyncVideoDecoder::open("input.mp4").await?.into_stream();
tokio::pin!(stream);
while let Some(Ok(frame)) = stream.next().await {
    encoder.push(frame).await?;
}
encoder.finish().await?;
```

### Pipeline (pipeline feature)

```rust
use avio::{Pipeline, EncoderConfig, VideoCodec, AudioCodec, BitrateMode};

Pipeline::builder()
    .input("input.mp4")
    .output("output.mp4", EncoderConfig::builder()
        .video_codec(VideoCodec::H264)
        .audio_codec(AudioCodec::Aac)
        .bitrate_mode(BitrateMode::Crf(23))
        .build())
    .build()?
    .run()?;
```

## Crate Family

| Crate         | Purpose                                        |
|---------------|------------------------------------------------|
| `ff-sys`      | Raw bindgen FFI — internal use only            |
| `ff-common`   | Shared buffer-pooling abstractions             |
| `ff-format`   | Pure-Rust type definitions (no FFmpeg linkage) |
| `ff-probe`    | Read-only media metadata extraction            |
| `ff-decode`   | Video and audio decoding                       |
| `ff-encode`   | Video and audio encoding                       |
| `ff-filter`   | Filter graph operations                        |
| `ff-pipeline` | Decode → filter → encode pipeline              |
| `ff-stream`   | HLS / DASH adaptive streaming output           |
| `avio`        | Unified facade (this crate)                    |

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0
