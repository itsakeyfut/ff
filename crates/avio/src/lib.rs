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

#[cfg(test)]
mod tests {
    use super::*;

    // ── ff-format (always-on) ─────────────────────────────────────────────────

    #[test]
    fn format_reexports_should_be_accessible() {
        let _: VideoCodec = VideoCodec::default();
        let _: AudioCodec = AudioCodec::default();
        let _: PixelFormat = PixelFormat::default();
        let _: SampleFormat = SampleFormat::default();
        let _: ChannelLayout = ChannelLayout::default();
        let _: ColorSpace = ColorSpace::default();
        let _: ColorRange = ColorRange::default();
        let _: ColorPrimaries = ColorPrimaries::default();
        let _: Rational = Rational::default();
        let _: Timestamp = Timestamp::default();
        let _: MediaInfo = MediaInfo::default();
    }

    // ── probe feature ─────────────────────────────────────────────────────────

    #[cfg(feature = "probe")]
    #[test]
    fn probe_open_should_be_accessible() {
        // open is a function — a non-existent path yields ProbeError
        let result = open("/no/such/file.mp4");
        assert!(matches!(result, Err(ProbeError::FileNotFound { .. })));
    }

    #[cfg(feature = "probe")]
    #[test]
    fn probe_error_should_be_accessible() {
        let err = ProbeError::FileNotFound {
            path: std::path::PathBuf::from("missing.mp4"),
        };
        assert!(err.to_string().contains("missing.mp4"));
    }

    // ── decode feature ────────────────────────────────────────────────────────

    #[cfg(feature = "decode")]
    #[test]
    fn decode_builder_types_should_be_accessible() {
        // Builder entry points are static methods on the decoder types.
        // Calling them with a dummy path exercises name resolution without
        // touching FFmpeg.
        let _builder: VideoDecoderBuilder = VideoDecoder::open("/no/such/file.mp4");
        let _builder: AudioDecoderBuilder = AudioDecoder::open("/no/such/file.mp4");
        let _builder: ImageDecoderBuilder = ImageDecoder::open("/no/such/file.mp4");
    }

    #[cfg(feature = "decode")]
    #[test]
    fn decode_error_should_be_accessible() {
        let _: DecodeError = DecodeError::EndOfStream;
    }

    #[cfg(feature = "decode")]
    #[test]
    fn decode_seek_mode_should_be_accessible() {
        let _: SeekMode = SeekMode::Keyframe;
        let _: SeekMode = SeekMode::Exact;
        let _: SeekMode = SeekMode::Backward;
    }

    #[cfg(feature = "decode")]
    #[test]
    fn decode_hardware_accel_should_be_accessible() {
        let _: HardwareAccel = HardwareAccel::Auto;
        let _: HardwareAccel = HardwareAccel::None;
    }

    // ── encode feature ────────────────────────────────────────────────────────

    #[cfg(feature = "encode")]
    #[test]
    fn encode_builder_types_should_be_accessible() {
        // VideoEncoder::create / AudioEncoder::create are the public entry
        // points that return their respective builder types.
        let _builder: VideoEncoderBuilder = VideoEncoder::create("/tmp/out.mp4");
        let _builder: AudioEncoderBuilder = AudioEncoder::create("/tmp/out.mp3");
    }

    #[cfg(feature = "encode")]
    #[test]
    fn encode_bitrate_mode_should_be_accessible() {
        let _: BitrateMode = BitrateMode::Cbr(1_000_000);
        let _: BitrateMode = BitrateMode::Crf(23);
    }

    #[cfg(feature = "encode")]
    #[test]
    fn encode_error_should_be_accessible() {
        let _: EncodeError = EncodeError::Cancelled;
    }
}
