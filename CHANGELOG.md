# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.2.0] - 2026-03-14

### Added

#### ff-filter (new crate)
- Full filter graph implementation: `FilterGraph`, `FilterGraphBuilder`, `FilterGraphInner`, and `FilterError` types ([#15](https://github.com/itsakeyfut/avio/issues/15)–[#20](https://github.com/itsakeyfut/avio/issues/20))
- `AVFilterGraph` lifecycle management with RAII `Drop` ([#16](https://github.com/itsakeyfut/avio/issues/16))
- `buffersrc` / `buffersink` argument helpers for both video and audio ([#21](https://github.com/itsakeyfut/avio/issues/21))
- Multi-input slot management for video and audio filter chains ([#22](https://github.com/itsakeyfut/avio/issues/22))
- Graph validation via `avfilter_graph_config` with propagated error codes and messages ([#23](https://github.com/itsakeyfut/avio/issues/23))
- `push_video` / `pull_video` and `push_audio` / `pull_audio` API with correct PTS/DTS timestamp handling ([#18](https://github.com/itsakeyfut/avio/issues/18), [#19](https://github.com/itsakeyfut/avio/issues/19))
- Filter name validation at `build()` time ([#17](https://github.com/itsakeyfut/avio/issues/17))
- Video filters: `scale`, `crop`, `trim` (with `setpts=PTS-STARTPTS` timestamp reset), `overlay`, `fade_in`, `fade_out`, `rotate`, `tone_map` ([#24](https://github.com/itsakeyfut/avio/issues/24)–[#30](https://github.com/itsakeyfut/avio/issues/30))
- Audio filters: `volume`, `amix`, `equalizer` ([#31](https://github.com/itsakeyfut/avio/issues/31)–[#33](https://github.com/itsakeyfut/avio/issues/33))
- Hardware-accelerated filter chains: CUDA (`hwupload_cuda` / `hwdownload`), VideoToolbox, VAAPI ([#34](https://github.com/itsakeyfut/avio/issues/34)–[#36](https://github.com/itsakeyfut/avio/issues/36))
- Integration tests covering all single-stream, multi-stream, and hardware filter paths ([#40](https://github.com/itsakeyfut/avio/issues/40), [#41](https://github.com/itsakeyfut/avio/issues/41))

#### ff-decode
- `ImageDecoder`: decodes still images (JPEG, PNG, BMP, TIFF, WebP) into `VideoFrame` using FFmpeg ([#37](https://github.com/itsakeyfut/avio/issues/37)–[#39](https://github.com/itsakeyfut/avio/issues/39))
- Builder pattern: `ImageDecoder::open(path)` → `ImageDecoderBuilder` → `ImageDecoder` ([#38](https://github.com/itsakeyfut/avio/issues/38))
- `decode()` (consuming), `decode_one()` (incremental), and `frames()` (iterator) APIs ([#39](https://github.com/itsakeyfut/avio/issues/39))
- `ImageFrameIterator` for API consistency with `VideoFrameIterator` and `AudioFrameIterator`
- `ImageDecoder`, `ImageDecoderBuilder`, and `ImageFrameIterator` re-exported from crate root and `prelude`
- Integration tests covering JPEG and PNG decode (width, height, pixel format) and error handling ([#42](https://github.com/itsakeyfut/avio/issues/42))

### Fixed

#### ff-filter
- `AV_BUFFERSRC_FLAG_KEEP_REF` type normalized across platforms to prevent Windows/Linux build divergence
- Null `avfilter_get_by_name` return treated as unverifiable (skipped) rather than a build error at startup

#### ff-sys
- `docsrs_stubs.rs` completed for `ff-probe`, `ff-decode`, and `ff-encode` crates ([#127](https://github.com/itsakeyfut/avio/issues/127))

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
