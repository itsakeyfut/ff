# ff-stream

Produce HLS and DASH adaptive bitrate output from any video source. Define a rendition ladder,
point it at an input file, and receive a standards-compliant package ready for CDN delivery.

> **Project status (as of 2026-03-26):** This crate is in an early phase. The high-level API is designed and reviewed by hand; AI is used as an accelerator to implement FFmpeg bindings efficiently. Code contributions are not expected at this time — questions, bug reports, and feature requests are welcome. See the [main repository](https://github.com/itsakeyfut/avio) for full context.

## Installation

```toml
[dependencies]
ff-stream = "0.12"
```

## HLS Output

```rust
use ff_stream::{HlsOutput, AbrLadder, Rendition};

let ladder = AbrLadder::new("source.mp4")
    .rendition(Rendition { width: 1920, height: 1080, bitrate: 6_000_000 })
    .rendition(Rendition { width: 1280, height: 720,  bitrate: 3_000_000 })
    .rendition(Rendition { width: 854,  height: 480,  bitrate: 1_500_000 })
    .rendition(Rendition { width: 640,  height: 360,  bitrate:   800_000 });

let output = HlsOutput::new("hls_output/");
output.run(&ladder)?;
// Writes hls_output/master.m3u8 and per-rendition segment directories.
```

## DASH Output

```rust
use ff_stream::{DashOutput, AbrLadder, Rendition};

let ladder = AbrLadder::new("source.mp4")
    .rendition(Rendition { width: 1920, height: 1080, bitrate: 6_000_000 })
    .rendition(Rendition { width: 1280, height: 720,  bitrate: 3_000_000 });

let output = DashOutput::new("dash_output/");
output.run(&ladder)?;
// Writes dash_output/manifest.mpd and per-rendition segment directories.
```

## Rendition Ladder

`AbrLadder` defines the set of quality levels to produce. Each `Rendition` specifies the
output resolution and target bitrate. The ladder is shared by both `HlsOutput` and `DashOutput`.

| Field | Type | Description |
|---|---|---|
| `width` | `u32` | Output frame width in pixels |
| `height` | `u32` | Output frame height in pixels |
| `bitrate` | `u64` | Target video bitrate in bits per second |

## Error Handling

| Variant | When it occurs |
|---|---|
| `StreamError::InvalidConfig` | Missing input, empty ladder, or conflicting options |
| `StreamError::Encode` | Wrapped `EncodeError` from a rendition encode stage |
| `StreamError::Io` | Write failure on the output directory |

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0
