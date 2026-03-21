//! Audio encoder builder and public API.
//!
//! This module provides [`AudioEncoderBuilder`] for fluent configuration and
//! [`AudioEncoder`] for encoding audio frames to a file.

use std::path::PathBuf;
use std::time::Instant;

use ff_format::AudioFrame;

use super::codec_options::AudioCodecOptions;
use super::encoder_inner::{AudioEncoderConfig, AudioEncoderInner};
use crate::{AudioCodec, Container, EncodeError};

/// Builder for constructing an [`AudioEncoder`].
///
/// Created by calling [`AudioEncoder::create()`]. Call [`build()`](Self::build)
/// to open the output file and prepare for encoding.
///
/// # Examples
///
/// ```ignore
/// use ff_encode::{AudioEncoder, AudioCodec};
///
/// let mut encoder = AudioEncoder::create("output.m4a")
///     .audio(48000, 2)
///     .audio_codec(AudioCodec::Aac)
///     .build()?;
/// ```
pub struct AudioEncoderBuilder {
    pub(crate) path: PathBuf,
    pub(crate) container: Option<Container>,
    pub(crate) audio_sample_rate: Option<u32>,
    pub(crate) audio_channels: Option<u32>,
    pub(crate) audio_codec: AudioCodec,
    pub(crate) audio_bitrate: Option<u64>,
    pub(crate) codec_options: Option<AudioCodecOptions>,
}

impl AudioEncoderBuilder {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            container: None,
            audio_sample_rate: None,
            audio_channels: None,
            audio_codec: AudioCodec::default(),
            audio_bitrate: None,
            codec_options: None,
        }
    }

    /// Configure audio stream settings.
    #[must_use]
    pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
        self.audio_sample_rate = Some(sample_rate);
        self.audio_channels = Some(channels);
        self
    }

    /// Set audio codec.
    #[must_use]
    pub fn audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = codec;
        self
    }

    /// Set audio bitrate in bits per second.
    #[must_use]
    pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
        self.audio_bitrate = Some(bitrate);
        self
    }

    /// Set container format explicitly (usually auto-detected from file extension).
    #[must_use]
    pub fn container(mut self, container: Container) -> Self {
        self.container = Some(container);
        self
    }

    /// Set per-codec encoding options.
    ///
    /// The variant must match the codec set via [`audio_codec()`](Self::audio_codec).
    /// A mismatch is silently ignored.
    #[must_use]
    pub fn codec_options(mut self, opts: AudioCodecOptions) -> Self {
        self.codec_options = Some(opts);
        self
    }

    /// Validate builder state and open the output file.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if configuration is invalid, the output path
    /// cannot be created, or no suitable encoder is found.
    pub fn build(self) -> Result<AudioEncoder, EncodeError> {
        AudioEncoder::from_builder(self)
    }
}

/// Encodes audio frames to a file using FFmpeg.
///
/// # Construction
///
/// Use [`AudioEncoder::create()`] to get an [`AudioEncoderBuilder`], then call
/// [`AudioEncoderBuilder::build()`]:
///
/// ```ignore
/// use ff_encode::{AudioEncoder, AudioCodec};
///
/// let mut encoder = AudioEncoder::create("output.m4a")
///     .audio(48000, 2)
///     .audio_codec(AudioCodec::Aac)
///     .build()?;
/// ```
pub struct AudioEncoder {
    inner: Option<AudioEncoderInner>,
    _config: AudioEncoderConfig,
    _start_time: Instant,
}

impl AudioEncoder {
    /// Creates a builder for the specified output file path.
    ///
    /// This method is infallible. Validation occurs when
    /// [`AudioEncoderBuilder::build()`] is called.
    pub fn create<P: AsRef<std::path::Path>>(path: P) -> AudioEncoderBuilder {
        AudioEncoderBuilder::new(path.as_ref().to_path_buf())
    }

    pub(crate) fn from_builder(builder: AudioEncoderBuilder) -> Result<Self, EncodeError> {
        // Validate per-codec options before constructing the inner encoder.
        if let Some(AudioCodecOptions::Opus(ref opts)) = builder.codec_options
            && let Some(dur) = opts.frame_duration_ms
            && ![2u32, 5, 10, 20, 40, 60].contains(&dur)
        {
            return Err(EncodeError::InvalidOption {
                name: "frame_duration_ms".to_string(),
                reason: "must be one of: 2, 5, 10, 20, 40, 60".to_string(),
            });
        }

        let config = AudioEncoderConfig {
            path: builder.path.clone(),
            sample_rate: builder
                .audio_sample_rate
                .ok_or_else(|| EncodeError::InvalidConfig {
                    reason: "Audio sample rate not configured".to_string(),
                })?,
            channels: builder
                .audio_channels
                .ok_or_else(|| EncodeError::InvalidConfig {
                    reason: "Audio channels not configured".to_string(),
                })?,
            codec: builder.audio_codec,
            bitrate: builder.audio_bitrate,
            codec_options: builder.codec_options,
            _progress_callback: false,
        };

        let inner = Some(AudioEncoderInner::new(&config)?);

        Ok(Self {
            inner,
            _config: config,
            _start_time: Instant::now(),
        })
    }

    /// Returns the name of the FFmpeg encoder actually used (e.g. `"aac"`, `"libopus"`).
    #[must_use]
    pub fn actual_codec(&self) -> &str {
        self.inner
            .as_ref()
            .map_or("", |inner| inner.actual_codec.as_str())
    }

    /// Pushes an audio frame for encoding.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if encoding fails or the encoder is not initialised.
    pub fn push(&mut self, frame: &AudioFrame) -> Result<(), EncodeError> {
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Audio encoder not initialized".to_string(),
            })?;
        // SAFETY: inner is properly initialised and we have exclusive access.
        unsafe { inner.push_frame(frame)? };
        Ok(())
    }

    /// Flushes remaining frames and writes the file trailer.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if finalising fails.
    pub fn finish(mut self) -> Result<(), EncodeError> {
        if let Some(mut inner) = self.inner.take() {
            // SAFETY: inner is properly initialised and we have exclusive access.
            unsafe { inner.finish()? };
        }
        Ok(())
    }
}

impl Drop for AudioEncoder {
    fn drop(&mut self) {
        // AudioEncoderInner handles cleanup in its own Drop.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_should_return_builder_without_error() {
        let _builder: AudioEncoderBuilder = AudioEncoder::create("output.m4a");
    }

    #[test]
    fn builder_audio_settings_should_be_stored() {
        let builder = AudioEncoder::create("output.m4a")
            .audio(48000, 2)
            .audio_codec(AudioCodec::Aac)
            .audio_bitrate(192_000);
        assert_eq!(builder.audio_sample_rate, Some(48000));
        assert_eq!(builder.audio_channels, Some(2));
        assert_eq!(builder.audio_codec, AudioCodec::Aac);
        assert_eq!(builder.audio_bitrate, Some(192_000));
    }

    #[test]
    fn build_without_sample_rate_should_return_error() {
        let result = AudioEncoder::create("output.m4a").build();
        assert!(result.is_err());
    }
}
