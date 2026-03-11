//! Video encoder public API.
//!
//! This module provides the public interface for video encoding operations.

use crate::{EncodeError, EncoderBuilder};
use ff_format::{AudioFrame, VideoFrame};
use std::time::Instant;

use super::encoder_inner::{VideoEncoderConfig, VideoEncoderInner, preset_to_string};

/// Video encoder.
///
/// Encodes video frames to a file using FFmpeg.
///
/// # Examples
///
/// ```no_run
/// use ff_encode::{VideoEncoder, VideoCodec};
/// use ff_format::VideoFrame;
///
/// let mut encoder = VideoEncoder::create("output.mp4")
///     .expect("Failed to create encoder")
///     .video(1920, 1080, 30.0)
///     .video_codec(VideoCodec::H264)
///     .build()
///     .expect("Failed to build encoder");
///
/// // Push frames
/// let frames = vec![]; // Your video frames here
/// for frame in frames {
///     encoder.push_video(&frame).expect("Failed to push frame");
/// }
///
/// encoder.finish().expect("Failed to finish encoding");
/// ```
pub struct VideoEncoder {
    inner: Option<VideoEncoderInner>,
    _config: VideoEncoderConfig,
    start_time: Instant,
    progress_callback: Option<Box<dyn crate::ProgressCallback>>,
}

impl VideoEncoder {
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
    /// use ff_encode::VideoEncoder;
    ///
    /// let builder = VideoEncoder::create("output.mp4")?;
    /// ```
    pub fn create<P: AsRef<std::path::Path>>(path: P) -> Result<EncoderBuilder, EncodeError> {
        EncoderBuilder::new(path.as_ref().to_path_buf())
    }

    /// Create an encoder from a builder.
    ///
    /// This is called by [`EncoderBuilder::build()`] and should not be called directly.
    pub(crate) fn from_builder(builder: EncoderBuilder) -> Result<Self, EncodeError> {
        let config = VideoEncoderConfig {
            path: builder.path.clone(),
            video_width: builder.video_width,
            video_height: builder.video_height,
            video_fps: builder.video_fps,
            video_codec: builder.video_codec,
            video_bitrate: builder.video_bitrate,
            video_quality: builder.video_quality,
            preset: preset_to_string(&builder.preset),
            hardware_encoder: builder.hardware_encoder,
            audio_sample_rate: builder.audio_sample_rate,
            audio_channels: builder.audio_channels,
            audio_codec: builder.audio_codec,
            audio_bitrate: builder.audio_bitrate,
            _progress_callback: builder.progress_callback.is_some(),
        };

        let inner = if config.video_width.is_some() {
            Some(VideoEncoderInner::new(&config)?)
        } else {
            None
        };

        Ok(Self {
            inner,
            _config: config,
            start_time: Instant::now(),
            progress_callback: builder.progress_callback,
        })
    }

    /// Get the actual video codec being used.
    ///
    /// Returns the name of the FFmpeg encoder (e.g., "h264_nvenc", "libx264").
    #[must_use]
    pub fn actual_video_codec(&self) -> &str {
        self.inner
            .as_ref()
            .map_or("", |inner| inner.actual_video_codec.as_str())
    }

    /// Get the actual audio codec being used.
    ///
    /// Returns the name of the FFmpeg encoder (e.g., "aac", "libopus").
    #[must_use]
    pub fn actual_audio_codec(&self) -> &str {
        self.inner
            .as_ref()
            .map_or("", |inner| inner.actual_audio_codec.as_str())
    }

    /// Get the hardware encoder actually being used.
    ///
    /// Returns the hardware encoder type that is actually being used for encoding.
    /// This may differ from what was requested if the requested encoder is not available.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::{VideoEncoder, VideoCodec, HardwareEncoder};
    ///
    /// let encoder = VideoEncoder::create("output.mp4")?
    ///     .video(1920, 1080, 30.0)
    ///     .video_codec(VideoCodec::H264)
    ///     .hardware_encoder(HardwareEncoder::Auto)
    ///     .build()?;
    ///
    /// println!("Using hardware encoder: {:?}", encoder.hardware_encoder());
    /// ```
    #[must_use]
    pub fn hardware_encoder(&self) -> crate::HardwareEncoder {
        let codec_name = self.actual_video_codec();

        // Detect hardware encoder from codec name
        if codec_name.contains("nvenc") {
            crate::HardwareEncoder::Nvenc
        } else if codec_name.contains("qsv") {
            crate::HardwareEncoder::Qsv
        } else if codec_name.contains("amf") {
            crate::HardwareEncoder::Amf
        } else if codec_name.contains("videotoolbox") {
            crate::HardwareEncoder::VideoToolbox
        } else if codec_name.contains("vaapi") {
            crate::HardwareEncoder::Vaapi
        } else {
            crate::HardwareEncoder::None
        }
    }

    /// Check if hardware encoding is being used.
    ///
    /// Returns `true` if the encoder is using hardware acceleration,
    /// `false` if using software encoding.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::{VideoEncoder, VideoCodec, HardwareEncoder};
    ///
    /// let encoder = VideoEncoder::create("output.mp4")?
    ///     .video(1920, 1080, 30.0)
    ///     .video_codec(VideoCodec::H264)
    ///     .hardware_encoder(HardwareEncoder::Auto)
    ///     .build()?;
    ///
    /// if encoder.is_hardware_encoding() {
    ///     println!("Using hardware encoder: {}", encoder.actual_video_codec());
    /// } else {
    ///     println!("Using software encoder: {}", encoder.actual_video_codec());
    /// }
    /// ```
    #[must_use]
    pub fn is_hardware_encoding(&self) -> bool {
        !matches!(self.hardware_encoder(), crate::HardwareEncoder::None)
    }

    /// Check if the actually selected video encoder is LGPL-compliant.
    ///
    /// Returns `true` if the encoder is safe for commercial use without licensing fees.
    /// Returns `false` for GPL encoders that require licensing.
    ///
    /// # LGPL-Compatible Encoders (Commercial Use OK)
    ///
    /// - **Hardware encoders**: h264_nvenc, h264_qsv, h264_amf, h264_videotoolbox, h264_vaapi
    /// - **Royalty-free codecs**: libvpx-vp9, libaom-av1, libsvtav1
    /// - **Professional codecs**: prores_ks, dnxhd
    ///
    /// # GPL Encoders (Licensing Required)
    ///
    /// - **libx264**: Requires MPEG LA H.264 license for commercial distribution
    /// - **libx265**: Requires MPEG LA H.265 license for commercial distribution
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::{VideoEncoder, VideoCodec, HardwareEncoder};
    ///
    /// // Default: Will use hardware encoder or VP9 fallback (LGPL-compliant)
    /// let encoder = VideoEncoder::create("output.mp4")?
    ///     .video(1920, 1080, 30.0)
    ///     .video_codec(VideoCodec::H264)
    ///     .build()?;
    ///
    /// if encoder.is_lgpl_compliant() {
    ///     println!("✓ Safe for commercial use: {}", encoder.actual_video_codec());
    /// } else {
    ///     println!("⚠ GPL encoder (requires licensing): {}", encoder.actual_video_codec());
    /// }
    /// ```
    ///
    /// # Note
    ///
    /// By default (without `gpl` feature), this will always return `true` because
    /// the encoder automatically selects LGPL-compatible alternatives.
    #[must_use]
    pub fn is_lgpl_compliant(&self) -> bool {
        let codec_name = self.actual_video_codec();

        // Hardware encoders are LGPL-compatible
        if codec_name.contains("nvenc")
            || codec_name.contains("qsv")
            || codec_name.contains("amf")
            || codec_name.contains("videotoolbox")
            || codec_name.contains("vaapi")
        {
            return true;
        }

        // LGPL-compatible software encoders
        if codec_name.contains("vp9")
            || codec_name.contains("av1")
            || codec_name.contains("aom")
            || codec_name.contains("svt")
            || codec_name.contains("prores")
            || codec_name == "mpeg4"
            || codec_name == "dnxhd"
        {
            return true;
        }

        // GPL encoders
        if codec_name == "libx264" || codec_name == "libx265" {
            return false;
        }

        // Default to true for unknown encoders (conservative approach)
        true
    }

    /// Push a video frame for encoding.
    ///
    /// # Arguments
    ///
    /// * `frame` - The video frame to encode
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails or the encoder is not initialized.
    /// Returns `EncodeError::Cancelled` if the progress callback requested cancellation.
    pub fn push_video(&mut self, frame: &VideoFrame) -> Result<(), EncodeError> {
        // Check for cancellation before encoding
        if let Some(ref callback) = self.progress_callback
            && callback.should_cancel()
        {
            return Err(EncodeError::Cancelled);
        }

        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Video encoder not initialized".to_string(),
            })?;

        // SAFETY: inner is properly initialized and we have exclusive access
        unsafe { inner.push_video_frame(frame)? };

        // Report progress after encoding
        let progress = self.create_progress_info();
        if let Some(ref mut callback) = self.progress_callback {
            callback.on_progress(&progress);
        }

        Ok(())
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
    /// Returns `EncodeError::Cancelled` if the progress callback requested cancellation.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_encode::{VideoEncoder, AudioCodec};
    /// use ff_format::AudioFrame;
    ///
    /// let mut encoder = VideoEncoder::create("output.mp4")?
    ///     .video(1920, 1080, 30.0)
    ///     .audio(48000, 2)
    ///     .audio_codec(AudioCodec::Aac)
    ///     .build()?;
    ///
    /// // Push audio frames
    /// let frame = AudioFrame::empty(1024, 2, 48000, ff_format::SampleFormat::F32)?;
    /// encoder.push_audio(&frame)?;
    /// ```
    pub fn push_audio(&mut self, frame: &AudioFrame) -> Result<(), EncodeError> {
        // Check for cancellation before encoding
        if let Some(ref callback) = self.progress_callback
            && callback.should_cancel()
        {
            return Err(EncodeError::Cancelled);
        }

        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Audio encoder not initialized".to_string(),
            })?;

        // SAFETY: inner is properly initialized and we have exclusive access
        unsafe { inner.push_audio_frame(frame)? };

        // Report progress after encoding
        let progress = self.create_progress_info();
        if let Some(ref mut callback) = self.progress_callback {
            callback.on_progress(&progress);
        }

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

    /// Create progress information from current encoder state.
    fn create_progress_info(&self) -> crate::Progress {
        let elapsed = self.start_time.elapsed();

        let (frames_encoded, bytes_written) = self
            .inner
            .as_ref()
            .map_or((0, 0), |inner| (inner.frame_count, inner.bytes_written));

        // Calculate current FPS
        #[allow(clippy::cast_precision_loss)]
        let current_fps = if !elapsed.is_zero() {
            frames_encoded as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        // Calculate current bitrate
        #[allow(clippy::cast_precision_loss)]
        let current_bitrate = if !elapsed.is_zero() {
            let elapsed_secs = elapsed.as_secs();
            if elapsed_secs > 0 {
                (bytes_written * 8) / elapsed_secs
            } else {
                // Less than 1 second elapsed, use fractional seconds
                ((bytes_written * 8) as f64 / elapsed.as_secs_f64()) as u64
            }
        } else {
            0
        };

        // We don't know total frames without user input, so this is None for now
        let total_frames = None;
        let remaining = None;

        crate::Progress {
            frames_encoded,
            total_frames,
            bytes_written,
            current_bitrate,
            elapsed,
            remaining,
            current_fps,
        }
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        // VideoEncoderInner will handle cleanup in its Drop implementation
    }
}

#[cfg(test)]
mod tests {
    use super::super::encoder_inner::{VideoEncoderConfig, VideoEncoderInner};
    use super::*;
    use crate::HardwareEncoder;

    /// Helper function to create a mock encoder for testing.
    fn create_mock_encoder(video_codec_name: &str, audio_codec_name: &str) -> VideoEncoder {
        VideoEncoder {
            inner: Some(VideoEncoderInner {
                format_ctx: std::ptr::null_mut(),
                video_codec_ctx: None,
                audio_codec_ctx: None,
                video_stream_index: -1,
                audio_stream_index: -1,
                sws_ctx: None,
                swr_ctx: None,
                frame_count: 0,
                audio_sample_count: 0,
                bytes_written: 0,
                actual_video_codec: video_codec_name.to_string(),
                actual_audio_codec: audio_codec_name.to_string(),
                last_src_width: None,
                last_src_height: None,
                last_src_format: None,
            }),
            _config: VideoEncoderConfig {
                path: "test.mp4".into(),
                video_width: Some(1920),
                video_height: Some(1080),
                video_fps: Some(30.0),
                video_codec: crate::VideoCodec::H264,
                video_bitrate: None,
                video_quality: None,
                preset: "medium".to_string(),
                hardware_encoder: HardwareEncoder::Auto,
                audio_sample_rate: None,
                audio_channels: None,
                audio_codec: crate::AudioCodec::Aac,
                audio_bitrate: None,
                _progress_callback: false,
            },
            start_time: std::time::Instant::now(),
            progress_callback: None,
        }
    }

    #[test]
    fn test_create_encoder_builder() {
        let builder = VideoEncoder::create("output.mp4");
        assert!(builder.is_ok());
    }

    #[test]
    fn test_is_lgpl_compliant_hardware_encoders() {
        // Test hardware encoder names
        let test_cases = vec![
            ("h264_nvenc", true),
            ("h264_qsv", true),
            ("h264_amf", true),
            ("h264_videotoolbox", true),
            ("hevc_nvenc", true),
            ("hevc_qsv", true),
            ("hevc_vaapi", true),
        ];

        for (codec_name, expected) in test_cases {
            let encoder = create_mock_encoder(codec_name, "");

            assert_eq!(
                encoder.is_lgpl_compliant(),
                expected,
                "Failed for codec: {}",
                codec_name
            );
        }
    }

    #[test]
    fn test_is_lgpl_compliant_software_encoders() {
        // Test software encoder names
        let test_cases = vec![
            ("libx264", false),
            ("libx265", false),
            ("libvpx-vp9", true),
            ("libaom-av1", true),
            ("libsvtav1", true),
            ("prores_ks", true),
            ("mpeg4", true),
            ("dnxhd", true),
        ];

        for (codec_name, expected) in test_cases {
            let encoder = create_mock_encoder(codec_name, "");

            assert_eq!(
                encoder.is_lgpl_compliant(),
                expected,
                "Failed for codec: {}",
                codec_name
            );
        }
    }

    #[test]
    fn test_hardware_encoder_detection() {
        // Test hardware encoder detection from codec name
        let test_cases = vec![
            ("h264_nvenc", HardwareEncoder::Nvenc, true),
            ("hevc_nvenc", HardwareEncoder::Nvenc, true),
            ("h264_qsv", HardwareEncoder::Qsv, true),
            ("hevc_qsv", HardwareEncoder::Qsv, true),
            ("h264_amf", HardwareEncoder::Amf, true),
            ("h264_videotoolbox", HardwareEncoder::VideoToolbox, true),
            ("hevc_videotoolbox", HardwareEncoder::VideoToolbox, true),
            ("h264_vaapi", HardwareEncoder::Vaapi, true),
            ("hevc_vaapi", HardwareEncoder::Vaapi, true),
            ("libx264", HardwareEncoder::None, false),
            ("libx265", HardwareEncoder::None, false),
            ("libvpx-vp9", HardwareEncoder::None, false),
        ];

        for (codec_name, expected_hw, expected_is_hw) in test_cases {
            let encoder = create_mock_encoder(codec_name, "");

            assert_eq!(
                encoder.hardware_encoder(),
                expected_hw,
                "Failed for codec: {}",
                codec_name
            );
            assert_eq!(
                encoder.is_hardware_encoding(),
                expected_is_hw,
                "is_hardware_encoding failed for codec: {}",
                codec_name
            );
        }
    }
}
