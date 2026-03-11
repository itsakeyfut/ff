//! Audio encoder public API.
//!
//! This module provides the public interface for audio encoding operations.

use crate::{EncodeError, EncoderBuilder};
use ff_format::AudioFrame;
use std::time::Instant;

use super::encoder_inner::{AudioEncoderConfig, AudioEncoderInner};

/// Audio encoder.
///
/// Encodes audio frames to a file using FFmpeg.
///
/// # Examples
///
/// ```no_run
/// use ff_encode::{AudioEncoder, AudioCodec};
/// use ff_format::{AudioFrame, SampleFormat};
///
/// let mut encoder = AudioEncoder::create("output.m4a")
///     .expect("Failed to create encoder")
///     .audio(48000, 2)
///     .audio_codec(AudioCodec::Aac)
///     .build_audio()
///     .expect("Failed to build encoder");
///
/// // Create and push audio frames
/// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
/// encoder.push(&frame).expect("Failed to push frame");
///
/// encoder.finish().expect("Failed to finish encoding");
/// ```
pub struct AudioEncoder {
    inner: Option<AudioEncoderInner>,
    _config: AudioEncoderConfig,
    _start_time: Instant,
}

impl AudioEncoder {
    /// Create a new encoder builder with the given output path.
    ///
    /// # Arguments
    ///
    /// * `path` - Output file path
    ///
    /// # Returns
    ///
    /// Returns an [`EncoderBuilder`] for configuring the encoder.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::AudioEncoder;
    ///
    /// let builder = AudioEncoder::create("output.m4a")?;
    /// ```
    pub fn create<P: AsRef<std::path::Path>>(path: P) -> Result<EncoderBuilder, EncodeError> {
        EncoderBuilder::new(path.as_ref().to_path_buf())
    }

    /// Create an encoder from a builder.
    ///
    /// This is called by [`EncoderBuilder::build()`] and should not be called directly.
    pub(crate) fn from_builder(builder: EncoderBuilder) -> Result<Self, EncodeError> {
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
            _progress_callback: builder.progress_callback.is_some(),
        };

        let inner = Some(AudioEncoderInner::new(&config)?);

        Ok(Self {
            inner,
            _config: config,
            _start_time: Instant::now(),
        })
    }

    /// Get the actual audio codec being used.
    ///
    /// Returns the name of the FFmpeg encoder (e.g., "aac", "libopus").
    #[must_use]
    pub fn actual_codec(&self) -> &str {
        self.inner
            .as_ref()
            .map_or("", |inner| inner.actual_codec.as_str())
    }

    /// Push an audio frame for encoding.
    ///
    /// # Arguments
    ///
    /// * `frame` - The audio frame to encode
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails or the encoder is not initialized.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::{AudioEncoder, AudioCodec};
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let mut encoder = AudioEncoder::create("output.m4a")?
    ///     .audio(48000, 2)
    ///     .audio_codec(AudioCodec::Aac)
    ///     .build()?;
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32)?;
    /// encoder.push(&frame)?;
    /// ```
    pub fn push(&mut self, frame: &AudioFrame) -> Result<(), EncodeError> {
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Audio encoder not initialized".to_string(),
            })?;

        // SAFETY: inner is properly initialized and we have exclusive access
        unsafe { inner.push_frame(frame)? };
        Ok(())
    }

    /// Finish encoding and write the file trailer.
    ///
    /// This method must be called to properly finalize the output file.
    /// It flushes any remaining encoded frames and writes the file trailer.
    ///
    /// # Errors
    ///
    /// Returns an error if finalizing fails.
    pub fn finish(mut self) -> Result<(), EncodeError> {
        if let Some(mut inner) = self.inner.take() {
            // SAFETY: inner is properly initialized and we have exclusive access
            unsafe { inner.finish()? };
        }
        Ok(())
    }
}

impl Drop for AudioEncoder {
    fn drop(&mut self) {
        // AudioEncoderInner will handle cleanup in its Drop implementation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_encoder_builder() {
        let builder = AudioEncoder::create("output.m4a");
        assert!(builder.is_ok());
    }
}
