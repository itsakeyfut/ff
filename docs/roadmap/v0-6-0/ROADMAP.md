# v0.6.0 — Async Support

**Goal**: Provide a native tokio async API for decoding and encoding, enabling non-blocking media pipelines in async Rust applications.

**Prerequisite**: v0.5.0 complete.

**Crates in scope**: `ff-decode`, `ff-encode`

**Feature flag**: `tokio` (opt-in; all synchronous APIs remain the default and are unchanged)

---

## Requirements

### Async Decoding

- Video frames can be decoded using `async/.await` without blocking the tokio runtime.
- Audio frames can be decoded using `async/.await`.
- Media files can be opened asynchronously.
- Decoded frames can be consumed as a `futures::Stream`, enabling lazy pull-based iteration.
- Ongoing decode operations can be cancelled by simply dropping the stream or decoder.
- Multiple media files can be decoded concurrently by spawning independent tokio tasks.

### Async Encoding

- Video frames can be pushed to an encoder using `async/.await`.
- Audio frames can be pushed to an encoder using `async/.await`.
- Encoding can be finalized asynchronously via an async `finish()` call.
- The encoder applies automatic backpressure: when the internal buffer is full (capacity: 8 frames), `push().await` suspends the caller rather than dropping frames or growing memory unboundedly.
- The async encoder correctly flushes all in-flight frames on `finish()`.

### Compatibility

- The `tokio` feature gate is strictly additive; disabling it produces exactly the same binary as before.
- `AsyncVideoDecoder`, `AsyncAudioDecoder`, `AsyncVideoEncoder`, and `AsyncAudioEncoder` are only compiled when the `tokio` feature is enabled.
- No public API from v0.5.0 or earlier is broken or removed.
- The async types are `Send`, enabling use across tokio task boundaries.

---

## Design Decisions

| Topic | Decision |
|---|---|
| FFmpeg thread model | All FFmpeg calls are dispatched via `spawn_blocking` to the thread pool — FFmpeg is not async-aware |
| Backpressure | Bounded `tokio::sync::mpsc` channel (cap=8); caller awaits when full |
| Stream trait | `futures::Stream`; `tokio-stream` is dev-dep only |
| Sync API | Preserved without change — no breaking changes |

---

## Definition of Done

- `cargo test -p ff-decode --features tokio` passes
- `cargo test -p ff-encode --features tokio` passes
- `cargo build --workspace` (without `tokio` feature) is unaffected
- `AsyncVideoDecoder::into_stream()` can drive a full decode loop in a tokio test
