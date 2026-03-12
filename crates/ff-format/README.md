# ff-format

Safe, FFmpeg-agnostic type definitions for video and audio processing.

## Overview

`ff-format` provides shared type definitions used across the ff-* crate family. All APIs are **safe** — no `unsafe` code is required. FFmpeg internals are completely hidden behind idiomatic Rust types.

## Features

- **Type-safe pixel formats**: YUV420p, RGBA, NV12, 10-bit HDR, and more
- **Audio sample formats**: F32, I16, planar/packed conversion
- **Precise timestamps**: Rational time bases with arithmetic operations
- **Video/Audio frames**: Safe abstractions for frame data
- **Stream metadata**: Video and audio stream information with builder pattern
- **Media container info**: Complete file metadata including multiple streams
- **Color metadata**: Color space, range, and primaries for HDR workflows

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## Module Structure

```
ff-format/src/
├── lib.rs          # Crate root, prelude
├── pixel.rs        # PixelFormat
├── sample.rs       # SampleFormat
├── time.rs         # Timestamp, Rational
├── frame/          # Frame types
│   ├── mod.rs
│   ├── video.rs    # VideoFrame
│   └── audio.rs    # AudioFrame
├── stream.rs       # VideoStreamInfo, AudioStreamInfo
├── media.rs        # MediaInfo
├── color.rs        # ColorSpace, ColorRange, ColorPrimaries
├── codec.rs        # VideoCodec, AudioCodec
├── channel.rs      # ChannelLayout
└── error.rs        # FormatError, FrameError
```

## Usage

### Basic Types

```rust
use ff_format::prelude::*;

// Pixel formats
let format = PixelFormat::Yuv420p;
assert!(format.is_planar());
assert_eq!(format.num_planes(), 3);

// Sample formats
let audio_fmt = SampleFormat::F32;
assert!(audio_fmt.is_float());
assert_eq!(audio_fmt.bytes_per_sample(), 4);

// Timestamps with time base
let time_base = Rational::new(1, 90000);
let ts = Timestamp::new(90000, time_base);
assert!((ts.as_secs_f64() - 1.0).abs() < 0.001);
```

### Stream Information

```rust
use ff_format::stream::{VideoStreamInfo, AudioStreamInfo};
use ff_format::codec::{VideoCodec, AudioCodec};
use ff_format::{PixelFormat, SampleFormat, Rational};

// Video stream with builder pattern
let video = VideoStreamInfo::builder()
    .index(0)
    .codec(VideoCodec::H264)
    .width(1920)
    .height(1080)
    .frame_rate(Rational::new(30, 1))
    .pixel_format(PixelFormat::Yuv420p)
    .build();

assert_eq!(video.width(), 1920);
assert!(video.is_full_hd());

// Audio stream
let audio = AudioStreamInfo::builder()
    .index(1)
    .codec(AudioCodec::Aac)
    .sample_rate(48000)
    .channels(2)
    .sample_format(SampleFormat::F32)
    .build();

assert!(audio.is_stereo());
```

### Media Container Info

```rust
use ff_format::media::MediaInfo;
use std::time::Duration;

let media = MediaInfo::builder()
    .path("/path/to/video.mp4")
    .format("mp4")
    .duration(Duration::from_secs(120))
    .file_size(100_000_000)
    .video_stream(video)
    .audio_stream(audio)
    .metadata("title", "My Video")
    .build();

assert!(media.has_video());
assert!(media.has_audio());
assert_eq!(media.resolution(), Some((1920, 1080)));
```

## License

MIT OR Apache-2.0
