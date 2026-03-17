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
// FramePool, SimpleFramePool, and VecPool come from ff-common (re-exported via
// ff-decode). VecPool is the canonical concrete pool; SimpleFramePool is its alias.
#[cfg(feature = "decode")]
pub use ff_common::VecPool;
#[cfg(feature = "decode")]
pub use ff_decode::{
    AudioDecoder, AudioDecoderBuilder, AudioFrameIterator, DecodeError, FramePool, HardwareAccel,
    ImageDecoder, ImageDecoderBuilder, ImageFrameIterator, SeekMode, SimpleFramePool, VideoDecoder,
    VideoDecoderBuilder, VideoFrameIterator,
};

// ── encode feature ────────────────────────────────────────────────────────────
//
// EncodeProgress / EncodeProgressCallback carry encode-specific metrics and are
// distinct from ff-pipeline's Progress / ProgressCallback, so both sets can be
// re-exported from avio without ambiguity.
// VideoCodecEncodeExt provides encode-specific helpers (is_lgpl_compatible,
// default_extension) on the shared VideoCodec type; import it to call them.
#[cfg(feature = "encode")]
pub use ff_encode::{
    AudioEncoder, AudioEncoderBuilder, BitrateMode, CRF_MAX, Container, EncodeError,
    EncodeProgress, EncodeProgressCallback, HardwareEncoder, ImageEncoder, ImageEncoderBuilder,
    Preset, VideoCodecEncodeExt, VideoEncoder, VideoEncoderBuilder,
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
    AudioPipeline, EncoderConfig, EncoderConfigBuilder, Pipeline, PipelineBuilder, PipelineError,
    Progress, ProgressCallback, ThumbnailPipeline,
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

    #[cfg(feature = "decode")]
    #[test]
    fn decode_vec_pool_should_be_accessible() {
        let pool: std::sync::Arc<VecPool> = VecPool::new(8);
        assert_eq!(pool.capacity(), 8);
        assert_eq!(pool.available(), 0);
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

    #[cfg(feature = "encode")]
    #[test]
    fn encode_progress_should_be_accessible() {
        assert!(std::mem::size_of::<EncodeProgress>() > 0);
    }

    #[cfg(feature = "encode")]
    #[test]
    fn encode_progress_callback_should_be_accessible() {
        // EncodeProgressCallback is a trait — verify it is in scope by creating
        // a minimal no-op implementation.
        struct NoOp;
        impl EncodeProgressCallback for NoOp {
            fn on_progress(&mut self, _: &EncodeProgress) {}
        }
        let _ = NoOp;
    }

    // ── filter feature ────────────────────────────────────────────────────────

    #[cfg(feature = "filter")]
    #[test]
    fn filter_graph_builder_should_be_accessible() {
        // FilterGraphBuilder::new() is the public entry point.
        let _builder: FilterGraphBuilder = FilterGraphBuilder::new();
    }

    #[cfg(feature = "filter")]
    #[test]
    fn filter_tone_map_should_be_accessible() {
        let _: ToneMap = ToneMap::Hable;
        let _: ToneMap = ToneMap::Reinhard;
        let _: ToneMap = ToneMap::Mobius;
    }

    #[cfg(feature = "filter")]
    #[test]
    fn filter_hw_accel_should_be_accessible() {
        let _: HwAccel = HwAccel::Cuda;
        let _: HwAccel = HwAccel::VideoToolbox;
    }

    #[cfg(feature = "filter")]
    #[test]
    fn filter_error_should_be_accessible() {
        let _: FilterError = FilterError::BuildFailed;
        let _: FilterError = FilterError::ProcessFailed;
    }

    // ── pipeline feature ──────────────────────────────────────────────────────

    #[cfg(feature = "pipeline")]
    #[test]
    fn pipeline_builder_should_be_accessible() {
        // Pipeline::builder() is the public entry point; verify name resolution.
        let _builder: PipelineBuilder = Pipeline::builder();
    }

    #[cfg(feature = "pipeline")]
    #[test]
    fn pipeline_error_should_be_accessible() {
        let _: PipelineError = PipelineError::NoInput;
        let _: PipelineError = PipelineError::NoOutput;
        let _: PipelineError = PipelineError::Cancelled;
    }

    #[cfg(feature = "pipeline")]
    #[test]
    fn pipeline_progress_should_be_accessible() {
        let p = Progress {
            frames_processed: 10,
            total_frames: Some(100),
            elapsed: std::time::Duration::from_secs(1),
        };
        assert_eq!(p.percent(), Some(10.0));
    }

    #[cfg(feature = "pipeline")]
    #[test]
    fn pipeline_progress_callback_should_be_accessible() {
        // ProgressCallback is Box<dyn Fn(&Progress) -> bool + Send>.
        let _cb: ProgressCallback = Box::new(|_: &Progress| true);
    }

    #[cfg(feature = "pipeline")]
    #[test]
    fn pipeline_thumbnail_pipeline_should_be_accessible() {
        // ThumbnailPipeline::new constructs without opening a file.
        let _t: ThumbnailPipeline = ThumbnailPipeline::new("/no/such/file.mp4");
    }

    #[cfg(feature = "pipeline")]
    #[test]
    fn pipeline_audio_pipeline_should_be_accessible() {
        let _: AudioPipeline = AudioPipeline::new();
    }

    #[cfg(all(feature = "pipeline", feature = "encode"))]
    #[test]
    fn pipeline_encoder_config_should_be_accessible() {
        let _config = EncoderConfig::builder()
            .video_codec(VideoCodec::H264)
            .audio_codec(AudioCodec::Aac)
            .bitrate_mode(BitrateMode::Cbr(4_000_000))
            .build();
    }

    // ── stream feature ────────────────────────────────────────────────────────

    #[cfg(feature = "stream")]
    #[test]
    fn stream_hls_output_should_be_accessible() {
        // HlsOutput::new() is the public entry point; verify name resolution.
        let _hls: HlsOutput = HlsOutput::new("/tmp/hls");
    }

    #[cfg(feature = "stream")]
    #[test]
    fn stream_dash_output_should_be_accessible() {
        // DashOutput::new() is the public entry point; verify name resolution.
        let _dash: DashOutput = DashOutput::new("/tmp/dash");
    }

    #[cfg(feature = "stream")]
    #[test]
    fn stream_abr_ladder_should_be_accessible() {
        // AbrLadder::new() is the public entry point; verify name resolution.
        let _ladder: AbrLadder = AbrLadder::new("/no/such/file.mp4");
    }

    #[cfg(feature = "stream")]
    #[test]
    fn stream_rendition_should_be_accessible() {
        let _r: Rendition = Rendition {
            width: 1280,
            height: 720,
            bitrate: 3_000_000,
        };
    }

    #[cfg(feature = "stream")]
    #[test]
    fn stream_error_should_be_accessible() {
        let _err: StreamError = StreamError::InvalidConfig {
            reason: "test".into(),
        };
    }
}
