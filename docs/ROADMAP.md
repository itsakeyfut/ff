# ff Roadmap

## Overview

The **ff** crate family aims to provide **safe, ergonomic video and audio processing in Rust**, built on top of FFmpeg's libav* libraries via `ff-sys`.

All unsafe FFmpeg calls are isolated in `*_inner.rs` files. Public APIs are fully safe.

### Target Users

- **Video editing software developers** — need filters, trimming, overlays, transitions
- **Streaming service developers** — need HLS/DASH output, real-time encoding, multi-bitrate ladders

---

## Current State — v0.1.0

### Published Crates

| Crate | Description |
|---|---|
| `ff-sys` | Raw FFmpeg bindings |
| `ff-common` | Shared types and error handling |
| `ff-format` | Container demux/mux |
| `ff-probe` | Media metadata extraction |
| `ff-decode` | Video/audio decoding, hardware acceleration |
| `ff-encode` | Video/audio encoding, hardware acceleration, thumbnail generation |
| `ff-filter` | Filter graph *(placeholder — not yet implemented)* |

### What Works Today

- [x] Metadata extraction (codec, resolution, duration, bitrate, streams)
- [x] Video/audio decoding
- [x] Video/audio encoding
- [x] Hardware acceleration (NVENC, QSV, VideoToolbox, VAAPI)
- [x] Thumbnail generation

### What Is Not Yet Supported

- Filter graphs (trim, scale, crop, overlay, fade, etc.)
- Multi-pass encoding
- Subtitles and chapter read/write
- HLS / DASH output
- Multi-input composition

---

## Milestones

---

### v0.1.x — Stabilization & Quality

**Goal**: Fix known issues and fill small gaps in the existing API surface.

- [ ] `ff-encode`: CRF (quality-based) encoding control
- [ ] `ff-decode`: Fix channel layout detection (`decoder_inner.rs` TODO)
- [ ] `ff-probe`: Chapter information retrieval API

---

### v0.2.0 — ff-filter Core *(highest priority)*

**Goal**: Implement the filter graph engine that video editing applications depend on.

**Target users**: Video editing software developers

All unsafe libavfilter calls are isolated in `filter_inner.rs`.

#### Video Filters

- [ ] `trim` — cut to time range
- [ ] `scale` — resize
- [ ] `crop` — crop region
- [ ] `overlay` — composite video over video
- [ ] `fade` — fade in / fade out
- [ ] `rotate` — rotation

#### Audio Filters

- [ ] `volume` — gain adjustment
- [ ] `amix` — mix multiple audio streams
- [ ] `equalizer` — frequency band control

#### API Design

```rust
let graph = FilterGraph::builder()
    .trim(10.0, 30.0)
    .scale(1280, 720)
    .fade_in(Duration::from_secs(1))
    .build()?;

let frame = graph.process_video(&input_frame)?;
```

---

### v0.3.0 — Encoding Enhancements & Metadata Write

**Goal**: Production-quality encoding control and full metadata round-trip.

**Target users**: Video editing software developers

- [ ] Multi-pass encoding (2-pass)
- [ ] VBR (variable bitrate)
- [ ] Metadata write (title, artist, comment, etc.)
- [ ] Chapter information write
- [ ] Subtitle stream read and write

---

### v0.4.0 — Streaming Output

**Goal**: Support adaptive streaming formats required by streaming services.

**Target users**: Streaming service developers

- [ ] HLS (HTTP Live Streaming) segment output
- [ ] DASH (Dynamic Adaptive Streaming over HTTP)
- [ ] Keyframe interval control
- [ ] Multi-bitrate output (ABR ladder)

---

### v0.5.0 — Unified Pipeline & Advanced Features

**Goal**: High-level pipeline API and advanced composition capabilities.

**Target users**: Both target groups

- [ ] `Pipeline` API — unified decode → filter → encode chain
- [ ] Multi-input support — concatenation and compositing of multiple sources
- [ ] HDR/SDR tone mapping
- [ ] Parallel thumbnail generation

#### Pipeline API Design

```rust
let pipeline = Pipeline::builder()
    .input("input.mp4")
    .filter(graph)
    .output("output.mp4", encoder_config)
    .build()?;

pipeline.run()?;
```

---

### v1.0.0 — Stable API

**Goal**: Semver-stable public API suitable for production use.

- [ ] Semver API stability guarantee across all crates
- [ ] Complete documentation for all public items
- [ ] Production usage examples and cookbook

---

## Design Principles

- **Safe public API** — all `unsafe` is contained in `*_inner.rs` modules; callers never touch raw pointers
- **Explicit errors** — no panics in library code; all failure paths return `Result`
- **Zero unnecessary allocation** — frame data is borrowed where possible
- **Layered crates** — each crate depends only on lower layers; no circular dependencies

---

## Contributing

Contributions are welcome. If you want to work on an item above, please open an issue first to coordinate.

Areas where help is especially appreciated:

- `ff-filter` implementation (v0.2.0 scope)
- Testing on non-Linux platforms (macOS, Windows)
- Hardware acceleration coverage (VAAPI, VideoToolbox, AMF)
