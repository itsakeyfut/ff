# v0.1.x — Stabilization & Quality

**Goal**: Fix known issues and fill small gaps in the existing API surface before the first filter release.

v0.1.x must complete before v0.2.0 begins.

---

## Crates in scope

`ff-sys`, `ff-common`, `ff-format`, `ff-probe`, `ff-decode`, `ff-encode`

---

## Tasks

### All crates

- [ ] Add `log = "0.4"` to workspace `Cargo.toml`
- [ ] Replace silent fallbacks with `log::warn!()`

### ff-decode

- [ ] Fix channel layout detection via `AVChannelLayout`
  - File: `ff-decode/src/audio/decoder_inner.rs:487`
  - Return proper `ChannelLayout` for 5.1, 7.1, etc.

### ff-encode

- [ ] Verify/complete CRF quality-based encoding
  - `.video_quality()` must be wired to `AVCodecContext.crf`
  - File: `ff-encode/src/video/encoder_inner.rs`
- [ ] Add `log::warn!()` for pixel format fallback
  - Replaces the TODO at line 1306 in `encoder_inner.rs`

### ff-probe

- [ ] New type: `ChapterInfo { id, title, start: Duration, end: Duration }`
- [ ] Add `chapters: Vec<ChapterInfo>` to `MediaInfo`
- [ ] Extract chapters from `AVFormatContext.chapters`
  - File: `ff-probe/src/info.rs`

---

## Design Decisions

| Topic | Decision |
|---|---|
| Logging | `log` crate (lightweight facade); consumers choose the backend |
| Async | Sync only — no tokio dependency |
| Error strategy | Each crate owns its error type; re-exported by `ff` facade in v0.5.0 |

---

## Definition of Done

- All checkboxes above checked
- `cargo test --workspace` passes
- No `eprintln!` or silent fallbacks remain in the crates listed above
