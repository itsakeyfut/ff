# ff-filter

Safe, high-level video and audio filter graph operations — no `unsafe` code required.

![Coming Soon](https://img.shields.io/badge/status-coming%20soon-yellow)

> **⚠️ Coming Soon — This crate is a placeholder and not yet implemented.**
> The API is under design. Do not use in production.

## Overview

`ff-filter` will provide a type-safe Rust interface to FFmpeg's `libavfilter`, enabling filter graph construction and execution for video and audio processing pipelines.

## Design Principles

All public APIs are **safe**. Users never need to write `unsafe` code. Unsafe FFmpeg internals (`libavfilter`) are fully encapsulated within this crate, following the same pattern as `ff-decode` and `ff-encode`.

## Planned Features

- **Filter graph API**: Construct complex filter chains with a builder pattern
- **Video filters**: trim, scale, crop, overlay, rotate, fade, color correction
- **Audio filters**: volume, equalizer, noise reduction, channel mixing, resampling
- **Hardware acceleration**: CUDA, OpenCL, Vulkan filter support
- **Pipeline integration**: Seamless interop with `ff-decode` and `ff-encode`

## Planned Usage

```rust,ignore
use ff_filter::FilterGraph;

// Trim from 10s to 30s, then scale to 720p
let graph = FilterGraph::new()
    .trim(10.0, 30.0)
    .scale(1280, 720)
    .build()?;
```

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## Related Crates

- **ff-probe** - Media metadata extraction
- **ff-decode** - Video/audio decoding (input to filter graph)
- **ff-encode** - Video/audio encoding (output from filter graph)
- **ff-format** - Shared type definitions
- **ff-sys** - Low-level FFmpeg FFI bindings

## License

MIT OR Apache-2.0
