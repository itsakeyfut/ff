# ff-decode

Decode video and audio frames without managing codec contexts, packet queues, or timestamp conversions. Open a file, call `decode_frame` in a loop, and receive `VideoFrame` objects with their position already expressed as a `Duration`.

## Installation

```toml
[dependencies]
ff-decode = "0.3"
ff-format = "0.3"
```

## Video Decoding

```rust
use ff_decode::VideoDecoder;
use ff_format::PixelFormat;

let mut decoder = VideoDecoder::open("video.mp4")?
    .output_format(PixelFormat::Rgba)
    .build()?;

while let Some(frame) = decoder.decode_frame()? {
    // frame.data()      — raw pixel bytes in RGBA order
    // frame.width()     — frame width in pixels
    // frame.height()    — frame height in pixels
    // frame.timestamp() — position as std::time::Duration
    process(&frame);
}
```

## Audio Decoding

```rust
use ff_decode::AudioDecoder;
use ff_format::{SampleFormat, ChannelLayout};

let mut decoder = AudioDecoder::open("audio.flac")?
    .output_format(SampleFormat::Fltp)
    .output_sample_rate(44_100)
    .output_channel_layout(ChannelLayout::Stereo)
    .build()?;

while let Some(frame) = decoder.decode_frame()? {
    // frame.data()        — interleaved or planar sample bytes
    // frame.sample_rate() — samples per second
    // frame.timestamp()   — position as std::time::Duration
    process(&frame);
}
```

## Seeking

```rust
use ff_decode::{VideoDecoder, SeekMode};
use std::time::Duration;

let mut decoder = VideoDecoder::open("video.mp4")?.build()?;

// Jump to the nearest keyframe at or before 30 seconds.
decoder.seek(Duration::from_secs(30), SeekMode::Keyframe)?;

// Jump to the exact position (may decode additional frames internally).
decoder.seek(Duration::from_secs(30), SeekMode::Exact)?;
```

Seeking does not re-open the file. The existing codec context is flushed and reused.

## Hardware Acceleration

```rust
use ff_decode::{VideoDecoder, HardwareAccel};

let mut decoder = VideoDecoder::open("video.mp4")?
    .hardware_accel(HardwareAccel::Auto)
    .build()?;
```

`HardwareAccel::Auto` probes for NVDEC, DXVA2, VideoToolbox, and VAAPI in that order, and falls back to software decoding if none is available.

## Error Handling

| Variant                          | When it occurs                                   |
|----------------------------------|--------------------------------------------------|
| `DecodeError::FileNotFound`      | The input path does not exist                    |
| `DecodeError::CannotOpen`        | FFmpeg could not open the container or codec     |
| `DecodeError::UnsupportedCodec`  | No decoder available for the stream's codec      |
| `DecodeError::InvalidConfig`     | Builder options are inconsistent or unsupported  |
| `DecodeError::Io`                | Read error on the underlying file                |

## What the Crate Handles for You

- Codec context allocation and lifetime
- PTS-to-`Duration` conversion using the stream's time base
- Packet queue management and buffering
- EOF signalled as `Ok(None)` rather than a special error variant
- Pixel format and sample format negotiation via `swscale` / `swresample`

## Feature Flags

| Flag | Description | Default |
|------|-------------|---------|
| `tokio` | Enables `AsyncVideoDecoder` and `AsyncAudioDecoder`. Wraps each blocking FFmpeg call in `tokio::task::spawn_blocking` and exposes a `futures::Stream` interface via `into_stream()`. Requires a tokio 1.x runtime. | disabled |

```toml
[dependencies]
ff-decode = { version = "0.5", features = ["tokio"] }
```

When the `tokio` feature is disabled, only the synchronous `VideoDecoder` and `AudioDecoder` APIs are compiled. No tokio dependency is pulled in.

## MSRV

Rust 1.93.0 (edition 2024).

## License

MIT OR Apache-2.0
