# v0.9.0 — Advanced Filtering, Effects & Clip Processing

**Goal**: Provide the core building blocks of a non-linear video editor — color grading, image adjustments, audio effects, transitions, text/graphics overlay, speed control, and multi-clip operations — so that the crate alone is sufficient for most media processing pipelines.

**Prerequisite**: v0.8.0 complete.

**Crates in scope**: `ff-filter`, `ff-encode`, `ff-decode`

---

## Requirements

### Color Grading & Color Correction

- A 3D LUT can be applied to video from a `.cube` or `.3dl` file, enabling industry-standard color grading workflows.
- Brightness, contrast, and saturation can be adjusted independently.
- Per-channel RGB color curves can be applied (control points mapped to a spline).
- White balance can be corrected by temperature (Kelvin) and tint offset.
- Hue can be rotated by an arbitrary angle.
- Gamma correction is available independently for shadows, midtones, and highlights.
- A three-way color corrector (lift/gamma/gain per shadows/midtones/highlights) is available for broadcast-style grading.
- A vignette effect can be applied with configurable radius and strength.

### Image Adjustment & Geometry

- Video can be cropped to an arbitrary rectangle (x, y, width, height).
- Video can be rotated by any angle, with a configurable background fill color.
- Video can be flipped horizontally and/or vertically.
- Video can be resized using a configurable resampling algorithm: nearest, bilinear, bicubic, Lanczos.
- Video can be padded to a target resolution with a configurable fill color (useful for letterboxing/pillarboxing).
- Video can be letterboxed or pillarboxed automatically to fit a target aspect ratio without distortion.
- Gaussian blur with configurable radius can be applied.
- Sharpening via unsharp mask (with configurable luma and chroma strength) is available.
- Video noise can be reduced via `hqdn3d` and `nlmeans` filters.
- Deinterlacing is available for interlaced source content.

### Video Transitions

- A fade-to-black or fade-from-black can be applied over a configurable duration.
- A fade-to-white or fade-from-white can be applied.
- A cross dissolve between two consecutive clips can be rendered over a configurable duration.

### Text & Graphics Overlay

- An arbitrary UTF-8 text string can be rendered onto video at a specified (x, y) position.
- Font family, size, color, bold/italic, and opacity are all configurable.
- A solid or transparent background box can be rendered behind the text.
- Subtitle files (SRT, ASS/SSA) can be burned directly into video frames (hard subtitles).
- A PNG image with an alpha channel can be composited over video at a configurable position and opacity — enabling watermarks, logos, and bug overlays.
- A scrolling text ticker can be rendered along a horizontal band.

### Speed Control

- Playback speed can be changed to any factor from 0.1× to 100× (slow motion and fast motion).
- Video can be played in reverse.
- A freeze frame can be inserted at a specific timestamp for a configurable duration.

### Audio Effects & Processing

- Audio can be normalized to a target integrated loudness (EBU R128 / LUFS).
- Audio can be normalized to a target true peak level (dBTP).
- Audio volume can be adjusted by a dB gain value.
- Audio can be faded in and faded out over a configurable duration.
- A parametric equalizer is available: low-shelf, high-shelf, and peak (bell) bands with configurable frequency, gain, and Q.
- A noise gate is available with configurable threshold, attack, and release.
- Dynamic range compression is available with configurable threshold, ratio, attack, and release.
- Stereo audio can be downmixed to mono.
- Audio channels can be remapped arbitrarily (e.g., swap L/R, extract a single channel, upmix mono to stereo).
- Audio delay can be applied (positive offset in milliseconds) for A/V sync correction.

### Multi-clip Operations

- Multiple video clips can be concatenated into a single output, with seamless timeline stitching.
- Multiple audio clips can be concatenated into a single output.
- A clip can be trimmed to a specific time range (start timestamp, end timestamp) without re-encoding (stream copy) where possible, and with re-encoding when filters are applied.
- Clips can be joined with a transition (cross dissolve) between them.

### Robustness & Input Validation

- `VideoEncoderBuilder::build()` rejects resolutions outside 2×2 to 32768×32768.
- `VideoEncoderBuilder::build()` rejects bitrates above 800 Mbps.
- `AudioEncoderBuilder::build()` rejects channel counts above 8 and sample rates outside 8000–384000 Hz.
- `DecodeError::UnsupportedResolution` is returned when a decoded frame exceeds the supported size limit.
- `EncodeError::InvalidDimensions` and `EncodeError::InvalidBitrate` are added.
- `VideoDecoder::decode_frame()` skips packets with `AVERROR_INVALIDDATA` and logs a warning rather than aborting, up to a consecutive limit of 32 — after which `DecodeError::StreamCorrupted` is returned.
- cargo-fuzz targets are added for `VideoDecoder::open`, `ff_probe::open`, and `VideoEncoder` — all must survive 60 seconds of fuzzing without crashing or triggering undefined behaviour.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Filter implementation | All effects are implemented as `ff-filter` graph nodes using libavfilter |
| LUT loading | `.cube` parsed in Rust; passed to `lut3d` filter |
| Transition rendering | Cross dissolve uses `xfade` filter; requires both clips to be decoded simultaneously |
| Speed control | Video: `setpts=PTS/factor`; Audio: `atempo` (chained for factors outside 0.5–2.0) |
| Concatenation | Uses `concat` filter for re-encode path; `av_interleaved_write_frame` stream copy for no-filter path |
| Fuzz runner | cargo-fuzz (LibFuzzer); CI `-max_total_time=60` per target |
| Validation location | `build()` only, consistent with existing conventions |

---

## Definition of Done

- LUT application verified against a reference image
- All audio effects tested with a known sine wave input
- `cargo test -p ff-filter` passes for all new filter nodes
- Fuzz targets run 60 seconds in CI without crashes
- Input validation unit tests cover all new error variants
