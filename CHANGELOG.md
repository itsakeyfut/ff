# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.6.0] - 2026-03-18

### Added

#### ff-decode — async API (tokio feature)
- `tokio` feature flag: opt-in async support that does not affect the default sync API ([#176](https://github.com/itsakeyfut/avio/issues/176))
- `AsyncVideoDecoder`: async wrapper around `VideoDecoder` backed by `tokio::task::spawn_blocking`; exposes `open()`, `decode_frame()`, and `into_stream()` returning `impl Stream<Item = Result<VideoFrame, DecodeError>> + Send` ([#177](https://github.com/itsakeyfut/avio/issues/177), [#178](https://github.com/itsakeyfut/avio/issues/178))
- `AsyncAudioDecoder`: async wrapper around `AudioDecoder` with the same three-method API ([#179](https://github.com/itsakeyfut/avio/issues/179))
- `AsyncImageDecoder`: async wrapper around `ImageDecoder`; exposes `open()` and `decode()` (no stream — single-frame by design)
- Compile-time `Send` bounds asserted for all async decoder types ([#180](https://github.com/itsakeyfut/avio/issues/180))
- Integration tests: async video decode frame count matches sync; async audio decode sample count matches sync ([#185](https://github.com/itsakeyfut/avio/issues/185), [#186](https://github.com/itsakeyfut/avio/issues/186))
- CI job `no-tokio-feature`: builds, tests, and lints the workspace with `--no-default-features` to verify the sync API is unaffected ([#188](https://github.com/itsakeyfut/avio/issues/188))

#### ff-encode — async API (tokio feature)
- `tokio` feature flag in `ff-encode` ([#181](https://github.com/itsakeyfut/avio/issues/181))
- `AsyncVideoEncoder`: frames are queued into a bounded `tokio::sync::mpsc` channel (capacity 8) consumed by a dedicated worker thread; `push().await` suspends automatically when the channel is full (back-pressure); `finish().await` sends a `WorkerMsg::Finish` sentinel, drops the sender, and joins the worker via `spawn_blocking` ([#182](https://github.com/itsakeyfut/avio/issues/182), [#184](https://github.com/itsakeyfut/avio/issues/184))
- `AsyncAudioEncoder`: identical pattern for audio frames ([#183](https://github.com/itsakeyfut/avio/issues/183))
- Integration tests: async video/audio encode produces valid output; back-pressure test ([#187](https://github.com/itsakeyfut/avio/issues/187))

#### avio — tokio feature
- `tokio` feature flag forwarding to `ff-decode/tokio` and `ff-encode/tokio`; re-exports all five async types (`AsyncVideoDecoder`, `AsyncAudioDecoder`, `AsyncImageDecoder`, `AsyncVideoEncoder`, `AsyncAudioEncoder`) under `#[cfg(feature = "tokio")]` ([#593](https://github.com/itsakeyfut/avio/issues/593))

#### Examples (async)
- `async_decode_video`: frame-by-frame, `into_stream()` combinators, and `tokio::spawn` patterns ([#594](https://github.com/itsakeyfut/avio/issues/594))
- `async_decode_audio`: same three patterns for audio, including sample counting via `fold` ([#595](https://github.com/itsakeyfut/avio/issues/595))
- `async_decode_image`: single decode and parallel multi-image decode via `futures::future::join_all` ([#596](https://github.com/itsakeyfut/avio/issues/596))
- `async_encode_video`: basic async encode loop and producer/consumer pattern with `mpsc` channel ([#597](https://github.com/itsakeyfut/avio/issues/597))
- `async_encode_audio`: same two patterns for audio, with real-time audio source motivation ([#598](https://github.com/itsakeyfut/avio/issues/598))
- `async_transcode`: end-to-end `AsyncVideoDecoder` → `AsyncVideoEncoder` in sequential and concurrent flavours; flagship async example ([#599](https://github.com/itsakeyfut/avio/issues/599))

### Changed

#### avio — documentation
- `## Encode` section in crate-level docs expanded into a three-part decision guide: when to use `Pipeline`, `VideoEncoder`/`AudioEncoder` directly, or the async encoders; each section includes bullet criteria, a code snippet, and cross-references to relevant examples

---

## [0.5.0] - 2026-03-16

### Added

#### ff-stream (new crate)
- `StreamError` enum with `InvalidConfig`, `Io`, `Encode`, and `Ffmpeg { code, message }` variants ([#69](https://github.com/itsakeyfut/avio/issues/69))
- `HlsOutput` consuming builder: configures and writes an HLS segmented stream via the FFmpeg HLS muxer, producing `playlist.m3u8` and numbered `.ts` segments ([#70](https://github.com/itsakeyfut/avio/issues/70), [#71](https://github.com/itsakeyfut/avio/issues/71))
- `DashOutput` consuming builder: configures and writes a DASH segmented stream via the FFmpeg DASH muxer, producing `manifest.mpd` and `.m4s` segments ([#72](https://github.com/itsakeyfut/avio/issues/72), [#73](https://github.com/itsakeyfut/avio/issues/73))
- `Rendition` struct describing a single resolution/bitrate quality level ([#74](https://github.com/itsakeyfut/avio/issues/74))
- `AbrLadder` builder: multi-rendition HLS output via `hls()` (produces master playlist + per-rendition subdirectories) and multi-representation DASH output via `dash()` (single `manifest.mpd` with multiple `Representation` elements) ([#75](https://github.com/itsakeyfut/avio/issues/75), [#76](https://github.com/itsakeyfut/avio/issues/76), [#77](https://github.com/itsakeyfut/avio/issues/77))
- Integration tests: `HlsOutput::write()` verifying playlist tags, segment naming, and target duration ([#84](https://github.com/itsakeyfut/avio/issues/84))
- Integration tests: `AbrLadder::hls()` verifying master playlist, per-rendition playlists, and `.ts` segments; `AbrLadder::dash()` verifying single-manifest DASH output ([#85](https://github.com/itsakeyfut/avio/issues/85))

#### avio (new crate)
- Facade crate that re-exports the public APIs of all `ff-*` crates behind feature flags (`probe`, `decode`, `encode`, `filter`, `pipeline`, `stream`) ([#78](https://github.com/itsakeyfut/avio/issues/78)–[#82](https://github.com/itsakeyfut/avio/issues/82))
- Integration tests verifying the facade compiles and resolves symbols for each feature combination ([#86](https://github.com/itsakeyfut/avio/issues/86))

#### ff-common
- `VecPool`: canonical `Arc`-based frame buffer pool backed by a `Mutex<Vec<Vec<u8>>>`; exposes `capacity()` and `available()` ([#502](https://github.com/itsakeyfut/avio/issues/502))
- `SimpleFramePool` type alias for `VecPool` for backwards compatibility

#### ff-format
- `DnxHd` variant added to `VideoCodec` ([#497](https://github.com/itsakeyfut/avio/issues/497))

### Changed

#### ff-encode
- `Progress` renamed to `EncodeProgress`; `ProgressCallback` renamed to `EncodeProgressCallback` to avoid collision with `ff-pipeline`'s `Progress`/`ProgressCallback` ([#500](https://github.com/itsakeyfut/avio/issues/500))
- `VideoCodec` and `AudioCodec` are no longer defined in `ff-encode`; they are re-exported from `ff-format` ([#498](https://github.com/itsakeyfut/avio/issues/498))

#### ff-pipeline
- `EncoderConfig` fields `video_codec` and `audio_codec` now use the canonical `ff-format` types ([#499](https://github.com/itsakeyfut/avio/issues/499))

#### ff-decode
- `SimpleFramePool` moved from `ff-decode` to `ff-common`; `ff-decode` re-exports it for backwards compatibility ([#502](https://github.com/itsakeyfut/avio/issues/502))

### Fixed

#### All crates
- `Ffmpeg` error variant unified to `Ffmpeg { code: i32, message: String }` (struct variant) across `ff-probe`, `ff-decode`, `ff-encode`, `ff-filter`, `ff-pipeline`, and `ff-stream` ([#486](https://github.com/itsakeyfut/avio/issues/486))

---

## [0.4.0] - 2026-03-16

### Added

#### ff-pipeline
- `Pipeline::run()` for single-input transcode: decode → optional filter graph → encode with progress tracking ([#58](https://github.com/itsakeyfut/avio/issues/58))
- Multi-input concatenation in `Pipeline::run()`: sequential inputs with PTS offset stitching ([#59](https://github.com/itsakeyfut/avio/issues/59))
- Audio stream handling in `Pipeline::run()`: audio decoded and encoded in parallel with video; silently skipped when no audio stream is present ([#60](https://github.com/itsakeyfut/avio/issues/60))
- Cancellation via `ProgressCallback`: returning `false` from the callback aborts the pipeline and returns `PipelineError::Cancelled` ([#61](https://github.com/itsakeyfut/avio/issues/61))
- `ThumbnailPipeline`: extracts a `VideoFrame` at each caller-specified timestamp using `VideoDecoder::seek` + `decode_one` ([#62](https://github.com/itsakeyfut/avio/issues/62))
- `parallel` Cargo feature for `ThumbnailPipeline`: when enabled, each timestamp is decoded in its own rayon thread; results are returned in ascending timestamp order ([#63](https://github.com/itsakeyfut/avio/issues/63))
- Integration tests: single-input transcode verifying non-zero output duration and progress callback invocation ([#64](https://github.com/itsakeyfut/avio/issues/64))
- Integration tests: multi-input concatenation verifying output duration ≈ sum of input durations ([#65](https://github.com/itsakeyfut/avio/issues/65))
- Integration tests: cancellation after first progress callback ([#66](https://github.com/itsakeyfut/avio/issues/66))
- Integration tests: `ThumbnailPipeline` frame count and dimension verification, sequential and parallel ([#67](https://github.com/itsakeyfut/avio/issues/67))

---

## [0.3.0] - 2026-03-14

### Added

#### ff-encode
- `ImageEncoder`: encodes a single `VideoFrame` to JPEG, PNG, or BMP using the FFmpeg image2 muxer ([#152](https://github.com/itsakeyfut/avio/issues/152))
- `ImageEncoderBuilder` options: `width`, `height`, `quality`, and `pixel_format` ([#153](https://github.com/itsakeyfut/avio/issues/153))
- `ImageEncoderInner` with RAII `Drop` for safe cleanup of FFmpeg resources ([#154](https://github.com/itsakeyfut/avio/issues/154))
- `BitrateMode` enum (`Cbr` / `Vbr`) for video bitrate control; `Vbr` wired to `AVCodecContext` ([#44](https://github.com/itsakeyfut/avio/issues/44), [#46](https://github.com/itsakeyfut/avio/issues/46))
- Two-pass video encoding via `VideoEncoderBuilder::two_pass()` ([#43](https://github.com/itsakeyfut/avio/issues/43))
- Metadata write support via `VideoEncoderBuilder::metadata()` ([#45](https://github.com/itsakeyfut/avio/issues/45))
- Chapter write support via `VideoEncoderBuilder::chapters()` ([#47](https://github.com/itsakeyfut/avio/issues/47))
- Subtitle passthrough via `VideoEncoderBuilder::subtitle_passthrough()` ([#48](https://github.com/itsakeyfut/avio/issues/48))
- Integration tests: `ImageEncoder` file creation, round-trip decode, quality, and drop-without-encode ([#155](https://github.com/itsakeyfut/avio/issues/155))
- Integration tests: chapter round-trip ([#52](https://github.com/itsakeyfut/avio/issues/52)) and subtitle passthrough round-trip ([#53](https://github.com/itsakeyfut/avio/issues/53))

#### ff-probe
- `SubtitleCodec` enum covering common subtitle codecs (ASS, SRT, WebVTT, HDMV PGS, DVB, MOV text, TTML) ([#49](https://github.com/itsakeyfut/avio/issues/49))
- `SubtitleStreamInfo` struct with `codec`, `language`, `title`, and `default_stream` accessors ([#50](https://github.com/itsakeyfut/avio/issues/50))
- `subtitle_streams()`, `has_subtitles()`, and `subtitle_stream_count()` on `MediaInfo` ([#51](https://github.com/itsakeyfut/avio/issues/51))
- Integration tests for subtitle stream probing

### Changed

#### ff-encode
- Module structure restructured to match the `ff-decode` pattern (`video/`, `audio/`, `image/`) ([#151](https://github.com/itsakeyfut/avio/issues/151))

### Fixed

#### ff-encode
- Subtitle packets with `AV_NOPTS_VALUE` DTS now have DTS mirrored from PTS before `av_interleaved_write_frame`; prevents silent packet drops by the Matroska muxer

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
