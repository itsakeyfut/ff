# v0.5.0 — ff-stream + ff Facade

**Goal**: Adaptive streaming output (HLS/DASH) and a unified facade crate that re-exports the entire family.

**Target users**: Streaming service developers

**Prerequisite**: v0.4.0 complete.

---

## Crates in scope

`ff-stream` (new crate), `ff` (new facade crate)

---

## ff-stream

### HLS Output

```rust
HlsOutput::new("out/")
    .segment_duration(Duration::from_secs(6))
    .keyframe_interval(60)
    .write()?;
```

- [ ] `HlsOutput` builder
- [ ] Segment duration control
- [ ] Keyframe interval control
- [ ] Writes `.m3u8` playlist and `.ts` segment files

### DASH Output

```rust
DashOutput::new("out/")
    .segment_duration(Duration::from_secs(4))
    .write()?;
```

- [ ] `DashOutput` builder
- [ ] Writes `manifest.mpd` and segment files

### ABR Ladder

```rust
AbrLadder::new("input.mp4")
    .add_rendition(Rendition { width: 1920, height: 1080, bitrate: 6_000_000 })
    .add_rendition(Rendition { width: 1280, height: 720,  bitrate: 3_000_000 })
    .add_rendition(Rendition { width: 854,  height: 480,  bitrate: 1_500_000 })
    .hls("out/")?;
```

- [ ] `Rendition` struct: `{ width, height, bitrate }`
- [ ] `AbrLadder` builder
- [ ] `.hls(out_dir)` — produces multi-variant HLS
- [ ] `.dash(out_dir)` — produces multi-representation DASH

---

## ff (Facade Crate)

### Usage

```toml
[dependencies]
ff = { version = "0.5", features = ["filter", "pipeline", "stream"] }
```

### Re-exports

- [ ] Re-exports all public types from each crate
- [ ] Error types re-exported as-is: `ff::DecodeError`, `ff::EncodeError`, `ff::FilterError`, etc.
- [ ] Feature flags map to individual crates:
  - `filter` → `ff-filter`
  - `pipeline` → `ff-pipeline`
  - `stream` → `ff-stream`
- [ ] Default features: `ff-probe`, `ff-decode`, `ff-encode`

---

## Design Decisions

| Topic | Decision |
|---|---|
| Streaming crate | `ff-stream` (new crate, this milestone) |
| Facade crate | `ff` (new crate, this milestone) |
| Error strategy | Each crate's error re-exported by `ff` facade as-is |

---

## Definition of Done

- All checkboxes above checked
- HLS output integration test (produces valid `.m3u8`)
- ABR ladder test (produces multi-variant playlist)
- `ff` facade compiles with all feature combinations
- `cargo test --workspace` passes
