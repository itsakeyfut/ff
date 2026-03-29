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
    AudioCodec, EncodeError, EncodeProgressCallback, HardwareEncoder, OutputContainer, Preset,
    VideoCodec,
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
    pub(crate) container: Option<OutputContainer>,
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
    pub(crate) video_codec_explicit: bool,
    pub(crate) audio_codec_explicit: bool,
    pub(crate) pixel_format: Option<ff_format::PixelFormat>,
    pub(crate) hdr10_metadata: Option<ff_format::Hdr10Metadata>,
    pub(crate) color_space: Option<ff_format::ColorSpace>,
    pub(crate) color_transfer: Option<ff_format::ColorTransfer>,
    pub(crate) color_primaries: Option<ff_format::ColorPrimaries>,
    /// Binary attachments: (raw data, MIME type, filename).
    pub(crate) attachments: Vec<(Vec<u8>, String, String)>,
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
            .field("video_codec_explicit", &self.video_codec_explicit)
            .field("audio_codec_explicit", &self.audio_codec_explicit)
            .field("pixel_format", &self.pixel_format)
            .field("hdr10_metadata", &self.hdr10_metadata)
            .field("color_space", &self.color_space)
            .field("color_transfer", &self.color_transfer)
            .field("color_primaries", &self.color_primaries)
            .field("attachments_count", &self.attachments.len())
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
            video_codec_explicit: false,
            audio_codec_explicit: false,
            pixel_format: None,
            hdr10_metadata: None,
            color_space: None,
            color_transfer: None,
            color_primaries: None,
            attachments: Vec::new(),
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
        self.video_codec_explicit = true;
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
        self.audio_codec_explicit = true;
        self
    }

    /// Set audio bitrate in bits per second.
    #[must_use]
    pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
        self.audio_bitrate = Some(bitrate);
        self
    }

    // === OutputContainer settings ===

    /// Set container format explicitly (usually auto-detected from file extension).
    #[must_use]
    pub fn container(mut self, container: OutputContainer) -> Self {
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

    // === Pixel format ===

    /// Override the pixel format for video encoding.
    ///
    /// When omitted the encoder uses `yuv420p` by default, except that
    /// H.265 `Main10` automatically selects `yuv420p10le`.
    #[must_use]
    pub fn pixel_format(mut self, fmt: ff_format::PixelFormat) -> Self {
        self.pixel_format = Some(fmt);
        self
    }

    // === HDR metadata ===

    /// Embed HDR10 static metadata in the output.
    ///
    /// Sets `color_primaries = BT.2020`, `color_trc = SMPTE ST 2084 (PQ)`,
    /// and `colorspace = BT.2020 NCL` on the codec context, then attaches
    /// `AV_PKT_DATA_CONTENT_LIGHT_LEVEL` and
    /// `AV_PKT_DATA_MASTERING_DISPLAY_METADATA` packet side data to every
    /// keyframe.
    ///
    /// Pair with [`codec_options`](Self::codec_options) using
    /// `H265Options { profile: H265Profile::Main10, .. }`
    /// and [`pixel_format(PixelFormat::Yuv420p10le)`](Self::pixel_format) for a
    /// complete HDR10 pipeline.
    #[must_use]
    pub fn hdr10_metadata(mut self, meta: ff_format::Hdr10Metadata) -> Self {
        self.hdr10_metadata = Some(meta);
        self
    }

    // === Color tagging ===

    /// Override the color space (matrix coefficients) written to the codec context.
    ///
    /// When omitted the encoder uses the FFmpeg default. HDR10 metadata, if set
    /// via [`hdr10_metadata()`](Self::hdr10_metadata), automatically selects
    /// BT.2020 NCL — this setter takes priority over that automatic choice.
    #[must_use]
    pub fn color_space(mut self, cs: ff_format::ColorSpace) -> Self {
        self.color_space = Some(cs);
        self
    }

    /// Override the color transfer characteristic (gamma curve) written to the codec context.
    ///
    /// When omitted the encoder uses the FFmpeg default. HDR10 metadata
    /// automatically selects PQ (SMPTE ST 2084) — this setter takes priority.
    /// Use [`ColorTransfer::Hlg`](ff_format::ColorTransfer::Hlg) for HLG broadcast HDR.
    #[must_use]
    pub fn color_transfer(mut self, trc: ff_format::ColorTransfer) -> Self {
        self.color_transfer = Some(trc);
        self
    }

    /// Override the color primaries written to the codec context.
    ///
    /// When omitted the encoder uses the FFmpeg default. HDR10 metadata
    /// automatically selects BT.2020 — this setter takes priority.
    #[must_use]
    pub fn color_primaries(mut self, cp: ff_format::ColorPrimaries) -> Self {
        self.color_primaries = Some(cp);
        self
    }

    // === Attachments ===

    /// Embed a binary attachment in the output container.
    ///
    /// Attachments are supported in MKV/WebM containers and are used for
    /// fonts (required by ASS/SSA subtitle rendering), cover art, or other
    /// binary files that consumers of the file may need.
    ///
    /// - `data` — raw bytes of the attachment
    /// - `mime_type` — MIME type string (e.g. `"application/x-truetype-font"`,
    ///   `"image/jpeg"`)
    /// - `filename` — the name reported inside the container (e.g. `"Arial.ttf"`)
    ///
    /// Multiple calls accumulate entries; each attachment becomes its own stream
    /// with `AVMEDIA_TYPE_ATTACHMENT` codec parameters.
    #[must_use]
    pub fn add_attachment(mut self, data: Vec<u8>, mime_type: &str, filename: &str) -> Self {
        self.attachments
            .push((data, mime_type.to_string(), filename.to_string()));
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
        let this = self.apply_container_defaults();
        this.validate()?;
        VideoEncoder::from_builder(this)
    }

    /// Apply container-specific codec defaults before validation.
    ///
    /// For WebM paths/containers, default to VP9 + Opus when the caller has
    /// not explicitly chosen a codec.
    fn apply_container_defaults(mut self) -> Self {
        let is_webm = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("webm"))
            || self
                .container
                .as_ref()
                .is_some_and(|c| *c == OutputContainer::WebM);

        if is_webm {
            if !self.video_codec_explicit {
                self.video_codec = VideoCodec::Vp9;
            }
            if !self.audio_codec_explicit {
                self.audio_codec = AudioCodec::Opus;
            }
        }

        let is_avi = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("avi"))
            || self
                .container
                .as_ref()
                .is_some_and(|c| *c == OutputContainer::Avi);

        if is_avi {
            if !self.video_codec_explicit {
                self.video_codec = VideoCodec::H264;
            }
            if !self.audio_codec_explicit {
                self.audio_codec = AudioCodec::Mp3;
            }
        }

        let is_mov = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mov"))
            || self
                .container
                .as_ref()
                .is_some_and(|c| *c == OutputContainer::Mov);

        if is_mov {
            if !self.video_codec_explicit {
                self.video_codec = VideoCodec::H264;
            }
            if !self.audio_codec_explicit {
                self.audio_codec = AudioCodec::Aac;
            }
        }

        // Image-sequence paths contain '%' (e.g. "frames/frame%04d.png").
        // Auto-select codec from the extension that follows the pattern.
        let is_image_sequence = self.path.to_str().is_some_and(|s| s.contains('%'));
        if is_image_sequence && !self.video_codec_explicit {
            let ext = self
                .path
                .to_str()
                .and_then(|s| s.rfind('.').map(|i| &s[i + 1..]))
                .unwrap_or("");
            if ext.eq_ignore_ascii_case("png") {
                self.video_codec = VideoCodec::Png;
            } else if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
                self.video_codec = VideoCodec::Mjpeg;
            }
        }

        self
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

        // Image-sequence paths (containing '%') do not support audio streams.
        let is_image_sequence = self.path.to_str().is_some_and(|s| s.contains('%'));
        if is_image_sequence && has_audio {
            return Err(EncodeError::InvalidConfig {
                reason: "Image sequence output does not support audio streams".to_string(),
            });
        }

        // PNG supports odd dimensions; all other codecs require even width/height.
        let requires_even_dims = !matches!(self.video_codec, VideoCodec::Png);

        if has_video {
            // Dimension range check (2–32768 inclusive).
            let w = self.video_width.unwrap_or(0);
            let h = self.video_height.unwrap_or(0);
            if (self.video_width.is_some() || self.video_height.is_some())
                && (!(2..=32_768).contains(&w) || !(2..=32_768).contains(&h))
            {
                log::warn!(
                    "video dimensions out of range width={w} height={h} \
                     (valid range 2–32768 per axis)"
                );
                return Err(EncodeError::InvalidDimensions {
                    width: w,
                    height: h,
                });
            }

            if let Some(width) = self.video_width
                && (requires_even_dims && width % 2 != 0)
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video width must be even, got {width}"),
                });
            }
            if let Some(height) = self.video_height
                && (requires_even_dims && height % 2 != 0)
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video height must be even, got {height}"),
                });
            }
            if let Some(fps) = self.video_fps
                && fps <= 0.0
            {
                return Err(EncodeError::InvalidConfig {
                    reason: format!("Video FPS must be positive, got {fps}"),
                });
            }
            if let Some(fps) = self.video_fps
                && fps > 1000.0
            {
                log::warn!("video fps exceeds maximum fps={fps} (maximum 1000)");
                return Err(EncodeError::InvalidConfig {
                    reason: format!("fps {fps} exceeds maximum 1000"),
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

            // Bitrate ceiling: 800 Mbps (800_000_000 bps).
            let effective_bitrate: Option<u64> = match self.video_bitrate_mode {
                Some(crate::BitrateMode::Cbr(bps)) => Some(bps),
                Some(crate::BitrateMode::Vbr { max, .. }) => Some(max),
                _ => None,
            };
            if let Some(bps) = effective_bitrate
                && bps > 800_000_000
            {
                log::warn!("video bitrate exceeds maximum bitrate={bps} maximum=800000000");
                return Err(EncodeError::InvalidBitrate { bitrate: bps });
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

        // WebM container codec enforcement.
        let is_webm = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("webm"))
            || self
                .container
                .as_ref()
                .is_some_and(|c| *c == OutputContainer::WebM);

        if is_webm {
            let webm_video_ok = matches!(
                self.video_codec,
                VideoCodec::Vp9 | VideoCodec::Av1 | VideoCodec::Av1Svt
            );
            if !webm_video_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "webm".to_string(),
                    codec: self.video_codec.name().to_string(),
                    hint: "WebM supports VP9, AV1 (video) and Vorbis, Opus (audio)".to_string(),
                });
            }

            let webm_audio_ok = matches!(self.audio_codec, AudioCodec::Opus | AudioCodec::Vorbis);
            if !webm_audio_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "webm".to_string(),
                    codec: self.audio_codec.name().to_string(),
                    hint: "WebM supports VP9, AV1 (video) and Vorbis, Opus (audio)".to_string(),
                });
            }
        }

        // AVI container codec enforcement.
        let is_avi = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("avi"))
            || self
                .container
                .as_ref()
                .is_some_and(|c| *c == OutputContainer::Avi);

        if is_avi {
            let avi_video_ok = matches!(self.video_codec, VideoCodec::H264 | VideoCodec::Mpeg4);
            if !avi_video_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "avi".to_string(),
                    codec: self.video_codec.name().to_string(),
                    hint: "AVI supports H264 and MPEG-4 (video); MP3, AAC, and PCM 16-bit (audio)"
                        .to_string(),
                });
            }

            let avi_audio_ok = matches!(
                self.audio_codec,
                AudioCodec::Mp3 | AudioCodec::Aac | AudioCodec::Pcm | AudioCodec::Pcm16
            );
            if !avi_audio_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "avi".to_string(),
                    codec: self.audio_codec.name().to_string(),
                    hint: "AVI supports H264 and MPEG-4 (video); MP3, AAC, and PCM 16-bit (audio)"
                        .to_string(),
                });
            }
        }

        // MOV container codec enforcement.
        let is_mov = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mov"))
            || self
                .container
                .as_ref()
                .is_some_and(|c| *c == OutputContainer::Mov);

        if is_mov {
            let mov_video_ok = matches!(
                self.video_codec,
                VideoCodec::H264 | VideoCodec::H265 | VideoCodec::ProRes
            );
            if !mov_video_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "mov".to_string(),
                    codec: self.video_codec.name().to_string(),
                    hint: "MOV supports H264, H265, and ProRes (video); AAC and PCM (audio)"
                        .to_string(),
                });
            }

            let mov_audio_ok = matches!(
                self.audio_codec,
                AudioCodec::Aac | AudioCodec::Pcm | AudioCodec::Pcm16 | AudioCodec::Pcm24
            );
            if !mov_audio_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "mov".to_string(),
                    codec: self.audio_codec.name().to_string(),
                    hint: "MOV supports H264, H265, and ProRes (video); AAC and PCM (audio)"
                        .to_string(),
                });
            }
        }

        // fMP4 container codec enforcement.
        let is_fmp4 = self
            .container
            .as_ref()
            .is_some_and(|c| *c == OutputContainer::FMp4);

        if is_fmp4 {
            let fmp4_video_ok = !matches!(
                self.video_codec,
                VideoCodec::Mpeg2 | VideoCodec::Mpeg4 | VideoCodec::Mjpeg
            );
            if !fmp4_video_ok {
                return Err(EncodeError::UnsupportedContainerCodecCombination {
                    container: "fMP4".to_string(),
                    codec: self.video_codec.name().to_string(),
                    hint: "fMP4 supports H.264, H.265, VP9, AV1".to_string(),
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
            pixel_format: builder.pixel_format,
            hdr10_metadata: builder.hdr10_metadata,
            color_space: builder.color_space,
            color_transfer: builder.color_transfer,
            color_primaries: builder.color_primaries,
            attachments: builder.attachments,
            container: builder.container,
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
        inner.push_video_frame(frame)?;
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
        inner.push_audio_frame(frame)?;
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
            inner.finish()?;
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
                hdr10_metadata: None,
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
                pixel_format: None,
                hdr10_metadata: None,
                color_space: None,
                color_transfer: None,
                color_primaries: None,
                attachments: Vec::new(),
                container: None,
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
            .container(OutputContainer::Mp4);
        assert_eq!(builder.container, Some(OutputContainer::Mp4));
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

    #[test]
    fn add_attachment_should_accumulate_entries() {
        let builder = VideoEncoder::create("output.mkv")
            .video(320, 240, 30.0)
            .add_attachment(vec![1, 2, 3], "application/x-truetype-font", "font.ttf")
            .add_attachment(vec![4, 5, 6], "image/jpeg", "cover.jpg");
        assert_eq!(builder.attachments.len(), 2);
        assert_eq!(builder.attachments[0].0, vec![1u8, 2, 3]);
        assert_eq!(builder.attachments[0].1, "application/x-truetype-font");
        assert_eq!(builder.attachments[0].2, "font.ttf");
        assert_eq!(builder.attachments[1].1, "image/jpeg");
        assert_eq!(builder.attachments[1].2, "cover.jpg");
    }

    #[test]
    fn add_attachment_with_no_attachments_should_start_empty() {
        let builder = VideoEncoder::create("output.mkv").video(320, 240, 30.0);
        assert!(builder.attachments.is_empty());
    }

    #[test]
    fn webm_extension_with_h264_video_codec_should_return_error() {
        let result = VideoEncoder::create("output.webm")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn webm_extension_with_h265_video_codec_should_return_error() {
        let result = VideoEncoder::create("output.webm")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H265)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn webm_extension_with_incompatible_audio_codec_should_return_error() {
        let result = VideoEncoder::create("output.webm")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Aac)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn webm_extension_without_explicit_codec_should_default_to_vp9_opus() {
        let builder = VideoEncoder::create("output.webm").video(640, 480, 30.0);
        let normalized = builder.apply_container_defaults();
        assert_eq!(normalized.video_codec, VideoCodec::Vp9);
        assert_eq!(normalized.audio_codec, AudioCodec::Opus);
    }

    #[test]
    fn webm_extension_with_explicit_vp9_should_preserve_codec() {
        let builder = VideoEncoder::create("output.webm")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9);
        assert!(builder.video_codec_explicit);
        let normalized = builder.apply_container_defaults();
        assert_eq!(normalized.video_codec, VideoCodec::Vp9);
    }

    #[test]
    fn webm_container_enum_with_incompatible_codec_should_return_error() {
        let result = VideoEncoder::create("output.mkv")
            .video(640, 480, 30.0)
            .container(OutputContainer::WebM)
            .video_codec(VideoCodec::H264)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn non_webm_extension_should_not_enforce_webm_codecs() {
        // H264 + AAC on .mp4 should not trigger WebM validation
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .build();
        // Should not fail with UnsupportedContainerCodecCombination
        assert!(!matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn avi_extension_without_explicit_codec_should_default_to_h264_mp3() {
        let builder = VideoEncoder::create("output.avi").video(640, 480, 30.0);
        let normalized = builder.apply_container_defaults();
        assert_eq!(normalized.video_codec, VideoCodec::H264);
        assert_eq!(normalized.audio_codec, AudioCodec::Mp3);
    }

    #[test]
    fn mov_extension_without_explicit_codec_should_default_to_h264_aac() {
        let builder = VideoEncoder::create("output.mov").video(640, 480, 30.0);
        let normalized = builder.apply_container_defaults();
        assert_eq!(normalized.video_codec, VideoCodec::H264);
        assert_eq!(normalized.audio_codec, AudioCodec::Aac);
    }

    #[test]
    fn avi_with_incompatible_video_codec_should_return_error() {
        let result = VideoEncoder::create("output.avi")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn avi_with_incompatible_audio_codec_should_return_error() {
        let result = VideoEncoder::create("output.avi")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Opus)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn mov_with_incompatible_video_codec_should_return_error() {
        let result = VideoEncoder::create("output.mov")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn mov_with_incompatible_audio_codec_should_return_error() {
        let result = VideoEncoder::create("output.mov")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Opus)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn avi_container_enum_with_incompatible_codec_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .container(OutputContainer::Avi)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn mov_container_enum_with_incompatible_codec_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .container(OutputContainer::Mov)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn avi_with_pcm_audio_should_pass_validation() {
        // AudioCodec::Pcm (backward-compat alias for 16-bit PCM) must be accepted in AVI.
        let result = VideoEncoder::create("output.avi")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Pcm)
            .build();
        assert!(!matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn mov_with_pcm24_audio_should_pass_validation() {
        let result = VideoEncoder::create("output.mov")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Pcm24)
            .build();
        assert!(!matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn non_avi_mov_extension_should_not_enforce_avi_mov_codecs() {
        // Vp9 on .webm should not trigger AVI/MOV validation
        let result = VideoEncoder::create("output.webm")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(!matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination {
                ref container, ..
            }) if container == "avi" || container == "mov"
        ));
    }

    #[test]
    fn fmp4_container_with_h264_should_pass_validation() {
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::H264)
            .container(OutputContainer::FMp4)
            .build();
        assert!(!matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination { .. })
        ));
    }

    #[test]
    fn fmp4_container_with_mpeg4_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Mpeg4)
            .container(OutputContainer::FMp4)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination {
                ref container, ..
            }) if container == "fMP4"
        ));
    }

    #[test]
    fn fmp4_container_with_mjpeg_should_return_error() {
        let result = VideoEncoder::create("output.mp4")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Mjpeg)
            .container(OutputContainer::FMp4)
            .build();
        assert!(matches!(
            result,
            Err(crate::EncodeError::UnsupportedContainerCodecCombination {
                ref container, ..
            }) if container == "fMP4"
        ));
    }
}
