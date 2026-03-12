# v1.0.0 — Stable API

**Goal**: Semver-stable public API suitable for production use across all crates.

**Prerequisite**: v0.5.0 complete.

---

## Tasks

### API Stability

- [ ] Semver stability guarantee documented for all crates
- [ ] MSRV (Minimum Supported Rust Version) policy documented in `README.md`
- [ ] Breaking change process defined in `CONTRIBUTING.md`

### Documentation

- [ ] `#![warn(missing_docs)]` enforced on all crates
- [ ] All public items have doc comments
- [ ] Production cookbook: 10+ end-to-end examples covering:
  - Transcode a file
  - Extract thumbnails
  - Trim and scale with ff-filter
  - Overlay two video streams
  - 2-pass encode
  - Read and write metadata + chapters
  - Subtitle passthrough
  - HLS output
  - ABR ladder (multi-bitrate HLS)
  - Full pipeline (decode → filter → encode)

### Security & Maintenance

- [ ] `SECURITY.md` — vulnerability reporting policy
- [ ] `CHANGELOG.md` — complete history from v0.1.0 through v1.0.0

### Quality Gates

- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `cargo test --workspace` passes on Linux, macOS, Windows
- [ ] No `unwrap()` or `expect()` in library code (only in tests and examples)

---

## Definition of Done

- All checkboxes above checked
- Crates published to crates.io with `version = "1.0.0"`
- docs.rs renders all public items with documentation
