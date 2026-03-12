# ff-pipeline

Unified decode-filter-encode pipeline — no `unsafe` code required.

![Coming Soon](https://img.shields.io/badge/status-coming%20soon-yellow)

> **⚠️ Coming Soon — This crate is a placeholder and not yet implemented.**
> The API is under design. Do not use in production.

## Overview

`ff-pipeline` will provide a high-level `Pipeline` type that connects `ff-decode`, `ff-filter`, and `ff-encode` into a single, progress-aware transcode pipeline.

## Design Principles

All public APIs are **safe**. Users never need to write `unsafe` code. Unsafe FFmpeg internals are fully encapsulated within the underlying crates.

## Planned Features

- **Unified pipeline**: Connect decode → filter → encode in a single builder call
- **Progress tracking**: `on_progress` callback with percent, elapsed, and ETA
- **Cancellation**: Stop a running pipeline via the progress callback
- **Multi-input**: Concatenate multiple input files
- **Parallel thumbnails**: `ThumbnailPipeline` with optional `rayon` feature

## Planned Usage

```rust,ignore
use ff_pipeline::Pipeline;

let pipeline = Pipeline::builder()
    .input("input.mp4")
    .output("output.mp4", encoder_config)
    .on_progress(|p| println!("{:.1}%", p.percent()))
    .build()?;

pipeline.run()?;
```

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## Related Crates

- **ff-decode** — Video/audio decoding
- **ff-filter** — Filter graph operations
- **ff-encode** — Video/audio encoding
- **ff-stream** — HLS/DASH streaming output
- **ff** — Facade crate (re-exports all)

## License

MIT OR Apache-2.0
