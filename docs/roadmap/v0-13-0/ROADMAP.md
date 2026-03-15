# v0.13.0 — Real-time Preview & Proxy Workflow

**Goal**: Introduce a new `ff-preview` crate providing a real-time playback engine — frame timing, A/V synchronization, seekable playback, and proxy generation — so that applications can build a responsive editing UI on top of the library without managing decode scheduling themselves.

**Prerequisite**: v0.12.0 complete.

**Crates in scope**: `ff-preview` (new crate), `ff-decode`, `ff-encode`

---

## Requirements

### Playback Clock

- A wall-clock-driven playback clock controls the timing of frame presentation.
- The clock can be started, paused, resumed, and stopped.
- The playback rate is configurable: 0.25×, 0.5×, 1×, 2×, and arbitrary fractional rates.
- The current playback position (presentation timestamp) is queryable at any time.
- The clock emits a callback or channel message when a new frame is due, with the target PTS for that frame.

### Frame Decode Pipeline

- A decode-ahead buffer pre-decodes and caches upcoming frames in a background thread, so that `next_frame()` returns without stalling the render thread.
- The buffer size is configurable (default: 8 frames).
- When playback reaches the end of the buffer capacity, back-pressure is applied — the background decoder pauses rather than allocating unbounded memory.
- On seek, the buffer is flushed and the background thread immediately begins decoding from the new position.

### Seeking

- Frame-accurate seek: given a target timestamp, the decoder seeks to the nearest preceding I-frame and then decodes forward to the exact target PTS.
- Coarse seek (I-frame only) is available as a fast alternative for scrub-bar dragging.
- Seeking is non-blocking: the caller receives a placeholder (last good frame) while the decoder catches up.
- Seek completion is signalled via a channel or callback.

### Audio / Video Synchronization

- The audio output clock is used as the master clock by default; video frames are presented at the timestamp that matches the audio playback position.
- When audio is absent, the system clock is used as the master clock.
- A/V offset correction is configurable (positive or negative delay in milliseconds) for sources with inherent A/V skew.
- Audio is delivered as PCM sample blocks sized to match the platform audio buffer, with timing aligned to the presentation clock.

### Output Interface

- `ff-preview` delivers decoded frames as raw pixel buffers (`VideoFrame` and `AudioFrame`) — it does not render to a screen itself.
- A `FrameSink` trait allows the caller to provide a custom receiver (wgpu texture upload, SDL surface blit, software compositor, etc.).
- A reference `RgbaSink` implementation converts `VideoFrame` to a contiguous RGBA `Vec<u8>` for easy integration with any 2D drawing library.
- Frames are delivered on a dedicated callback thread; the frame sink implementation must be `Send`.

### Proxy Workflow

- Low-resolution proxy files can be generated from any source clip: configurable resolution (e.g., 1/2, 1/4, 1/8 of original) and codec (H.264 or MJPEG for fast decode).
- Proxy files are stored alongside the source with a configurable naming convention.
- `ff-preview` automatically substitutes the proxy file during playback when:
  1. A proxy exists for the active clip, and
  2. The caller has enabled proxy mode.
- On export, the original full-resolution source is used transparently regardless of proxy mode.
- A proxy generation job can run in the background without blocking the playback engine.

### Performance Requirements

- Full 1080p/30 playback must be achievable on a modern CPU (tested in CI with a benchmark).
- Seek-to-display latency (coarse seek): ≤ 100 ms on a local SSD source.
- Seek-to-display latency (frame-accurate): ≤ 500 ms for a target up to 60 frames ahead of the nearest I-frame.

---

## Design Decisions

| Topic | Decision |
|---|---|
| New crate | `ff-preview` — depends on `ff-decode`, `ff-format`; optional dep on `ff-filter` |
| Clock master | Audio clock by default; system clock fallback |
| Threading model | Background decode thread + foreground render thread; communication via bounded channels |
| `FrameSink` | Trait object (`Box<dyn FrameSink + Send>`); zero-copy path available when the sink accepts `Arc<VideoFrame>` |
| Proxy format | H.264 (fast decode) or MJPEG (random-access per-frame); caller chooses |
| `tokio` feature | `ff-preview` gains an async API behind `tokio` feature (same pattern as ff-decode/ff-encode) |

---

## Definition of Done

- 1080p/30 playback loop runs for 60 seconds without frame drops in CI benchmark
- Seek-to-exact-frame test: frame at t=30.000s is returned correctly after seek from t=0
- A/V sync test: audio and video PTS delta is within ±1 video frame during 60-second playback
- Proxy generation test: 1/4-resolution H.264 proxy is generated and substituted transparently
- `RgbaSink` integration test: 10 frames decoded and converted to RGBA without corruption
