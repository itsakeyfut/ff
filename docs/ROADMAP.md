# ff Roadmap

## Overview

The **ff** crate family aims to provide **safe, ergonomic video and audio processing in Rust**, built on top of FFmpeg's libav* libraries via `ff-sys`.

All unsafe FFmpeg calls are isolated in `*_inner.rs` files. Public APIs are fully safe.

### Target Users

- **Video editing software developers** — need filters, trimming, overlays, transitions
- **Streaming service developers** — need HLS/DASH output, real-time encoding, multi-bitrate ladders

---

## Crate Family

| Crate | When | Description |
|---|---|---|
| `ff-sys` | v0.1.x | Raw FFmpeg FFI bindings |
| `ff-common` | v0.1.x | Shared pool abstraction |
| `ff-format` | v0.1.x | Type system (codecs, frames, formats) |
| `ff-probe` | v0.1.x | Metadata + chapter extraction |
| `ff-decode` | v0.1.x | Video/audio/image decoding, thumbnails |
| `ff-encode` | v0.1.x | Video/audio encoding |
| `ff-filter` | v0.2.0 | Filter graph (libavfilter) |
| `ff-pipeline` | v0.4.0 | decode→filter→encode unified pipeline |
| `ff-stream` | v0.5.0 | HLS/DASH streaming output |
| `ff` | v0.5.0 | Facade crate (re-exports all) |

---

## Design Principles

- **Safe public API** — all `unsafe` is contained in `*_inner.rs` modules; callers never touch raw pointers
- **Explicit errors** — no panics in library code; all failure paths return `Result`
- **Sync only** — no async/tokio dependency
- **Structured logging** — `log` crate facade; consumers choose the backend
- **Layered crates** — each crate depends only on lower layers; no circular dependencies

---

## Milestones

| Version | Status | Details |
|---|---|---|
| [v0.1.x](roadmap/v0-1-x/ROADMAP.md) | In Progress | Stabilization & quality fixes |
| [v0.2.0](roadmap/v0-2-0/ROADMAP.md) | Planned | ff-filter + still image decoding |
| [v0.3.0](roadmap/v0-3-0/ROADMAP.md) | Planned | Encoding enhancements + metadata write |
| [v0.4.0](roadmap/v0-4-0/ROADMAP.md) | Planned | ff-pipeline unified pipeline |
| [v0.5.0](roadmap/v0-5-0/ROADMAP.md) | Planned | ff-stream + ff facade |
| [v1.0.0](roadmap/v1-0-0/ROADMAP.md) | Planned | Stable API |

---

## Contributing

Contributions are welcome. If you want to work on an item above, please open an issue first to coordinate.

Areas where help is especially appreciated:

- `ff-filter` implementation (v0.2.0 scope)
- Testing on non-Linux platforms (macOS, Windows)
- Hardware acceleration coverage (VAAPI, VideoToolbox, AMF)
