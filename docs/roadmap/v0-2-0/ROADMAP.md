# v0.2.0 — ff-filter Core

**Goal**: Implement the filter graph engine that video editing applications depend on, plus still image decoding.

**Target users**: Video editing software developers

**Prerequisite**: v0.1.x complete.

---

## Crates in scope

`ff-filter` (new implementation), `ff-decode` (image module)

---

## ff-filter

### API Design

`FilterGraph` uses a push/pull stateful model with support for multiple named input slots.

```rust
// Single-stream pipeline
let graph = FilterGraph::builder()
    .trim(10.0, 30.0)
    .scale(1280, 720)
    .fade_in(Duration::from_secs(1))
    .build()?;

graph.push_video(0, &frame)?;
let out = graph.pull_video()?;

// Multi-stream (overlay)
let graph = FilterGraph::builder()
    .overlay(10, 10)
    .build()?;

graph.push_video(0, &base)?;
graph.push_video(1, &overlay)?;
let out = graph.pull_video()?;
```

API is type-safe builder only — no raw string filter chains in the public API.

### Video Filters

- [ ] `trim(start, end)` — drop out-of-range frames, rewrite timestamps
- [ ] `scale(w, h)` — resize
- [ ] `crop(x, y, w, h)` — crop region
- [ ] `overlay(x, y)` — 2-input composite
- [ ] `fade_in(duration)` / `fade_out(duration)`
- [ ] `rotate(degrees)`
- [ ] `tone_map(ToneMap)` — HDR→SDR (Hable, Reinhard, Mobius)

### Audio Filters

- [ ] `volume(gain_db)` — gain adjustment
- [ ] `amix(inputs)` — N-input mix
- [ ] `equalizer(band, gain_db)` — frequency band control

### Hardware Filtering

- [ ] CUDA
- [ ] VideoToolbox
- [ ] VAAPI

### Error Type

```rust
pub enum FilterError {
    BuildFailed,
    ProcessFailed,
    InvalidInput,
    Ffmpeg(String),
}
```

### Internals

- All unsafe code lives in `filter_inner.rs`
- `FilterGraph` holds `*mut AVFilterGraph`, freed on drop via `avfilter_graph_free`
- Builder validates the graph at `.build()` time, not per-filter

---

## ff-decode: Still Image Decoding

New module: `ff-decode/src/image/`

```rust
let decoder = ImageDecoder::open("photo.jpg")?;
let frame: VideoFrame = decoder.decode()?;
```

- [ ] `ImageDecoder::open(path) -> Result<ImageDecoder>`
- [ ] `ImageDecoder::decode() -> Result<VideoFrame>`
- [ ] Supported formats: JPEG, PNG, BMP, TIFF, WebP
- [ ] Thumbnail API remains in `ff-decode` (no change)

---

## Design Decisions

| Topic | Decision |
|---|---|
| ff-filter API | Type-safe builder only — no raw lavfi strings exposed |
| Overlay model | `FilterGraph` with multiple input slots (push index, pull) |
| HDR tone mapping | ff-filter video filter (`tone_map`), this milestone |
| Still image | `ff-decode::ImageDecoder` |

---

## Definition of Done

- All checkboxes above checked
- `FilterGraph` single-stream and multi-stream (overlay) integration tests pass
- `ImageDecoder` decodes JPEG and PNG in tests
- `cargo test --workspace` passes
