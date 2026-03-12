# v0.3.0 — Encoding Enhancements & Metadata Write

**Goal**: Production-quality encoding control and full metadata round-trip, including chapter and subtitle support.

**Target users**: Video editing software developers

**Prerequisite**: v0.2.0 complete.

---

## Crates in scope

`ff-encode`, `ff-probe`

---

## ff-encode

### Multi-pass Encoding

- [ ] `.two_pass()` builder method
- [ ] First pass writes to `/dev/null` (stats only); second pass produces output

### Variable Bitrate Modes

```rust
pub enum BitrateMode {
    Cbr(u64),
    Vbr { target: u64, max: u64 },
    Crf(u32),
}
```

- [ ] `BitrateMode` enum
- [ ] Wire `BitrateMode::Crf` to `AVCodecContext.crf`
- [ ] Wire `BitrateMode::Vbr` to `rc_min_rate` / `rc_max_rate`

### Metadata Write

- [ ] `.metadata(key, value)` — calls `av_dict_set` on `AVFormatContext`
- [ ] `.chapter(ChapterInfo)` — write chapter list to output container

### Subtitle Passthrough

- [ ] Copy subtitle streams without re-encoding
- [ ] API: `.subtitle_passthrough(source_path, stream_index)`

---

## ff-probe

### Subtitle Stream Info

```rust
pub enum SubtitleCodec {
    Srt,
    Ass,
    Dvb,
    Hdmv,
    Webvtt,
}

pub struct SubtitleStreamInfo {
    pub index: usize,
    pub codec: SubtitleCodec,
    pub language: Option<String>,
    pub title: Option<String>,
}
```

- [ ] `SubtitleCodec` enum
- [ ] `SubtitleStreamInfo` struct
- [ ] `subtitle_streams: Vec<SubtitleStreamInfo>` added to `MediaInfo`
- [ ] Extract from `AVStream` entries with `AVMEDIA_TYPE_SUBTITLE`

---

## Design Decisions

| Topic | Decision |
|---|---|
| Subtitle | Read in ff-probe; passthrough encode in ff-encode |
| Chapter write | `ChapterInfo` type shared from ff-probe |

---

## Definition of Done

- All checkboxes above checked
- Round-trip test: probe chapters → write chapters → probe output matches
- Subtitle passthrough integration test
- `cargo test --workspace` passes
