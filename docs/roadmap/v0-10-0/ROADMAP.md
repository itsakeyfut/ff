# v0.10.0 — Multi-track Composition & Media Analysis

**Goal**: Enable multi-track video/audio composition, clip timeline operations, media analysis tools, and thumbnail/preview generation — the foundational layer for building a non-linear editing workflow on top of the library.

**Prerequisite**: v0.9.0 complete.

**Crates in scope**: `ff-filter`, `ff-encode`, `ff-decode`, `ff-stream`, `ff-probe`

---

## Requirements

### Multi-track Video Composition

- Multiple video tracks can be composited with a configurable z-order, enabling picture-in-picture and side-by-side layouts.
- Each video track has configurable position (x, y), scale, and opacity at the composition stage.
- The output canvas size is configurable independently of any individual track's resolution.
- A solid color background can be used as the base layer when fewer tracks than z-levels are active.
- Video tracks can be composited with pixel-perfect alignment at any resolution up to 8K.

### Multi-track Audio Mixing

- Multiple audio tracks can be mixed into a single output stream.
- Per-track volume (linear gain) and pan (stereo position) are independently configurable.
- Any number of audio tracks can be mixed simultaneously.
- Tracks with different sample rates or channel layouts are automatically resampled and converted before mixing.

### Timeline Clip Operations

- Any track can be offset in time relative to the composition origin (positive or negative delay in milliseconds).
- Video and audio tracks are automatically synchronized by their presentation timestamps.
- A clip can be trimmed to an arbitrary in/out time range without re-encoding (stream copy) when no filters are applied.
- Multiple video clips can be concatenated into a single seamless output.
- Multiple audio clips can be concatenated into a single seamless output.
- Clips can be joined with a cross-dissolve transition between them.
- A video's audio track can be replaced with a different audio source (stream copy mux, no video re-encode).
- An audio track can be extracted from a video file and saved as a standalone audio file.
- An audio track can be added to a silent video file.

### Media Analysis & Inspection

- Audio waveform data (peak and RMS amplitude per configurable time interval) can be extracted as a Rust data structure suitable for waveform display rendering.
- Integrated loudness (LUFS), loudness range (LRA), and true peak (dBTP) can be measured per the EBU R128 standard.
- Scene change timestamps can be detected and returned as a `Vec<Duration>`, enabling automatic clip splitting and chapter generation.
- Keyframe timestamps in a video stream can be enumerated.
- Silence intervals in an audio stream can be detected with configurable threshold and minimum duration.
- SSIM and PSNR quality metrics can be computed between a processed video and a reference, enabling automated quality regression tests.
- Per-frame or per-scene color histogram data (RGB channels and luminance) can be extracted as a structured type.
- Black frame intervals can be detected (useful for removing leader/tail from captures).

### Thumbnail & Preview Frame Generation

- A single video frame can be extracted at any timestamp and returned as a `VideoFrame` or saved as an image file.
- Frames can be batch-extracted at regular time intervals across the full duration of a clip.
- A thumbnail sprite sheet (a single image containing a W×H grid of thumbnails) can be generated, suitable for video player hover previews and scrub bars.
- An animated GIF preview can be generated from a configurable time range and frame rate.
- The "best" representative thumbnail frame can be selected automatically by skipping near-black, near-white, and blurry frames.
- Frame extraction supports hardware-accelerated decode paths for throughput.

### Export Presets

- Predefined export presets are provided for common delivery targets:
  - YouTube 1080p (H.264 CRF 18, AAC 192 kbps, MP4)
  - YouTube 4K (H.265 CRF 20, AAC 256 kbps, MP4)
  - Twitter/X (H.264, AAC, MP4, ≤512 MB size constraint)
  - Instagram Reels (H.264, AAC, MP4, 9:16 vertical, 30 fps)
  - Blu-ray compliant (H.264/H.265, AC-3, MKV)
  - Podcast audio (AAC-LC 128 kbps, M4A)
  - Lossless archive (FFV1 + FLAC, MKV)
  - Web preview (VP9 + Opus, WebM, 720p)
- Custom presets can be defined as plain Rust structs and reused across an application.
- A preset validates all settings against known platform limits before encoding begins, returning a structured error if any constraint is violated.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Composition engine | Implemented as an `ff-filter` `overlay` graph; no separate compositor crate at this stage |
| Timeline model | No timeline data structure in the library — callers assemble filter graphs directly |
| EBU R128 | Uses libavfilter `ebur128` filter; results returned as a Rust struct |
| Scene detection | Uses libavfilter `select` with scene score threshold |
| Sprite sheets | Generated via `tile` filter after thumbnail extraction |
| Export presets | Plain Rust structs; no TOML/JSON serialization at this stage |

---

## Definition of Done

- Multi-track composition test: 3-layer video + 2-track audio → single MP4 output
- EBU R128 measurement verified against a reference sine wave
- Scene detection tested on a known cut sequence
- Sprite sheet generation produces a correctly tiled image
- All predefined export presets produce output accepted by their target validator (e.g., `ffprobe`)
