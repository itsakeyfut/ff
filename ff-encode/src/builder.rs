//! Encoder builder implementation.

use crate::{
    AudioCodec, Container, EncodeError, HardwareEncoder, Preset, ProgressCallback, VideoCodec,
};
use std::path::PathBuf;

/// Builder for constructing a [`VideoEncoder`](crate::VideoEncoder) or [`AudioEncoder`](crate::AudioEncoder).
///
/// Provides a fluent API for configuring encoding parameters before
/// creating the actual encoder instance.
///
/// # Examples
///
/// ```ignore
/// use ff_encode::{VideoEncoder, VideoCodec, AudioCodec, Preset};
///
/// let encoder = VideoEncoder::create("output.mp4")?
///     .video(1920, 1080, 30.0)
///     .video_codec(VideoCodec::H264)
///     .video_bitrate(8_000_000)
///     .preset(Preset::Medium)
///     .audio(48000, 2)
///     .audio_codec(AudioCodec::Aac)
///     .audio_bitrate(192_000)
///     .build()?;
/// ```
pub struct EncoderBuilder {
    /// Output file path
    pub(crate) path: PathBuf,

    /// Container format (auto-detected if None)
    pub(crate) container: Option<Container>,

    // Video settings
    /// Video width in pixels
    pub(crate) video_width: Option<u32>,
    /// Video height in pixels
    pub(crate) video_height: Option<u32>,
    /// Video frame rate (frames per second)
    pub(crate) video_fps: Option<f64>,
    /// Video codec
    pub(crate) video_codec: VideoCodec,
    /// Video bitrate in bits per second
    pub(crate) video_bitrate: Option<u64>,
    /// Video quality (CRF: 0-51, lower is higher quality)
    pub(crate) video_quality: Option<u32>,
    /// Encoding preset
    pub(crate) preset: Preset,
    /// Hardware encoder
    pub(crate) hardware_encoder: HardwareEncoder,

    // Audio settings
    /// Audio sample rate in Hz
    pub(crate) audio_sample_rate: Option<u32>,
    /// Number of audio channels
    pub(crate) audio_channels: Option<u32>,
    /// Audio codec
    pub(crate) audio_codec: AudioCodec,
    /// Audio bitrate in bits per second
    pub(crate) audio_bitrate: Option<u64>,

    // Callbacks
    /// Progress callback handler
    pub(crate) progress_callback: Option<Box<dyn ProgressCallback>>,
}

impl std::fmt::Debug for EncoderBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncoderBuilder")
            .field("path", &self.path)
            .field("container", &self.container)
            .field("video_width", &self.video_width)
            .field("video_height", &self.video_height)
            .field("video_fps", &self.video_fps)
            .field("video_codec", &self.video_codec)
            .field("video_bitrate", &self.video_bitrate)
            .field("video_quality", &self.video_quality)
            .field("preset", &self.preset)
            .field("hardware_encoder", &self.hardware_encoder)
            .field("audio_sample_rate", &self.audio_sample_rate)
            .field("audio_channels", &self.audio_channels)
            .field("audio_codec", &self.audio_codec)
            .field("audio_bitrate", &self.audio_bitrate)
            .field(
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

impl EncoderBuilder {
    /// Create a new encoder builder with the given output path.
    ///
    /// # Arguments
    ///
    /// * `path` - Output file path
    ///
    /// # Errors
    ///
    /// Returns an error if the output path is invalid.
    pub fn new(path: PathBuf) -> Result<Self, EncodeError> {
        // Validate path has a parent directory
        if path.parent().is_none() {
            return Err(EncodeError::InvalidConfig {
                reason: "Output path must have a parent directory".to_string(),
            });
        }

        Ok(Self {
            path,
            container: None,
            video_width: None,
            video_height: None,
            video_fps: None,
            video_codec: VideoCodec::default(),
            video_bitrate: None,
            video_quality: None,
            preset: Preset::default(),
            hardware_encoder: HardwareEncoder::default(),
            audio_sample_rate: None,
            audio_channels: None,
            audio_codec: AudioCodec::default(),
            audio_bitrate: None,
            progress_callback: None,
        })
    }

    // === Video settings ===

    /// Configure video stream settings.
    ///
    /// # Arguments
    ///
    /// * `width` - Video width in pixels
    /// * `height` - Video height in pixels
    /// * `fps` - Frame rate in frames per second
    #[must_use]
    pub fn video(mut self, width: u32, height: u32, fps: f64) -> Self {
        self.video_width = Some(width);
        self.video_height = Some(height);
        self.video_fps = Some(fps);
        self
    }

    /// Set video codec.
    ///
    /// # Arguments
    ///
    /// * `codec` - Video codec to use
    #[must_use]
    pub fn video_codec(mut self, codec: VideoCodec) -> Self {
        self.video_codec = codec;
        self
    }

    /// Set video bitrate in bits per second.
    ///
    /// # Arguments
    ///
    /// * `bitrate` - Target bitrate in bps (e.g., `8_000_000` for 8 Mbps)
    #[must_use]
    pub fn video_bitrate(mut self, bitrate: u64) -> Self {
        self.video_bitrate = Some(bitrate);
        self
    }

    /// Set video quality using CRF (Constant Rate Factor).
    ///
    /// # Arguments
    ///
    /// * `crf` - Quality value (0-51, lower is higher quality)
    ///
    /// Note: CRF mode typically produces better quality than constant bitrate
    /// for the same file size, but the final file size is less predictable.
    #[must_use]
    pub fn video_quality(mut self, crf: u32) -> Self {
        self.video_quality = Some(crf);
        self
    }

    /// Set encoding preset (speed vs quality tradeoff).
    ///
    /// # Arguments
    ///
    /// * `preset` - Encoding preset
    #[must_use]
    pub fn preset(mut self, preset: Preset) -> Self {
        self.preset = preset;
        self
    }

    /// Set hardware encoder.
    ///
    /// # Arguments
    ///
    /// * `hw` - Hardware encoder to use
    #[must_use]
    pub fn hardware_encoder(mut self, hw: HardwareEncoder) -> Self {
        self.hardware_encoder = hw;
        self
    }

    // === Audio settings ===

    /// Configure audio stream settings.
    ///
    /// # Arguments
    ///
    /// * `sample_rate` - Sample rate in Hz (e.g., 48000)
    /// * `channels` - Number of channels (1 = mono, 2 = stereo)
    #[must_use]
    pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
        self.audio_sample_rate = Some(sample_rate);
        self.audio_channels = Some(channels);
        self
    }

    /// Set audio codec.
    ///
    /// # Arguments
    ///
    /// * `codec` - Audio codec to use
    #[must_use]
    pub fn audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = codec;
        self
    }

    /// Set audio bitrate in bits per second.
    ///
    /// # Arguments
    ///
    /// * `bitrate` - Target bitrate in bps (e.g., `192_000` for 192 kbps)
    #[must_use]
    pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
        self.audio_bitrate = Some(bitrate);
        self
    }

    // === Container settings ===

    /// Set container format explicitly.
    ///
    /// # Arguments
    ///
    /// * `container` - Container format
    ///
    /// Note: Usually not needed as the format is auto-detected from file extension.
    #[must_use]
    pub fn container(mut self, container: Container) -> Self {
        self.container = Some(container);
        self
    }

    // === Callbacks ===

    /// Set progress callback using a closure or function.
    ///
    /// This is a convenience method for simple progress callbacks.
    /// For cancellation support, use `progress_callback()` instead.
    ///
    /// # Arguments
    ///
    /// * `callback` - Closure or function to call with progress updates
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::VideoEncoder;
    ///
    /// let encoder = VideoEncoder::create("output.mp4")?
    ///     .video(1920, 1080, 30.0)
    ///     .on_progress(|progress| {
    ///         println!("Progress: {:.1}%", progress.percent());
    ///     })
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn on_progress<F>(mut self, callback: F) -> Self
    where
        F: FnMut(&crate::Progress) + Send + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Set progress callback using a trait object.
    ///
    /// Use this method when you need cancellation support or want to
    /// use a custom struct implementing `ProgressCallback`.
    ///
    /// # Arguments
    ///
    /// * `callback` - Progress callback handler
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::{VideoEncoder, ProgressCallback, Progress};
    /// use std::sync::Arc;
    /// use std::sync::atomic::{AtomicBool, Ordering};
    ///
    /// struct CancellableProgress {
    ///     cancelled: Arc<AtomicBool>,
    /// }
    ///
    /// impl ProgressCallback for CancellableProgress {
    ///     fn on_progress(&mut self, progress: &Progress) {
    ///         println!("Progress: {:.1}%", progress.percent());
    ///     }
    ///
    ///     fn should_cancel(&self) -> bool {
    ///         self.cancelled.load(Ordering::Relaxed)
    ///     }
    /// }
    ///
    /// let cancelled = Arc::new(AtomicBool::new(false));
    /// let encoder = VideoEncoder::create("output.mp4")?
    ///     .video(1920, 1080, 30.0)
    ///     .progress_callback(CancellableProgress {
    ///         cancelled: cancelled.clone()
    ///     })
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn progress_callback<C: ProgressCallback + 'static>(mut self, callback: C) -> Self {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    // === Build ===

    /// Build the encoder.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration is invalid
    /// - The output file cannot be created
    /// - No suitable encoder is found for the requested codec
    pub fn build(self) -> Result<crate::VideoEncoder, EncodeError> {
        // Validate configuration
        self.validate()?;

        // Build encoder
        crate::VideoEncoder::from_builder(self)
    }

    /// Validate the builder configuration.
    fn validate(&self) -> Result<(), EncodeError> {
        // Check if at least one stream is configured
        let has_video =
            self.video_width.is_some() && self.video_height.is_some() && self.video_fps.is_some();
        let has_audio = self.audio_sample_rate.is_some() && self.audio_channels.is_some();

        if !has_video && !has_audio {
            return Err(EncodeError::InvalidConfig {
                reason: "At least one video or audio stream must be configured".to_string(),
            });
        }

        // Validate video settings
        if has_video {
            if let Some(width) = self.video_width
                && (width == 0 || width % 2 != 0)
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video width must be non-zero and even, got {width}"),
                });
            }

            if let Some(height) = self.video_height
                && (height == 0 || height % 2 != 0)
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video height must be non-zero and even, got {height}"),
                });
            }

            if let Some(fps) = self.video_fps
                && fps <= 0.0
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video FPS must be positive, got {fps}"),
                });
            }

            if let Some(quality) = self.video_quality
                && quality > 51
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video quality (CRF) must be 0-51, got {quality}"),
                });
            }
        }

        // Validate audio settings
        if has_audio {
            if let Some(sample_rate) = self.audio_sample_rate
                && sample_rate == 0
            {
                return Err(EncodeError::InvalidConfig {
                    reason: "Audio sample rate must be non-zero".to_string(),
                });
            }

            if let Some(channels) = self.audio_channels
                && channels == 0
            {
                return Err(EncodeError::InvalidConfig {
                    reason: "Audio channels must be non-zero".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Build an audio-only encoder with the configured settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid or encoder creation fails.
    pub fn build_audio(self) -> Result<crate::AudioEncoder, EncodeError> {
        crate::AudioEncoder::from_builder(self)
    }
}

#[cfg(test)]
// Tests are allowed to use unwrap() for simplicity
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_video_only() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, 30.0)
            .video_codec(VideoCodec::H264)
            .video_bitrate(8_000_000);

        assert_eq!(builder.video_width, Some(1920));
        assert_eq!(builder.video_height, Some(1080));
        assert_eq!(builder.video_fps, Some(30.0));
        assert_eq!(builder.video_codec, VideoCodec::H264);
        assert_eq!(builder.video_bitrate, Some(8_000_000));
    }

    #[test]
    fn test_builder_audio_only() {
        let builder = EncoderBuilder::new("output.mp3".into())
            .unwrap()
            .audio(48000, 2)
            .audio_codec(AudioCodec::Mp3)
            .audio_bitrate(192_000);

        assert_eq!(builder.audio_sample_rate, Some(48000));
        assert_eq!(builder.audio_channels, Some(2));
        assert_eq!(builder.audio_codec, AudioCodec::Mp3);
        assert_eq!(builder.audio_bitrate, Some(192_000));
    }

    #[test]
    fn test_builder_both_streams() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, 30.0)
            .audio(48000, 2);

        assert_eq!(builder.video_width, Some(1920));
        assert_eq!(builder.audio_sample_rate, Some(48000));
    }

    #[test]
    fn test_builder_preset() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, 30.0)
            .preset(Preset::Fast);

        assert_eq!(builder.preset, Preset::Fast);
    }

    #[test]
    fn test_builder_hardware_encoder() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, 30.0)
            .hardware_encoder(HardwareEncoder::Nvenc);

        assert_eq!(builder.hardware_encoder, HardwareEncoder::Nvenc);
    }

    #[test]
    fn test_builder_container() {
        let builder = EncoderBuilder::new("output.video".into())
            .unwrap()
            .video(1920, 1080, 30.0)
            .container(Container::Mp4);

        assert_eq!(builder.container, Some(Container::Mp4));
    }

    #[test]
    fn test_validate_no_streams() {
        let builder = EncoderBuilder::new("output.mp4".into()).unwrap();
        let result = builder.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_odd_width() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1921, 1080, 30.0);
        let result = builder.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_odd_height() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1081, 30.0);
        let result = builder.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_fps() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, -1.0);
        let result = builder.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_quality() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, 30.0)
            .video_quality(100);
        let result = builder.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_valid_config() {
        let builder = EncoderBuilder::new("output.mp4".into())
            .unwrap()
            .video(1920, 1080, 30.0);
        let result = builder.validate();
        assert!(result.is_ok());
    }
}
