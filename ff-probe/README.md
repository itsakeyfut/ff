# ff-probe

Safe, high-level media file metadata extraction — no `unsafe` code required.

## Overview

`ff-probe` provides functionality for extracting metadata from media files, including video streams, audio streams, and container information. All APIs are **safe** — FFmpeg internals are fully encapsulated so you never need to write `unsafe` code. It serves as the Rust equivalent of ffprobe with a clean, ergonomic API.

## Features

- **Container format detection**: MP4, MKV, AVI, MOV, WebM, and more
- **Video stream analysis**: Codec, resolution, frame rate, pixel format
- **Audio stream analysis**: Codec, sample rate, channels, sample format
- **Color metadata**: Color space, range, and primaries for HDR workflows
- **Bitrate extraction**: Container and stream-level bitrate information
- **Metadata access**: Title, artist, album, and custom metadata tags

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## Module Structure

```
ff-probe/src/
├── lib.rs      # Crate root, prelude, re-exports
├── info.rs     # open() function, FFmpeg integration
└── error.rs    # ProbeError
```

## Usage

### Quick Start

```rust
use ff_probe::open;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let info = open("video.mp4")?;

    println!("Format: {}", info.format());
    println!("Duration: {:?}", info.duration());

    if let Some(video) = info.primary_video() {
        println!("Video: {}x{} @ {:.2} fps",
            video.width(),
            video.height(),
            video.fps()
        );
    }

    if let Some(audio) = info.primary_audio() {
        println!("Audio: {} Hz, {} channels",
            audio.sample_rate(),
            audio.channels()
        );
    }

    Ok(())
}
```

### Detailed Information

```rust
use ff_probe::{open, ColorSpace, ColorPrimaries};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let info = open("video.mp4")?;

    // Enumerate all video streams
    for (i, stream) in info.video_streams().iter().enumerate() {
        println!("Video stream {}: {} {}x{}",
            i, stream.codec_name(), stream.width(), stream.height());
        println!("  Color space: {:?}", stream.color_space());
        println!("  Color range: {:?}", stream.color_range());

        // Check for HDR content
        if stream.color_primaries() == ColorPrimaries::Bt2020 {
            println!("  HDR content detected!");
        }

        if let Some(bitrate) = stream.bitrate() {
            println!("  Bitrate: {} kbps", bitrate / 1000);
        }
    }

    // Enumerate all audio streams
    for (i, stream) in info.audio_streams().iter().enumerate() {
        println!("Audio stream {}: {} {} Hz, {} ch",
            i, stream.codec_name(), stream.sample_rate(), stream.channels());
        if let Some(lang) = stream.language() {
            println!("  Language: {}", lang);
        }
    }

    // Access container metadata
    if let Some(title) = info.title() {
        println!("Title: {}", title);
    }

    Ok(())
}
```

### Error Handling

```rust
use ff_probe::{open, ProbeError};

let result = open("/nonexistent/path.mp4");

match result {
    Err(ProbeError::FileNotFound { path }) => {
        println!("File not found: {}", path.display());
    }
    Err(ProbeError::CannotOpen { path, reason }) => {
        println!("Cannot open {}: {}", path.display(), reason);
    }
    Err(ProbeError::InvalidMedia { path, reason }) => {
        println!("Invalid media {}: {}", path.display(), reason);
    }
    Err(e) => println!("Other error: {}", e),
    Ok(info) => println!("Opened: {}", info.format()),
}
```

## Dependencies

- `ff-format`: Common types for video/audio processing
- `ff-sys`: FFmpeg FFI bindings

## License

MIT OR Apache-2.0
