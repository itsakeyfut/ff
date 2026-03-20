# avio

Safe, high-level audio/video/image processing for Rust — decode, encode, probe, and filter without `unsafe` code.

[![Crates.io](https://img.shields.io/crates/v/avio.svg)](https://crates.io/crates/avio)
[![Docs.rs](https://docs.rs/avio/badge.svg)](https://docs.rs/avio)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

## Contributing

All open issues are tracked in the master issue [#112](https://github.com/itsakeyfut/avio/issues/112). I am currently the sole maintainer and implementing everything myself, so contributions are not expected — but if you would like to help, check issue #112 to find the next item to work on and give it a try.

Please assign me as a reviewer on your pull request. I will review it and provide feedback within one week.

Thank you for any contribution you make.

## Overview

`avio` is a family of Rust crates that provide safe, ergonomic multimedia processing. All public APIs are **safe** — unsafe internals are fully encapsulated so you never need to write `unsafe` code in your application.

Currently backed by FFmpeg, with planned support for GStreamer and other backends.

```rust
use ff_probe::open;
use ff_decode::{VideoDecoder, SeekMode};
use ff_encode::{VideoEncoder, VideoCodec, AudioCodec};
use std::time::Duration;

// Inspect a media file
let info = open("input.mp4")?;
println!("{}x{} @ {:.2} fps", info.primary_video().unwrap().width(), ...);

// Decode frames
let mut decoder = VideoDecoder::open("input.mp4")?.build()?;
for frame in decoder.frames().take(100) {
    let frame = frame?;
    // process frame.planes() ...
}

// Re-encode
let mut encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::H264)
    .audio(48000, 2)
    .audio_codec(AudioCodec::Aac)
    .build()?;
encoder.finish()?;
```

## Crate Family

| Crate | Description | crates.io | docs.rs |
|-------|-------------|-----------|---------|
| [`ff-probe`](./crates/ff-probe) | Media metadata extraction | [![](https://img.shields.io/crates/v/ff-probe.svg)](https://crates.io/crates/ff-probe) | [![](https://docs.rs/ff-probe/badge.svg)](https://docs.rs/ff-probe) |
| [`ff-decode`](./crates/ff-decode) | Video and audio decoding | [![](https://img.shields.io/crates/v/ff-decode.svg)](https://crates.io/crates/ff-decode) | [![](https://docs.rs/ff-decode/badge.svg)](https://docs.rs/ff-decode) |
| [`ff-encode`](./crates/ff-encode) | Video and audio encoding | [![](https://img.shields.io/crates/v/ff-encode.svg)](https://crates.io/crates/ff-encode) | [![](https://docs.rs/ff-encode/badge.svg)](https://docs.rs/ff-encode) |
| [`ff-filter`](./crates/ff-filter) | Filter graph operations | [![](https://img.shields.io/crates/v/ff-filter.svg)](https://crates.io/crates/ff-filter) | [![](https://docs.rs/ff-filter/badge.svg)](https://docs.rs/ff-filter) |
| [`ff-pipeline`](./crates/ff-pipeline) | Decode-filter-encode pipeline | [![](https://img.shields.io/crates/v/ff-pipeline.svg)](https://crates.io/crates/ff-pipeline) | [![](https://docs.rs/ff-pipeline/badge.svg)](https://docs.rs/ff-pipeline) |
| [`ff-stream`](./crates/ff-stream) | HLS/DASH streaming output | [![](https://img.shields.io/crates/v/ff-stream.svg)](https://crates.io/crates/ff-stream) | [![](https://docs.rs/ff-stream/badge.svg)](https://docs.rs/ff-stream) |
| [`ff-format`](./crates/ff-format) | Shared type definitions | [![](https://img.shields.io/crates/v/ff-format.svg)](https://crates.io/crates/ff-format) | [![](https://docs.rs/ff-format/badge.svg)](https://docs.rs/ff-format) |
| [`ff-common`](./crates/ff-common) | Common traits and buffer pooling | [![](https://img.shields.io/crates/v/ff-common.svg)](https://crates.io/crates/ff-common) | [![](https://docs.rs/ff-common/badge.svg)](https://docs.rs/ff-common) |
| [`ff-sys`](./crates/ff-sys) | Low-level FFmpeg FFI bindings | [![](https://img.shields.io/crates/v/ff-sys.svg)](https://crates.io/crates/ff-sys) | [![](https://docs.rs/ff-sys/badge.svg)](https://docs.rs/ff-sys) |
| [`avio`](./crates/avio) | Facade crate — re-exports all member crates | [![](https://img.shields.io/crates/v/avio.svg)](https://crates.io/crates/avio) | [![](https://docs.rs/avio/badge.svg)](https://docs.rs/avio) |

## Features

- **Safe API** — No `unsafe` code required in user code
- **Probe** — Extract metadata (codec, resolution, duration, bitrate, HDR info) from any media file
- **Decode** — Frame-by-frame video/audio decoding with Iterator pattern, seeking, and thumbnail generation
- **Encode** — Video/audio encoding with hardware acceleration and LGPL-compliant defaults
- **Hardware Acceleration** — NVENC/NVDEC, Intel QSV, AMD AMF, Apple VideoToolbox, VA-API
- **Filter Graph** — Trim, scale, crop, overlay, and more via `libavfilter`
- **HLS/DASH Streaming** — Adaptive bitrate output via `ff-stream`
- **Async** — Tokio-backed async decode/encode with back-pressure (`tokio` feature)
- **Cross-platform** — Windows, macOS, Linux

## Installation

Add the crates you need to your `Cargo.toml`:

```toml
[dependencies]
ff-probe  = "0.6"
ff-decode = "0.6"
ff-encode = "0.6"

# Or use the facade crate for everything
avio = "0.6"
```

### Prerequisites

FFmpeg development libraries must be installed on your system.

#### Windows

```powershell
vcpkg install ffmpeg:x64-windows
$env:VCPKG_ROOT = "C:\vcpkg"
```

#### macOS

```bash
brew install ffmpeg
```

#### Linux (Debian/Ubuntu)

```bash
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev
```

## Quick Start

### Probe

```rust
use ff_probe::open;

let info = open("video.mp4")?;

if let Some(video) = info.primary_video() {
    println!("{}x{} @ {:.2} fps ({:?})", video.width(), video.height(), video.fps(), video.codec());
}
if let Some(audio) = info.primary_audio() {
    println!("{} Hz, {} ch ({:?})", audio.sample_rate(), audio.channels(), audio.codec());
}
```

### Decode

```rust
use ff_decode::{VideoDecoder, AudioDecoder, SeekMode};
use ff_format::{PixelFormat, SampleFormat};
use std::time::Duration;

// Video
let mut decoder = VideoDecoder::open("video.mp4")?
    .output_format(PixelFormat::Rgba)
    .build()?;

for frame in decoder.frames() {
    let frame = frame?;
    // frame.planes() contains pixel data
}

// Seek and decode a single frame
decoder.seek(Duration::from_secs(30), SeekMode::Exact)?;
let frame = decoder.decode_one()?;

// Audio
let mut decoder = AudioDecoder::open("audio.mp3")?
    .output_format(SampleFormat::F32)
    .output_sample_rate(48000)
    .build()?;

for frame in decoder.frames() {
    let frame = frame?;
    // frame.channel_data() contains audio samples
}
```

### Encode

```rust
use ff_encode::{VideoEncoder, VideoCodec, AudioCodec, Preset};

// Automatically selects an LGPL-compatible encoder (hardware or VP9/AV1 fallback)
let mut encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 30.0)
    .video_codec(VideoCodec::H264)
    .video_bitrate(8_000_000)
    .preset(Preset::Fast)
    .audio(48000, 2)
    .audio_codec(AudioCodec::Aac)
    .build()?;

for frame in video_frames {
    encoder.push_video(&frame)?;
}
encoder.finish()?;
```

### Hardware Acceleration

```rust
use ff_decode::{VideoDecoder, HardwareAccel};
use ff_encode::{VideoEncoder, HardwareEncoder};

// Decode with GPU
let decoder = VideoDecoder::open("video.mp4")?
    .hardware_accel(HardwareAccel::Auto)
    .build()?;

// Encode with GPU
let encoder = VideoEncoder::create("output.mp4")?
    .video(1920, 1080, 60.0)
    .hardware_encoder(HardwareEncoder::Auto)
    .build()?;
```

## Showcase

### [ascii-term](https://github.com/itsakeyfut/ascii-term) — Terminal ASCII Art Video Player

A full-featured terminal media player built entirely on `avio`. It renders video as colored
ASCII art in the terminal with synchronized audio playback, and was fully migrated from
`ffmpeg-next` / `ffmpeg-sys-next` to `avio`.

**What it demonstrates:**

- `VideoDecoder` with `PixelFormat::Rgb24` output for per-pixel luminance mapping
- `AudioDecoder` with custom PCM conversion (`SampleFormat::F32`) feeding [rodio](https://crates.io/crates/rodio) for playback
- Synchronized audio/video across two threads via `crossbeam-channel`
- 10 selectable ASCII character maps, per-character RGB coloring, real-time terminal resize

This is a real-world proof that `avio` can replace `ffmpeg-next` / `ffmpeg-sys-next` in
decode-heavy applications without any direct `unsafe` FFmpeg code in the application layer.

## Platform Support

| Platform | Status | Hardware Acceleration |
|----------|--------|-----------------------|
| Windows | ✅ | NVENC/NVDEC, QSV, AMF |
| macOS | ✅ | VideoToolbox |
| Linux | ✅ | VAAPI, NVENC/NVDEC, QSV |

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## License

Licensed under either of:

- [MIT License](./LICENSE-MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.

### FFmpeg License

This project links against FFmpeg, which is licensed under [LGPL 2.1+](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html) by default. The `gpl` feature of `ff-encode` enables GPL-licensed codecs (libx264, libx265) — see [`ff-encode`](./crates/ff-encode/README.md) for details.

## Test Assets

The audio fixture used in integration tests is provided by [Music Atelier Amacha](https://amachamusic.chagasi.com/) (甘茶の音楽工房), composed by Amacha. Used with permission under the site's free-use terms.
