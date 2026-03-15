# ff-format

Shared data types for the ff-* crate family. No FFmpeg dependency — these types exist so crates can exchange frames and stream metadata without coupling to FFmpeg's internal structs.

## Key Types

| Type           | Description                                                     |
|----------------|-----------------------------------------------------------------|
| `PixelFormat`  | Enumeration of supported pixel formats (Yuv420p, Rgba, …)       |
| `SampleFormat` | Audio sample formats (Fltp, S16, …)                             |
| `ChannelLayout`| Mono, Stereo, Surround51, and others                            |
| `ColorSpace`   | BT.601, BT.709, BT.2020, and others                             |
| `VideoCodec`   | H264, H265, Vp9, Av1, ProRes, and others                        |
| `AudioCodec`   | Aac, Opus, Mp3, Flac, and others                                |
| `VideoFrame`   | Decoded video frame with pixel data and associated `Timestamp`  |
| `AudioFrame`   | Decoded audio frame with sample data and associated `Timestamp` |
| `Timestamp`    | Media position expressed as a `Rational` fraction of seconds    |

These are the types `ff-decode` hands back when you decode a frame, and the types `ff-encode` expects when you push a frame.

## Example

```rust
use ff_format::{Timestamp, Rational};

// Create a timestamp at 2.5 seconds (5/2).
let ts = Timestamp::new(5, 2);

// Arithmetic on rational timestamps stays exact.
let one_second = Timestamp::new(1, 1);
let three_seconds = ts + one_second * Rational::new(1, 1);

assert_eq!(ts.as_secs_f64(), 2.5);
```

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0
