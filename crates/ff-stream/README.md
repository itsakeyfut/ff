# ff-stream

HLS and DASH adaptive streaming output — no `unsafe` code required.

![Coming Soon](https://img.shields.io/badge/status-coming%20soon-yellow)

> **⚠️ Coming Soon — This crate is a placeholder and not yet implemented.**
> The API is under design. Do not use in production.

## Overview

`ff-stream` will provide `HlsOutput`, `DashOutput`, and `AbrLadder` types for producing adaptive bitrate streaming content from video files.

## Design Principles

All public APIs are **safe**. Users never need to write `unsafe` code. Unsafe FFmpeg internals are fully encapsulated within the underlying `ff-encode` crate.

## Planned Features

- **HLS output**: Produce `.m3u8` playlists and `.ts` segments via `HlsOutput`
- **DASH output**: Produce `.mpd` manifests and segments via `DashOutput`
- **ABR ladder**: Multi-rendition encoding in one pass via `AbrLadder`
- **Keyframe control**: Configurable keyframe interval for clean segment boundaries

## Planned Usage

```rust,ignore
use ff_stream::{HlsOutput, AbrLadder, Rendition};
use std::time::Duration;

// Single-quality HLS
HlsOutput::new("out/")
    .segment_duration(Duration::from_secs(6))
    .write()?;

// Multi-rendition ABR ladder
AbrLadder::new("input.mp4")
    .add_rendition(Rendition { width: 1920, height: 1080, bitrate: 6_000_000 })
    .add_rendition(Rendition { width: 1280, height: 720,  bitrate: 3_000_000 })
    .hls("out/")?;
```

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## Related Crates

- **ff-encode** — Video/audio encoding (used internally)
- **ff-pipeline** — Unified decode-filter-encode pipeline
- **ff** — Facade crate (re-exports all)

## License

MIT OR Apache-2.0
