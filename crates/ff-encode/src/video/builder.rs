//! Video encoder builder and public API.
//!
//! This module provides [`VideoEncoderBuilder`] for fluent configuration and
//! [`VideoEncoder`] for encoding video (and optionally audio) frames.

use std::path::PathBuf;
use std::time::Instant;

use ff_format::{AudioFrame, VideoFrame};

use super::codec_options::VideoCodecOptions;
use super::encoder_inner::{VideoEncoderConfig, VideoEncoderInner, preset_to_string};
use crate::{
    AudioCodec, Container, EncodeError, EncodeProgressCallback, HardwareEncoder, Preset, VideoCodec,
};

/// Builder for constructing a [`VideoEncoder`].
///
/// Created by calling [`VideoEncoder::create()`]. Call [`build()`](Self::build)
/// to open the output file and prepare for encoding.
///
/// # Examples
///
/// ```ignore
/// use ff_encode::{VideoEncoder, VideoCodec, Preset};
///
/// let mut encoder = VideoEncoder::create("output.mp4")
///     .video(1920, 1080, 30.0)
///     .video_codec(VideoCodec::H264)
///     .preset(Preset::Medium)
///     .build()?;
/// ```
pub struct VideoEncoderBuilder {
    pub(crate) path: PathBuf,
    pub(crate) container: Option<Container>,
    pub(crate) video_width: Option<u32>,
    pub(crate) video_height: Option<u32>,
    pub(crate) video_fps: Option<f64>,
    pub(crate) video_codec: VideoCodec,
    pub(crate) video_bitrate_mode: Option<crate::BitrateMode>,
    pub(crate) preset: Preset,
    pub(crate) hardware_encoder: HardwareEncoder,
    pub(crate) audio_sample_rate: Option<u32>,
    pub(crate) audio_channels: Option<u32>,
    pub(crate) audio_codec: AudioCodec,
    pub(crate) audio_bitrate: Option<u64>,
    pub(crate) progress_callback: Option<Box<dyn EncodeProgressCallback>>,
    pub(crate) two_pass: bool,
    pub(crate) metadata: Vec<(String, String)>,
    pub(crate) chapters: Vec<ff_format::chapter::ChapterInfo>,
    pub(crate) subtitle_passthrough: Option<(String, usize)>,
    pub(crate) codec_options: Option<VideoCodecOptions>,
}

impl std::fmt::Debug for VideoEncoderBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoEncoderBuilder")
            .field("path", &self.path)
            .field("container", &self.container)
            .field("video_width", &self.video_width)
            .field("video_height", &self.video_height)
            .field("video_fps", &self.video_fps)
            .field("video_codec", &self.video_codec)
            .field("video_bitrate_mode", &self.video_bitrate_mode)
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
            .field("two_pass", &self.two_pass)
            .field("metadata", &self.metadata)
            .field("chapters", &self.chapters)
            .field("subtitle_passthrough", &self.subtitle_passthrough)
            .field("codec_options", &self.codec_options)
            .finish()
    }
}

impl VideoEncoderBuilder {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            container: None,
            video_width: None,
            video_height: None,
            video_fps: None,
            video_codec: VideoCodec::default(),
            video_bitrate_mode: None,
            preset: Preset::default(),
            hardware_encoder: HardwareEncoder::default(),
            audio_sample_rate: None,
            audio_channels: None,
            audio_codec: AudioCodec::default(),
            audio_bitrate: None,
            progress_callback: None,
            two_pass: false,
            metadata: Vec::new(),
            chapters: Vec::new(),
            subtitle_passthrough: None,
            codec_options: None,
        }
    }

    // === Video settings ===

    /// Configure video stream settings.
    #[must_use]
    pub fn video(mut self, width: u32, height: u32, fps: f64) -> Self {
        self.video_width = Some(width);
        self.video_height = Some(height);
        self.video_fps = Some(fps);
        self
    }

    /// Set video codec.
    #[must_use]
    pub fn video_codec(mut self, codec: VideoCodec) -> Self {
        self.video_codec = codec;
        self
    }

    /// Set the bitrate control mode for video encoding.
    #[must_use]
    pub fn bitrate_mode(mut self, mode: crate::BitrateMode) -> Self {
        self.video_bitrate_mode = Some(mode);
        self
    }

    /// Set encoding preset (speed vs quality tradeoff).
    #[must_use]
    pub fn preset(mut self, preset: Preset) -> Self {
        self.preset = preset;
        self
    }

    /// Set hardware encoder.
    #[must_use]
    pub fn hardware_encoder(mut self, hw: HardwareEncoder) -> Self {
        self.hardware_encoder = hw;
        self
    }

    // === Audio settings ===

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

    // === Container settings ===

    /// Set container format explicitly (usually auto-detected from file extension).
    #[must_use]
    pub fn container(mut self, container: Container) -> Self {
        self.container = Some(container);
        self
    }

    // === Callbacks ===

    /// Set a closure as the progress callback.
    #[must_use]
    pub fn on_progress<F>(mut self, callback: F) -> Self
    where
        F: FnMut(&crate::EncodeProgress) + Send + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Set a [`EncodeProgressCallback`] trait object (supports cancellation).
    #[must_use]
    pub fn progress_callback<C: EncodeProgressCallback + 'static>(mut self, callback: C) -> Self {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    // === Two-pass ===

    /// Enable two-pass encoding for more accurate bitrate distribution.
    ///
    /// Two-pass encoding is video-only and is incompatible with audio streams.
    #[must_use]
    pub fn two_pass(mut self) -> Self {
        self.two_pass = true;
        self
    }

    // === Metadata ===

    /// Embed a metadata tag in the output container.
    ///
    /// Calls `av_dict_set` on `AVFormatContext->metadata` before the header
    /// is written. Multiple calls accumulate entries; duplicate keys use the
    /// last value.
    #[must_use]
    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.push((key.to_string(), value.to_string()));
        self
    }

    // === Chapters ===

    /// Add a chapter to the output container.
    ///
    /// Allocates an `AVChapter` entry on `AVFormatContext` before the header
    /// is written. Multiple calls accumulate chapters in the order added.
    #[must_use]
    pub fn chapter(mut self, chapter: ff_format::chapter::ChapterInfo) -> Self {
        self.chapters.push(chapter);
        self
    }

    // === Subtitle passthrough ===

    /// Copy a subtitle stream from an existing file into the output container.
    ///
    /// Opens `source_path`, locates the stream at `stream_index`, and registers it
    /// as a passthrough stream in the output.  Packets are copied verbatim using
    /// `av_interleaved_write_frame` without re-encoding.
    ///
    /// `stream_index` is the zero-based index of the subtitle stream inside
    /// `source_path`.  For files with a single subtitle track this is typically `0`
    /// (or whichever index `ffprobe` reports).
    ///
    /// If the source cannot be opened or the stream index is invalid, a warning is
    /// logged and encoding continues without subtitles.
    #[must_use]
    pub fn subtitle_passthrough(mut self, source_path: &str, stream_index: usize) -> Self {
        self.subtitle_passthrough = Some((source_path.to_string(), stream_index));
        self
    }

    // === Per-codec options ===

    /// Set per-codec encoding options.
    ///
    /// Applied via `av_opt_set` before `avcodec_open2` during [`build()`](Self::build).
    /// This is additive — omitting it leaves codec defaults unchanged.
    /// Any option that the chosen encoder does not support is logged as a
    /// warning and skipped; it never causes `build()` to return an error.
    ///
    /// The [`VideoCodecOptions`] variant should match the codec selected via
    /// [`video_codec()`](Self::video_codec).  A mismatch is silently ignored.
    #[must_use]
    pub fn codec_options(mut self, opts: VideoCodecOptions) -> Self {
        self.codec_options = Some(opts);
        self
    }

    // === Build ===

    /// Validate builder state and open the output file.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if configuration is invalid, the output path
    /// cannot be created, or no suitable encoder is found.
    pub fn build(self) -> Result<VideoEncoder, EncodeError> {
        self.validate()?;
        VideoEncoder::from_builder(self)
    }

    fn validate(&self) -> Result<(), EncodeError> {
        let has_video =
            self.video_width.is_some() && self.video_height.is_some() && self.video_fps.is_some();
        let has_audio = self.audio_sample_rate.is_some() && self.audio_channels.is_some();

        if !has_video && !has_audio {
            return Err(EncodeError::InvalidConfig {
                reason: "At least one video or audio stream must be configured".to_string(),
            });
        }

        if self.two_pass {
            if !has_video {
                return Err(EncodeError::InvalidConfig {
                    reason: "Two-pass encoding requires a video stream".to_string(),
                });
            }
            if has_audio {
                return Err(EncodeError::InvalidConfig {
                    reason:
                        "Two-pass encoding is video-only and is incompatible with audio streams"
                            .to_string(),
                });
            }
        }

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
            if let Some(crate::BitrateMode::Crf(q)) = self.video_bitrate_mode
                && q > crate::CRF_MAX
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!(
                        "BitrateMode::Crf value must be 0-{}, got {q}",
                        crate::CRF_MAX
                    ),
                });
            }
            if let Some(crate::BitrateMode::Vbr { target, max }) = self.video_bitrate_mode
                && max < target
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("BitrateMode::Vbr max ({max}) must be >= target ({target})"),
                });
            }
        }

        if let Some(VideoCodecOptions::Av1(ref opts)) = self.codec_options
            && opts.cpu_used > 8
        {
            return Err(EncodeError::InvalidOption {
                name: "cpu_used".to_string(),
                reason: "must be 0–8".to_string(),
            });
        }

        if let Some(VideoCodecOptions::Av1Svt(ref opts)) = self.codec_options
            && opts.preset > 13
        {
            return Err(EncodeError::InvalidOption {
                name: "preset".to_string(),
                reason: "must be 0–13".to_string(),
            });
        }

        if let Some(VideoCodecOptions::Vp9(ref opts)) = self.codec_options {
            if opts.cpu_used < -8 || opts.cpu_used > 8 {
                return Err(EncodeError::InvalidOption {
                    name: "cpu_used".to_string(),
                    reason: "must be -8–8".to_string(),
                });
            }
            if let Some(cq) = opts.cq_level
                && cq > 63
            {
                return Err(EncodeError::InvalidOption {
                    name: "cq_level".to_string(),
                    reason: "must be 0–63".to_string(),
                });
            }
        }

        if let Some(VideoCodecOptions::Dnxhd(ref opts)) = self.codec_options
            && opts.variant.is_dnxhd()
        {
            let valid = matches!(
                (self.video_width, self.video_height),
                (Some(1920), Some(1080)) | (Some(1280), Some(720))
            );
            if !valid {
                return Err(EncodeError::InvalidOption {
                    name: "variant".to_string(),
                    reason: "DNxHD variants require 1920×1080 or 1280×720 resolution".to_string(),
                });
            }
        }

        if has_audio {
            if let Some(rate) = self.audio_sample_rate
                && rate == 0
            {
                return Err(EncodeError::InvalidConfig {
                    reason: "Audio sample rate must be non-zero".to_string(),
                });
            }
            if let Some(ch) = self.audio_channels
                && ch == 0
            {
                return Err(EncodeError::InvalidConfig {
                    reason: "Audio channels must be non-zero".to_string(),
                });
            }
        }

        Ok(())
    }
}

/// Encodes video (and optionally audio) frames to a file using FFmpeg.
///
/// # Construction
///
/// Use [`VideoEncoder::create()`] to get a [`VideoEncoderBuilder`], then call
/// [`VideoEncoderBuilder::build()`]:
///
/// ```ignore
/// use ff_encode::{VideoEncoder, VideoCodec};
///
/// let mut encoder = VideoEncoder::create("output.mp4")
///     .video(1920, 1080, 30.0)
///     .video_codec(VideoCodec::H264)
///     .build()?;
/// ```
pub struct VideoEncoder {
    inner: Option<VideoEncoderInner>,
    _config: VideoEncoderConfig,
    start_time: Instant,
    progress_callback: Option<Box<dyn crate::EncodeProgressCallback>>,
}

impl VideoEncoder {
    /// Creates a builder for the specified output file path.
    ///
    /// This method is infallible. Validation occurs when
    /// [`VideoEncoderBuilder::build()`] is called.
    pub fn create<P: AsRef<std::path::Path>>(path: P) -> VideoEncoderBuilder {
        VideoEncoderBuilder::new(path.as_ref().to_path_buf())
    }

    pub(crate) fn from_builder(builder: VideoEncoderBuilder) -> Result<Self, EncodeError> {
        let config = VideoEncoderConfig {
            path: builder.path.clone(),
            video_width: builder.video_width,
            video_height: builder.video_height,
            video_fps: builder.video_fps,
            video_codec: builder.video_codec,
            video_bitrate_mode: builder.video_bitrate_mode,
            preset: preset_to_string(&builder.preset),
            hardware_encoder: builder.hardware_encoder,
            audio_sample_rate: builder.audio_sample_rate,
            audio_channels: builder.audio_channels,
            audio_codec: builder.audio_codec,
            audio_bitrate: builder.audio_bitrate,
            _progress_callback: builder.progress_callback.is_some(),
            two_pass: builder.two_pass,
            metadata: builder.metadata,
            chapters: builder.chapters,
            subtitle_passthrough: builder.subtitle_passthrough,
            codec_options: builder.codec_options,
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

    /// Returns the name of the FFmpeg encoder actually used (e.g. `"h264_nvenc"`, `"libx264"`).
    #[must_use]
    pub fn actual_video_codec(&self) -> &str {
        self.inner
            .as_ref()
            .map_or("", |inner| inner.actual_video_codec.as_str())
    }

    /// Returns the name of the FFmpeg audio encoder actually used.
    #[must_use]
    pub fn actual_audio_codec(&self) -> &str {
        self.inner
            .as_ref()
            .map_or("", |inner| inner.actual_audio_codec.as_str())
    }

    /// Returns the hardware encoder actually in use.
    #[must_use]
    pub fn hardware_encoder(&self) -> crate::HardwareEncoder {
        let codec_name = self.actual_video_codec();
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

    /// Returns `true` if a hardware encoder is active.
    #[must_use]
    pub fn is_hardware_encoding(&self) -> bool {
        !matches!(self.hardware_encoder(), crate::HardwareEncoder::None)
    }

    /// Returns `true` if the selected encoder is LGPL-compatible (safe for commercial use).
    #[must_use]
    pub fn is_lgpl_compliant(&self) -> bool {
        let codec_name = self.actual_video_codec();
        if codec_name.contains("nvenc")
            || codec_name.contains("qsv")
            || codec_name.contains("amf")
            || codec_name.contains("videotoolbox")
            || codec_name.contains("vaapi")
        {
            return true;
        }
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
        if codec_name == "libx264" || codec_name == "libx265" {
            return false;
        }
        true
    }

    /// Pushes a video frame for encoding.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if encoding fails or the encoder is not initialised.
    /// Returns [`EncodeError::Cancelled`] if the progress callback requested cancellation.
    pub fn push_video(&mut self, frame: &VideoFrame) -> Result<(), EncodeError> {
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
        // SAFETY: inner is properly initialised and we have exclusive access.
        unsafe { inner.push_video_frame(frame)? };
        let progress = self.create_progress_info();
        if let Some(ref mut callback) = self.progress_callback {
            callback.on_progress(&progress);
        }
        Ok(())
    }

    /// Pushes an audio frame for encoding.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if encoding fails or the encoder is not initialised.
    pub fn push_audio(&mut self, frame: &AudioFrame) -> Result<(), EncodeError> {
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
        // SAFETY: inner is properly initialised and we have exclusive access.
        unsafe { inner.push_audio_frame(frame)? };
        let progress = self.create_progress_info();
        if let Some(ref mut callback) = self.progress_callback {
            callback.on_progress(&progress);
        }
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

    fn create_progress_info(&self) -> crate::EncodeProgress {
        let elapsed = self.start_time.elapsed();
        let (frames_encoded, bytes_written) = self
            .inner
            .as_ref()
            .map_or((0, 0), |inner| (inner.frame_count, inner.bytes_written));
        #[allow(clippy::cast_precision_loss)]
        let current_fps = if !elapsed.is_zero() {
            frames_encoded as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        #[allow(clippy::cast_precision_loss)]
        let current_bitrate = if !elapsed.is_zero() {
            let elapsed_secs = elapsed.as_secs();
            if elapsed_secs > 0 {
                (bytes_written * 8) / elapsed_secs
            } else {
                ((bytes_written * 8) as f64 / elapsed.as_secs_f64()) as u64
            }
        } else {
            0
        };
        crate::EncodeProgress {
            frames_encoded,
            total_frames: None,
            bytes_written,
            current_bitrate,
            elapsed,
            remaining: None,
            current_fps,
        }
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        // VideoEncoderInner handles cleanup in its own Drop.
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::encoder_inner::{VideoEncoderConfig, VideoEncoderInner};
    use super::*;
    use crate::HardwareEncoder;

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
                two_pass: false,
                pass1_codec_ctx: None,
                buffered_frames: Vec::new(),
                two_pass_config: None,
                stats_in_cstr: None,
                subtitle_passthrough: None,
            }),
            _config: VideoEncoderConfig {
                path: "test.mp4".into(),
                video_width: Some(1920),
                video_height: Some(1080),
                video_fps: Some(30.0),
                video_codec: crate::VideoCodec::H264,
                video_bitrate_mode: None,
                preset: "medium".to_string(),
                hardware_encoder: HardwareEncoder::Auto,
                audio_sample_rate: None,
                audio_channels: None,
                audio_codec: crate::AudioCodec::Aac,
                audio_bitrate: None,
                _progress_callback: false,
                two_pass: false,
                metadata: Vec::new(),
                chapters: Vec::new(),
                subtitle_passthrough: None,
                codec_options: None,
            },
            start_time: std::time::Instant::now(),
            progress_callback: None,
        }
    }

    #[test]
    fn create_should_return_builder_without_error() {
        let _builder: VideoEncoderBuilder = VideoEncoder::create("output.mp4");
    }

    #[test]
    fn builder_video_settings_should_be_stored() {
        let builder = VideoEncoder::create("output.mp4")
            .video(1920, 1080, 30.0)
            .video_codec(VideoCodec::H264)
            .bitrate_mode(crate::BitrateMode::Cbr(8_000_000));
        assert_eq!(builder.video_width, Some(1920));
        assert_eq!(builder.video_height, Some(1080));
        assert_eq!(builder.video_fps, Some(30.0));
        assert_eq!(builder.video_codec, VideoCodec::H264);
        assert_eq!(
            builder.video_bitrate_mode,
            Some(crate::BitrateMode::Cbr(8_000_000))
        );
    }

    #[test]
    fn builder_audio_settings_should_be_stored() {
        let builder = VideoEncoder::create("output.mp4")
            .audio(48000, 2)
            .audio_codec(AudioCodec::Aac)
            .audio_bitrate(192_000);
        assert_eq!(builder.audio_sample_rate, Some(48000));
        assert_eq!(builder.audio_channels, Some(2));
        assert_eq!(builder.audio_codec, AudioCodec::Aac);
        assert_eq!(builder.audio_bitrate, Some(192_000));
    }

    #[test]
    fn builder_preset_should_be_stored() {
        let builder = VideoEncoder::create("output.mp4")
            .video(1920, 1080, 30.0)
            .preset(Preset::Fast);
        assert_eq!(builder.preset, Preset::Fast);
    }

    #[test]
    fn builder_hardware_encoder_should_be_stored() {
        let builder = VideoEncoder::create("output.mp4")
            .video(1920, 1080, 30.0)
            .hardware_encoder(HardwareEncoder::Nvenc);
        assert_eq!(builder.hardware_encoder, HardwareEncoder::Nvenc);
    }

    #[test]
    fn builder_container_should_be_stored() {
        let builder = VideoEncoder::create("output.mp4")
            .video(1920, 1080, 30.0)
            .container(Container::Mp4);
        assert_eq!(builder.container, Some(Container::Mp4));
    }

    #[test]
    fn build_without_streams_should_return_error() {
        let result = VideoEncoder::create("output.mp4").build();
        assert!(result.is_err());
    }

    #[test]
    fn build_with_odd_width_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(1921, 1080, 30.0)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn build_with_odd_height_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(1920, 1081, 30.0)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn build_with_invalid_fps_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(1920, 1080, -1.0)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn two_pass_flag_should_be_stored_in_builder() {
        let builder = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .two_pass();
        assert!(builder.two_pass);
    }

    #[test]
    fn two_pass_with_audio_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .audio(48000, 2)
            .two_pass()
            .build();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(
                matches!(e, crate::EncodeError::InvalidConfig { .. }),
                "expected InvalidConfig, got {e:?}"
            );
        }
    }

    #[test]
    fn two_pass_without_video_should_return_error() {
        let result = VideoEncoder::create("output.mp4").two_pass().build();
        assert!(result.is_err());
    }

    #[test]
    fn build_with_crf_above_51_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(1920, 1080, 30.0)
            .bitrate_mode(crate::BitrateMode::Crf(100))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn bitrate_mode_vbr_with_max_less_than_target_should_return_error() {
        let output_path = "test_vbr.mp4";
        let result = VideoEncoder::create(output_path)
            .video(640, 480, 30.0)
            .bitrate_mode(crate::BitrateMode::Vbr {
                target: 4_000_000,
                max: 2_000_000,
            })
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn is_lgpl_compliant_should_be_true_for_hardware_encoders() {
        for codec_name in &[
            "h264_nvenc",
            "h264_qsv",
            "h264_amf",
            "h264_videotoolbox",
            "hevc_vaapi",
        ] {
            let encoder = create_mock_encoder(codec_name, "");
            assert!(
                encoder.is_lgpl_compliant(),
                "expected LGPL-compliant for {codec_name}"
            );
        }
    }

    #[test]
    fn is_lgpl_compliant_should_be_false_for_gpl_encoders() {
        for codec_name in &["libx264", "libx265"] {
            let encoder = create_mock_encoder(codec_name, "");
            assert!(
                !encoder.is_lgpl_compliant(),
                "expected non-LGPL for {codec_name}"
            );
        }
    }

    #[test]
    fn hardware_encoder_detection_should_match_codec_name() {
        let cases: &[(&str, HardwareEncoder, bool)] = &[
            ("h264_nvenc", HardwareEncoder::Nvenc, true),
            ("h264_qsv", HardwareEncoder::Qsv, true),
            ("h264_amf", HardwareEncoder::Amf, true),
            ("h264_videotoolbox", HardwareEncoder::VideoToolbox, true),
            ("h264_vaapi", HardwareEncoder::Vaapi, true),
            ("libx264", HardwareEncoder::None, false),
            ("libvpx-vp9", HardwareEncoder::None, false),
        ];
        for (codec_name, expected_hw, expected_is_hw) in cases {
            let encoder = create_mock_encoder(codec_name, "");
            assert_eq!(
                encoder.hardware_encoder(),
                *expected_hw,
                "hw for {codec_name}"
            );
            assert_eq!(
                encoder.is_hardware_encoding(),
                *expected_is_hw,
                "is_hw for {codec_name}"
            );
        }
    }
}
