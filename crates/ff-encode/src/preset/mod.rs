//! Export preset types and predefined presets.
//!
//! [`ExportPreset`] bundles a [`VideoEncoderConfig`] and an [`AudioEncoderConfig`]
//! into a named snapshot that can be applied to a [`VideoEncoderBuilder`] before
//! calling `.build()`.
//!
//! # Examples
//!
//! ```ignore
//! use ff_encode::{ExportPreset, VideoEncoder};
//!
//! let preset = ExportPreset::youtube_1080p();
//! preset.validate()?;
//!
//! let mut encoder = preset
//!     .apply_video(VideoEncoder::create("output.mp4"))
//!     .apply_audio_settings(VideoEncoder::create("output.mp4")) // hypothetical
//!     .build()?;
//! ```

mod presets;
mod validation;

use ff_format::{PixelFormat, VideoCodec};

use crate::video::codec_options::VideoCodecOptions;
use crate::{AudioCodec, BitrateMode, EncodeError, VideoEncoderBuilder};

/// Configuration for the video stream of an export preset.
#[derive(Debug, Clone)]
pub struct VideoEncoderConfig {
    /// Video codec.
    pub codec: VideoCodec,
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Output frame rate.
    pub fps: f64,
    /// Bitrate control mode.
    pub bitrate_mode: BitrateMode,
    /// Optional pixel format override. `None` lets the encoder choose.
    pub pixel_format: Option<PixelFormat>,
    /// Optional per-codec advanced options.
    pub codec_options: Option<VideoCodecOptions>,
}

/// Configuration for the audio stream of an export preset.
#[derive(Debug, Clone)]
pub struct AudioEncoderConfig {
    /// Audio codec.
    pub codec: AudioCodec,
    /// Sample rate in Hz (e.g. 48000).
    pub sample_rate: u32,
    /// Number of audio channels (1 = mono, 2 = stereo).
    pub channels: u32,
    /// Audio bitrate in bits per second (e.g. 192_000 = 192 kbps).
    pub bitrate: u64,
}

/// A named export preset combining video and audio encoder configuration.
///
/// Create a predefined preset with [`ExportPreset::youtube_1080p()`] etc., or
/// build a custom one as a struct literal. Call [`validate()`](Self::validate)
/// before encoding to catch platform-constraint violations early.
///
/// # Examples
///
/// ```ignore
/// use ff_encode::{ExportPreset, VideoEncoder};
///
/// let preset = ExportPreset::youtube_1080p();
/// preset.validate()?;
///
/// let mut encoder = preset
///     .apply_video(VideoEncoder::create("output.mp4"))
///     .build()?;
/// ```
#[derive(Debug, Clone)]
pub struct ExportPreset {
    /// Human-readable name (e.g. `"youtube_1080p"`).
    pub name: String,
    /// Video encoder configuration.
    pub video: VideoEncoderConfig,
    /// Audio encoder configuration.
    pub audio: AudioEncoderConfig,
}

impl ExportPreset {
    // ── Predefined presets ────────────────────────────────────────────────────

    /// YouTube 1080p preset: H.264, CRF 18, 1920×1080, 30 fps, AAC 192 kbps.
    #[must_use]
    pub fn youtube_1080p() -> Self {
        presets::youtube_1080p()
    }

    /// YouTube 4K preset: H.265, CRF 20, 3840×2160, 30 fps, AAC 256 kbps.
    #[must_use]
    pub fn youtube_4k() -> Self {
        presets::youtube_4k()
    }

    // ── Validation ────────────────────────────────────────────────────────────

    /// Validates this preset against platform-specific constraints.
    ///
    /// Call this before passing the preset to [`apply_video`](Self::apply_video)
    /// or [`apply_audio`](Self::apply_audio) to surface constraint violations
    /// before encoding begins.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::PresetConstraintViolation`] when a platform rule
    /// is violated (e.g. fps > 60 on a YouTube preset).
    pub fn validate(&self) -> Result<(), EncodeError> {
        validation::validate_preset(self)
    }

    // ── Builder helpers ───────────────────────────────────────────────────────

    /// Applies the video configuration to a [`VideoEncoderBuilder`].
    ///
    /// Sets resolution, frame rate, codec, bitrate mode, and optionally pixel
    /// format and per-codec options. Does **not** call [`validate`](Self::validate)
    /// — do that separately before encoding.
    #[must_use]
    pub fn apply_video(&self, builder: VideoEncoderBuilder) -> VideoEncoderBuilder {
        let mut b = builder
            .video(self.video.width, self.video.height, self.video.fps)
            .video_codec(self.video.codec)
            .bitrate_mode(self.video.bitrate_mode.clone());
        if let Some(pf) = self.video.pixel_format {
            b = b.pixel_format(pf);
        }
        if let Some(opts) = self.video.codec_options.clone() {
            b = b.codec_options(opts);
        }
        b
    }

    /// Applies the audio configuration to a [`VideoEncoderBuilder`].
    ///
    /// Sets sample rate, channel count, codec, and bitrate.
    #[must_use]
    pub fn apply_audio(&self, builder: VideoEncoderBuilder) -> VideoEncoderBuilder {
        builder
            .audio(self.audio.sample_rate, self.audio.channels)
            .audio_codec(self.audio.codec)
            .audio_bitrate(self.audio.bitrate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_1080p_preset_should_have_correct_codec() {
        let preset = ExportPreset::youtube_1080p();
        assert_eq!(
            preset.video.codec,
            VideoCodec::H264,
            "expected H264 codec for youtube_1080p"
        );
    }

    #[test]
    fn youtube_4k_preset_should_have_h265_codec() {
        let preset = ExportPreset::youtube_4k();
        assert_eq!(
            preset.video.codec,
            VideoCodec::H265,
            "expected H265 codec for youtube_4k"
        );
    }

    #[test]
    fn youtube_1080p_preset_should_have_correct_resolution() {
        let preset = ExportPreset::youtube_1080p();
        assert_eq!(preset.video.width, 1920);
        assert_eq!(preset.video.height, 1080);
    }

    #[test]
    fn youtube_4k_preset_should_have_correct_resolution() {
        let preset = ExportPreset::youtube_4k();
        assert_eq!(preset.video.width, 3840);
        assert_eq!(preset.video.height, 2160);
    }

    #[test]
    fn youtube_1080p_preset_audio_should_be_aac() {
        let preset = ExportPreset::youtube_1080p();
        assert_eq!(
            preset.audio.codec,
            AudioCodec::Aac,
            "expected AAC audio codec for youtube_1080p"
        );
        assert_eq!(preset.audio.bitrate, 192_000);
    }

    #[test]
    fn youtube_4k_preset_audio_should_be_aac() {
        let preset = ExportPreset::youtube_4k();
        assert_eq!(
            preset.audio.codec,
            AudioCodec::Aac,
            "expected AAC audio codec for youtube_4k"
        );
        assert_eq!(preset.audio.bitrate, 256_000);
    }

    #[test]
    fn youtube_1080p_preset_should_have_human_readable_name() {
        let preset = ExportPreset::youtube_1080p();
        assert!(!preset.name.is_empty(), "preset name must not be empty");
    }

    #[test]
    fn youtube_4k_preset_should_have_human_readable_name() {
        let preset = ExportPreset::youtube_4k();
        assert!(!preset.name.is_empty(), "preset name must not be empty");
    }

    #[test]
    fn custom_preset_should_apply_codec_to_builder() {
        use crate::VideoEncoder;

        let preset = ExportPreset {
            name: "custom".to_string(),
            video: VideoEncoderConfig {
                codec: VideoCodec::H264,
                width: 1280,
                height: 720,
                fps: 24.0,
                bitrate_mode: BitrateMode::Crf(23),
                pixel_format: None,
                codec_options: None,
            },
            audio: AudioEncoderConfig {
                codec: AudioCodec::Aac,
                sample_rate: 44100,
                channels: 2,
                bitrate: 128_000,
            },
        };

        let builder = preset.apply_video(VideoEncoder::create("out.mp4"));
        assert_eq!(builder.video_codec, VideoCodec::H264);
        assert_eq!(builder.video_width, Some(1280));
        assert_eq!(builder.video_height, Some(720));
    }

    #[test]
    fn apply_audio_should_set_sample_rate_and_bitrate() {
        use crate::VideoEncoder;

        let preset = ExportPreset::youtube_1080p();
        let builder = preset.apply_audio(VideoEncoder::create("out.mp4"));
        assert_eq!(builder.audio_sample_rate, Some(48000));
        assert_eq!(builder.audio_bitrate, Some(192_000));
    }

    #[test]
    fn youtube_1080p_preset_should_pass_validation() {
        let preset = ExportPreset::youtube_1080p();
        assert!(preset.validate().is_ok());
    }

    #[test]
    fn youtube_4k_preset_should_pass_validation() {
        let preset = ExportPreset::youtube_4k();
        assert!(preset.validate().is_ok());
    }
}
