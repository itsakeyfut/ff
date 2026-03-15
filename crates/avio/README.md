# avio

![Status](https://img.shields.io/badge/status-in%20development-yellow)

`avio` is the unified facade for the `ff-*` crate family. Depend on a single crate and opt in to only the capabilities you need via feature flags. Currently a placeholder — the facade re-exports are under development.

## Feature Flags

| Feature    | Enables                                   |
|------------|-------------------------------------------|
| `probe`    | `ff-probe` — read-only media metadata     |
| `decode`   | `ff-decode` — video and audio decoding    |
| `encode`   | `ff-encode` — video and audio encoding    |
| `filter`   | `ff-filter` — filter graph operations     |
| `pipeline` | `ff-pipeline` — decode → filter → encode  |
| `stream`   | `ff-stream` — HLS / DASH streaming output |

## Planned Usage

```toml,ignore
[dependencies]
avio = { version = "0.3", features = ["probe", "decode", "encode"] }
```

```rust,ignore
// With the "probe" and "decode" features enabled:
use avio::probe;
use avio::decode::VideoDecoder;
use avio::format::PixelFormat;

let info = probe::open("video.mp4")?;
println!("duration: {:?}", info.duration());

let mut decoder = VideoDecoder::open("video.mp4")?
    .output_format(PixelFormat::Rgba)
    .build()?;

while let Some(frame) = decoder.decode_frame()? {
    process(&frame);
}
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
