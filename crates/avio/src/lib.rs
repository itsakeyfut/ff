//! Safe, high-level audio/video/image processing for Rust.
//!
//! `avio` is the facade crate for the `ff-*` crate family — a backend-agnostic
//! multimedia toolkit. It re-exports the public APIs of all member crates behind
//! feature flags, so users can depend on a single crate and opt into only the
//! functionality they need.
//!
//! # Feature Flags
//!
//! | Feature    | Crate         | Default | Implies    |
//! |------------|---------------|---------|------------|
//! | `probe`    | `ff-probe`    | yes     | —          |
//! | `decode`   | `ff-decode`   | yes     | —          |
//! | `encode`   | `ff-encode`   | yes     | —          |
//! | `filter`   | `ff-filter`   | no      | —          |
//! | `pipeline` | `ff-pipeline` | no      | `filter`   |
//! | `stream`   | `ff-stream`   | no      | `pipeline` |
//!
//! # Usage
//!
//! ```toml
//! # Default: probe + decode + encode
//! [dependencies]
//! avio = "0.5"
//!
//! # Add filtering
//! avio = { version = "0.5", features = ["filter"] }
//!
//! # Full stack (implies filter + pipeline)
//! avio = { version = "0.5", features = ["stream"] }
//! ```

// ── Always-available types from ff-format ────────────────────────────────────
//
// ff-format is an unconditional dependency, so these types are always present
// regardless of which features are enabled. Re-exporting them here avoids the
// duplicate-symbol problem that would arise from re-exporting VideoCodec /
// AudioCodec separately from ff-probe *and* ff-encode (both of which pull them
// in from ff-format anyway).
pub use ff_format::{
    AudioCodec, AudioFrame, AudioStreamInfo, ChannelLayout, ChapterInfo, ChapterInfoBuilder,
    ColorPrimaries, ColorRange, ColorSpace, FormatError, FrameError, MediaInfo, MediaInfoBuilder,
    PixelFormat, PooledBuffer, Rational, SampleFormat, SubtitleCodec, SubtitleStreamInfo,
    SubtitleStreamInfoBuilder, Timestamp, VideoCodec, VideoFrame, VideoStreamInfo,
};

// ── probe feature ─────────────────────────────────────────────────────────────
#[cfg(feature = "probe")]
pub use ff_probe::{ProbeError, open};

// ── decode feature ────────────────────────────────────────────────────────────
//
// PooledBuffer and the frame/codec types are already re-exported from ff-format
// above, so we omit them here to keep a single canonical source.
#[cfg(feature = "decode")]
pub use ff_decode::{
    AudioDecoder, AudioDecoderBuilder, AudioFrameIterator, DecodeError, FramePool, HardwareAccel,
    ImageDecoder, ImageDecoderBuilder, ImageFrameIterator, SeekMode, SimpleFramePool, VideoDecoder,
    VideoDecoderBuilder, VideoFrameIterator,
};

// ── encode feature ────────────────────────────────────────────────────────────
//
// Progress / ProgressCallback are intentionally omitted here: they are also
// exported by ff-pipeline, and exposing them from two paths under the same name
// would be ambiguous when the `pipeline` feature is also enabled.
#[cfg(feature = "encode")]
pub use ff_encode::{
    AudioEncoder, AudioEncoderBuilder, BitrateMode, CRF_MAX, Container, EncodeError,
    HardwareEncoder, ImageEncoder, ImageEncoderBuilder, Preset, VideoEncoder, VideoEncoderBuilder,
};

// ── filter feature ────────────────────────────────────────────────────────────
#[cfg(feature = "filter")]
pub use ff_filter::{FilterError, FilterGraph, FilterGraphBuilder, HwAccel, ToneMap};

// ── pipeline feature ──────────────────────────────────────────────────────────
//
// Enabling `pipeline` also enables `filter` (see Cargo.toml).
// Progress / ProgressCallback are re-exported here as the canonical source.
#[cfg(feature = "pipeline")]
pub use ff_pipeline::{
    EncoderConfig, Pipeline, PipelineBuilder, PipelineError, Progress, ProgressCallback,
    ThumbnailPipeline,
};

// ── stream feature ────────────────────────────────────────────────────────────
//
// Enabling `stream` also enables `pipeline` (and transitively `filter`).
#[cfg(feature = "stream")]
pub use ff_stream::{AbrLadder, DashOutput, HlsOutput, Rendition, StreamError};
