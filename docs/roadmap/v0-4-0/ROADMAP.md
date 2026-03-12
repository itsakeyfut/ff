# v0.4.0 — ff-pipeline

**Goal**: Unified high-level pipeline that connects decode → filter → encode with progress tracking and multi-input support.

**Target users**: Both video editing and streaming developers

**Prerequisite**: v0.3.0 complete.

---

## Crates in scope

`ff-pipeline` (new crate)

---

## ff-pipeline

### API Design

```rust
let pipeline = Pipeline::builder()
    .input("input.mp4")
    .filter(graph)
    .output("output.mp4", encoder_config)
    .on_progress(|p| println!("{:.1}%", p.percent()))
    .build()?;

pipeline.run()?;
```

### Features

- [ ] Connects `ff-decode` → `ff-filter` → `ff-encode`
- [ ] Audio + video stream handling (both passed through the chain)
- [ ] Unified progress tracking via `ProgressCallback`
- [ ] Cancellation: `ProgressCallback::should_cancel()` returns `true` to abort

### Multi-input (Concatenation)

```rust
Pipeline::builder()
    .input("part1.mp4")
    .input("part2.mp4")
    .output("combined.mp4", encoder_config)
    .build()?;
```

- [ ] `.input()` can be called multiple times for concatenation
- [ ] Order of inputs determines playback order

### Parallel Thumbnails

```rust
ThumbnailPipeline::new("input.mp4")
    .timestamps(vec![10.0, 30.0, 60.0])
    .run()?; // returns Vec<VideoFrame>
```

- [ ] `ThumbnailPipeline` using `rayon` (optional feature flag: `parallel`)
- [ ] Feature flag: `ff-pipeline = { features = ["parallel"] }`

### Progress API

```rust
pub struct Progress {
    pub frames_processed: u64,
    pub total_frames: Option<u64>,
    pub elapsed: Duration,
}

impl Progress {
    pub fn percent(&self) -> f64 { ... }
}
```

---

## Design Decisions

| Topic | Decision |
|---|---|
| Async | Sync only — `pipeline.run()` blocks until complete |
| Cancellation | Via `ProgressCallback` return value, not channels |
| Parallel thumbnails | Optional `rayon` feature, not enabled by default |

---

## Definition of Done

- All checkboxes above checked
- Single-input transcode integration test
- Multi-input concatenation integration test
- `ThumbnailPipeline` test (with `parallel` feature)
- `cargo test --workspace` passes
