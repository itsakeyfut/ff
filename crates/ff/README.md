# ff

High-level, safe FFmpeg bindings for Rust.

![Coming Soon](https://img.shields.io/badge/status-coming%20soon-yellow)

> **⚠️ Coming Soon — This crate is a placeholder and not yet fully implemented.**
> The core crates (`ff-probe`, `ff-decode`, `ff-encode`) are under active development.

## Overview

`ff` is the facade crate for the ff-* crate family. It re-exports the public APIs of all member crates behind feature flags, so you can depend on a single crate and opt into only the functionality you need.

## Feature Flags

| Feature    | Crate         | Default |
|------------|---------------|---------|
| `probe`    | `ff-probe`    | yes     |
| `decode`   | `ff-decode`   | yes     |
| `encode`   | `ff-encode`   | yes     |
| `filter`   | `ff-filter`   | no      |
| `pipeline` | `ff-pipeline` | no      |
| `stream`   | `ff-stream`   | no      |

## Planned Usage

```toml
[dependencies]
# Default features: probe + decode + encode
ff = "0.5"

# With filter graph and pipeline support
ff = { version = "0.5", features = ["filter", "pipeline"] }

# Full feature set
ff = { version = "0.5", features = ["filter", "pipeline", "stream"] }
```

```rust,ignore
use ff::prelude::*;

let info = ff::probe::open("input.mp4")?;
println!("duration: {:?}", info.duration());
```

## Minimum Supported Rust Version

Rust 1.93.0 or later (edition 2024).

## The ff-* Crate Family

| Crate          | Description                              |
|----------------|------------------------------------------|
| `ff-sys`       | Raw FFmpeg FFI bindings (bindgen)        |
| `ff-common`    | Shared pool abstractions                 |
| `ff-format`    | Shared type system (codecs, frames, ...) |
| `ff-probe`     | Metadata and chapter extraction          |
| `ff-decode`    | Video/audio/image decoding               |
| `ff-encode`    | Video/audio encoding                     |
| `ff-filter`    | Filter graph (libavfilter)               |
| `ff-pipeline`  | Unified decode-filter-encode pipeline    |
| `ff-stream`    | HLS/DASH streaming output                |
| `ff`           | Facade crate (this crate)                |

## License

MIT OR Apache-2.0
