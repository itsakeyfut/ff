# ff-stream

![Status](https://img.shields.io/badge/status-in%20development-yellow)

`ff-stream` will provide `HlsOutput`, `DashOutput`, and `AbrLadder` for producing adaptive bitrate streaming content from any video source. Currently a placeholder — the API is under design.

## Planned API

```rust,ignore
use ff_stream::{HlsOutput, AbrLadder, Rendition};

// Define an ABR ladder: multiple quality renditions from one input.
let ladder = AbrLadder::builder()
    .rendition(Rendition::new(1920, 1080, 6_000_000))
    .rendition(Rendition::new(1280, 720,  3_000_000))
    .rendition(Rendition::new(854,  480,  1_500_000))
    .rendition(Rendition::new(640,  360,    800_000))
    .build()?;

// Write an HLS package to a directory.
let output = HlsOutput::builder()
    .input("source.mp4")
    .output_dir("hls_output/")
    .segment_duration(6)
    .ladder(ladder)
    .build()?;

output.run()?;
// Writes hls_output/master.m3u8 and per-rendition segment files.
```

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0
