# v0.10.0 — Multi-track Composition & Media Analysis

**Goal**: Enable multi-track video/audio composition, Clip/Timeline abstraction, media analysis
tools, thumbnail/preview generation, and export presets — the foundational layer for building a
non-linear editing workflow on top of the library.

**Prerequisite**: v0.9.0 complete.

**Crates in scope**: `ff-common`, `ff-format`, `ff-filter`, `ff-decode`, `ff-encode`, `ff-pipeline`

---

## Requirements

### Multi-track Video Composition

- Multiple video tracks can be composited with a configurable z-order, enabling picture-in-picture
  and side-by-side layouts.
- Each video track has configurable position (x, y), scale, and opacity at the composition stage.
- The output canvas size is configurable independently of any individual track's resolution.
- A solid color background can be used as the base layer when fewer tracks than z-levels are active.
- Video tracks can be composited with pixel-perfect alignment at any resolution up to 8K.

### Multi-track Audio Mixing

- Multiple audio tracks can be mixed into a single output stream.
- Per-track volume (dB gain) and pan (stereo position −1.0 to +1.0) are independently configurable.
- Any number of audio tracks can be mixed simultaneously.
- Tracks with different sample rates or channel layouts are automatically resampled and converted
  before mixing.
- An ordered per-track effect chain can be applied to each audio track before the mix (#679).

### Clip & Timeline Abstraction

- A `Clip` type represents one media source with in/out points, a timeline offset, and per-clip
  metadata. It carries no FFmpeg state — it is a plain Rust value type.
- A `Timeline` type represents an ordered layout of `Clip` instances across video and audio tracks.
- `Timeline::render()` composes and encodes the full timeline to an output file.
- `VideoFrame` and `AudioFrame` support cloning so clips can be passed through effect pipelines
  without consuming the frame.

### Timeline Clip Operations

- Any track can be offset in time relative to the composition origin (positive or negative delay in
  milliseconds).
- A clip can be trimmed to an arbitrary in/out time range without re-encoding (stream copy) when no
  filters are applied.
- Multiple video clips can be concatenated into a single seamless output.
- Multiple audio clips can be concatenated into a single seamless output.
- Clips can be joined with a cross-dissolve transition between them.
- A video's audio track can be replaced with a different audio source (stream copy mux, no video
  re-encode).
- An audio track can be extracted from a video file and saved as a standalone audio file.
- An audio track can be added to a silent video file.

### Media Analysis & Inspection

- Audio waveform data (peak and RMS amplitude per configurable time interval) can be extracted as a
  `Vec<WaveformSample>` suitable for waveform display rendering.
- Integrated loudness (LUFS), loudness range (LRA), and true peak (dBTP) can be measured per the
  EBU R128 standard.
- Scene change timestamps can be detected and returned as a `Vec<Duration>`, enabling automatic
  clip splitting and chapter generation.
- Keyframe timestamps in a video stream can be enumerated.
- Silence intervals in an audio stream can be detected with configurable threshold and minimum
  duration.
- SSIM and PSNR quality metrics can be computed between a processed video and a reference, enabling
  automated quality regression tests.
- Per-frame color histogram data (RGB channels and luminance) can be extracted as a
  `Vec<FrameHistogram>`.
- Black frame intervals can be detected (useful for removing leader/tail from captures).

### Thumbnail & Preview Frame Generation

- A single video frame can be extracted at any timestamp and returned as a `VideoFrame`.
- Frames can be batch-extracted at regular time intervals across the full duration of a clip.
- A thumbnail sprite sheet (a single image containing a cols×rows grid of thumbnails) can be
  generated, suitable for video player hover previews and scrub bars.
- An animated GIF preview can be generated from a configurable time range and frame rate.
- The "best" representative thumbnail frame can be selected automatically by skipping near-black,
  near-white, and blurry frames.

### Export Presets

- Predefined export presets are provided for common delivery targets:
  - YouTube 1080p (H.264 CRF 18, AAC 192 kbps, MP4)
  - YouTube 4K (H.265 CRF 20, AAC 256 kbps, MP4)
  - Twitter/X (H.264, AAC, MP4, ≤512 MB size constraint)
  - Instagram Square (H.264, AAC, MP4, 1:1)
  - Instagram Reels (H.264, AAC, MP4, 9:16 vertical, 30 fps)
  - Blu-ray 1080p (H.264/H.265, AC-3, MKV)
  - Podcast (AAC-LC 128 kbps, M4A)
  - Lossless archive (FFV1 + FLAC, MKV)
  - Web preview (VP9 + Opus, WebM, 720p)
- Custom presets can be defined as plain Rust structs and reused across an application.
- A preset validates all settings against known platform limits before encoding begins, returning a
  structured error if any constraint is violated.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Clip/Timeline | `Clip` and `Timeline` types live in `ff-pipeline`; they are pure Rust value types with no FFmpeg state. `Timeline::render()` assembles filter graphs from Clips at call time. |
| Composition engine | `MultiTrackComposer` in `ff-filter` uses `lavfi color` (base canvas) + chained `overlay` filters. Each layer has an optional `scale` and `colorchannelmixer` for opacity. |
| Audio mixing | `MultiTrackAudioMixer` in `ff-filter` inserts `aresample`/`aformat` per track when needed, then feeds `amix`. Per-track effects (#679) are chained before `amix`. |
| Per-track effect chain | `AudioTrack.effects: Vec<FilterStep>` — the same `FilterStep` enum used by `FilterGraphBuilder`, inserted into the per-track sub-graph before mixing. |
| VideoFrame/AudioFrame clone | `PooledBuffer` in `ff-common` gains `Clone` (deep copy of the underlying buffer). `VideoFrame` and `AudioFrame` in `ff-format` derive `Clone`. |
| Analysis tools | Analysis structs (`WaveformAnalyzer`, `SceneDetector`, etc.) live in a new `ff-decode::analysis` module. They decode via `VideoDecoder`/`AudioDecoder` internally — no new unsafe in the public API module. |
| Quality metrics | `LoudnessMeter` and `QualityMetrics` live in a new `ff-filter::analysis` module because they drive FFmpeg filter graphs (`ebur128`, `ssim`, `psnr`). |
| Frame extraction | `VideoDecoder::extract_frame(timestamp)` added to the existing `ff-decode::video` module. `FrameExtractor` and `ThumbnailSelector` live in a new `ff-decode::extract` module. |
| Sprite sheet | Uses FFmpeg's `fps` + `scale` + `tile` filter chain. Implemented in `ff-encode::preview`. |
| Animated GIF | Two-pass: `palettegen` then `paletteuse`. Implemented in `ff-encode::preview`. |
| Best thumbnail | `ThumbnailSelector` samples candidates at `candidate_interval`, scores each by mean luma and Laplacian variance, and returns the best non-black/non-white/non-blurry frame. Logic is pure Rust (no FFmpeg filter needed). |
| Export presets | Plain Rust structs; no TOML/JSON serialization at this stage. Validation enforces platform-specific limits (bitrate, aspect ratio, fps). |
| AudioExtractor / AudioReplacement / AudioAdder | Live in `ff-encode::media_ops` because they all produce an output file via FFmpeg's mux layer. |
| EBU R128 | Uses libavfilter `ebur128=metadata=1`; results parsed from filter metadata after draining all frames. |
| Scene detection | Uses libavfilter `select=gt(scene\,threshold)` + `showinfo`; scene scores are read from log output at `log::debug!` level and parsed in the inner module. |
| Keyframe enumeration | Reads `AV_PKT_FLAG_KEY` from packet flags via `av_read_frame` loop; no decoder needed. |
| Silence detection | Uses libavfilter `silencedetect=n=threshold:d=min_duration`; start/end parsed from filter metadata. |
| Black frame detection | Uses libavfilter `blackdetect=d=0.1:pic_th=0.98`; timestamps parsed from filter metadata. |
| SSIM / PSNR | Uses libavfilter `ssim` / `psnr` with a `lavfi` nullsink; mean value parsed after draining. |
| Histogram extraction | Uses manual per-pixel computation in the inner module (no dedicated libavfilter histogram — the FFmpeg `histogram` filter writes video output, not structured data). |

---

## Module Structure Changes

### ff-common

```
ff-common/src/
  pool.rs    ← PooledBuffer gains Clone (deep copy)
```

### ff-format

```
ff-format/src/frame/
  video.rs   ← VideoFrame derives Clone
  audio.rs   ← AudioFrame derives Clone
```

### ff-filter

```
ff-filter/src/
  composition/          ← NEW
    mod.rs              ← MultiTrackComposer, MultiTrackAudioMixer, VideoLayer, AudioTrack
    composer_inner.rs   ← unsafe FFmpeg filter graph calls (pub(crate))
  analysis/             ← NEW
    mod.rs              ← LoudnessMeter, QualityMetrics, LoudnessResult
    analysis_inner.rs   ← unsafe FFmpeg filter graph calls (pub(crate))
```

### ff-decode

```
ff-decode/src/
  analysis/             ← NEW
    mod.rs              ← WaveformAnalyzer, SceneDetector, KeyframeEnumerator,
                           SilenceDetector, SilenceRange, HistogramExtractor,
                           FrameHistogram, BlackFrameDetector, WaveformSample
    analysis_inner.rs   ← unsafe FFmpeg demux / filter calls (pub(crate))
  extract/              ← NEW
    mod.rs              ← FrameExtractor, ThumbnailSelector
    extract_inner.rs    ← unsafe FFmpeg calls (pub(crate))
  video/mod.rs          ← + extract_frame(timestamp) method on VideoDecoder
```

### ff-encode

```
ff-encode/src/
  media_ops/            ← NEW
    mod.rs              ← AudioReplacement, AudioExtractor, AudioAdder
    media_inner.rs      ← unsafe FFmpeg mux/remux calls (pub(crate))
  preset/               ← NEW
    mod.rs              ← ExportPreset, VideoEncoderConfig, AudioEncoderConfig
    presets.rs          ← predefined preset constants (safe)
    validation.rs       ← preset validation logic (safe)
  preview/              ← NEW
    mod.rs              ← SpriteSheet, GifPreview
    preview_inner.rs    ← unsafe FFmpeg filter + encode calls (pub(crate))
```

### ff-pipeline

```
ff-pipeline/src/
  clip.rs               ← NEW: Clip, ClipMetadata
  timeline.rs           ← NEW: Timeline, TimelineBuilder
```

---

## New Error Variants

### DecodeError

```rust
#[error("no frame found at timestamp: {timestamp:?}")]
NoFrameAtTimestamp { timestamp: Duration },

#[error("analysis failed: {reason}")]
AnalysisFailed { reason: String },
```

### FilterError

```rust
#[error("composition failed: {reason}")]
CompositionFailed { reason: String },

#[error("analysis failed: {reason}")]
AnalysisFailed { reason: String },
```

### EncodeError

```rust
#[error("preset constraint violated: preset={preset} reason={reason}")]
PresetConstraintViolation { preset: String, reason: String },

#[error("media operation failed: {reason}")]
MediaOperationFailed { reason: String },
```

### PipelineError

```rust
#[error("timeline render failed: {reason}")]
TimelineRenderFailed { reason: String },

#[error("clip source not found: path={path}")]
ClipNotFound { path: String },
```

---

## Definition of Done

- Multi-track composition test: 3-layer video + 2-track audio → single MP4 output (#326)
- EBU R128 measurement verified against a reference sine wave (±0.5 LUFS)
- Scene detection tested on a known cut sequence (≤1 frame tolerance)
- Sprite sheet generates correctly tiled image with correct pixel dimensions
- All predefined export presets produce output accepted by `ffprobe` validation
- `cargo test -p ff-filter -p ff-decode -p ff-encode -p ff-pipeline` all pass
