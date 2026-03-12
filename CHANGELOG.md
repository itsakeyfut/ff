# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.1.3] - 2026-03-13

### Fixed

#### ff-sys
- docs.rs builds now succeed for all crates. `build.rs` detects the `DOCS_RS`
  environment variable and writes empty bindings, emitting `cfg(docsrs)`.
  A new `docsrs_stubs.rs` file provides shape-compatible stub types, constants,
  functions, and wrapper modules so that `ff-probe`, `ff-decode`, and `ff-encode`
  compile on docs.rs without any changes to those crates ([#125](https://github.com/itsakeyfut/avio/pull/125))
- All crates now carry `[package.metadata.docs.rs]` with
  `rustdoc-args = ["--cfg", "docsrs"]` ([#125](https://github.com/itsakeyfut/avio/pull/125))

---

## [0.1.2] - 2026-03-12

### Added

#### ff-format
- New `ChapterInfo` and `ChapterInfoBuilder` types for representing chapter markers within a media container (MKV, MP4, M4A, etc.). Fields: `id`, `title`, `start`, `end`, `time_base`, `metadata` ([#12](https://github.com/itsakeyfut/avio/pull/123))
- `MediaInfo` now exposes `chapters()`, `has_chapters()`, and `chapter_count()` accessors, plus corresponding builder methods `.chapter()` / `.chapters()` ([#13](https://github.com/itsakeyfut/avio/pull/123))

#### ff-probe
- Chapter extraction from `AVFormatContext`: iterates `nb_chapters`, converts each `AVChapter` (including `AVRational`-based timestamps) into `ChapterInfo` and populates `MediaInfo::chapters()` ([#14](https://github.com/itsakeyfut/avio/pull/123))
- Re-exports `ChapterInfo` and `ChapterInfoBuilder` from the public API and prelude

### Fixed

#### ff-encode
- `av_opt_set` return values for CRF and preset options are now checked; a `log::warn!` is emitted and the encode continues with the encoder default when an option is unsupported ([#10](https://github.com/itsakeyfut/avio/pull/122))

#### ff-decode
- Channel layout detection now reads `AVChannelLayout.u.mask` when `order == AV_CHANNEL_ORDER_NATIVE`, correctly distinguishing layouts that share a channel count (e.g. `Stereo2_1` vs `Surround3_0`). Falls back to channel-count heuristic with a `log::warn!` for unrecognised masks or non-native order ([#11](https://github.com/itsakeyfut/avio/pull/119))

### Changed

#### ff-decode, ff-encode, ff-probe, ff-format
- Silent fallbacks that previously discarded unsupported values without notice now emit `log::warn!` with `key=value` diagnostics ([#5–#9](https://github.com/itsakeyfut/avio/pull/116))

---

## [0.1.1] - 2026-03-11

### Fixed

#### ff-sys
- Raise minimum required FFmpeg version from 6.0 to 7.0. FFmpeg 7.x converted
  scaling flags from `#define` macros to `enum SwsFlags`, making 6.x
  incompatible with the bindgen-generated `SwsFlags_SWS_*` constants ([#1](https://github.com/itsakeyfut/avio/pull/1))

---

## [0.1.0] - 2026-03-11

### Added

#### ff-sys
- Raw FFmpeg FFI bindings via bindgen
- Platform-specific build support: pkg-config (Linux), Homebrew (macOS), vcpkg (Windows)

#### ff-common
- Shared error types and common utilities

#### ff-format
- Container demux and mux support

#### ff-probe
- Media metadata extraction (codec, resolution, duration, bitrate, streams)

#### ff-decode
- Video and audio decoding
- Hardware acceleration support (NVENC, QSV, VideoToolbox, VAAPI)

#### ff-encode
- Video and audio encoding
- Hardware acceleration support
- Thumbnail generation

#### ff-filter
- Placeholder crate (filter graph not yet implemented)
