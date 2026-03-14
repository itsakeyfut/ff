//! Internal video encoder implementation.
//!
//! This module contains the internal implementation details of the video encoder,
//! including FFmpeg context management and encoding operations.

// Rust 2024: Allow unsafe operations in unsafe functions for FFmpeg C API
#![allow(unsafe_op_in_unsafe_fn)]
// FFmpeg C API frequently requires raw pointer casting
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::cast_possible_wrap)]

use crate::{AudioCodec, EncodeError, VideoCodec};
use ff_format::{AudioFrame, VideoFrame};
use ff_sys::{
    AVChannelLayout, AVCodecContext, AVCodecID, AVCodecID_AV_CODEC_ID_AAC,
    AVCodecID_AV_CODEC_ID_AV1, AVCodecID_AV_CODEC_ID_DNXHD, AVCodecID_AV_CODEC_ID_FLAC,
    AVCodecID_AV_CODEC_ID_H264, AVCodecID_AV_CODEC_ID_HEVC, AVCodecID_AV_CODEC_ID_MP3,
    AVCodecID_AV_CODEC_ID_MPEG4, AVCodecID_AV_CODEC_ID_OPUS, AVCodecID_AV_CODEC_ID_PCM_S16LE,
    AVCodecID_AV_CODEC_ID_PRORES, AVCodecID_AV_CODEC_ID_VORBIS, AVCodecID_AV_CODEC_ID_VP9,
    AVFormatContext, AVFrame, AVPixelFormat, AVPixelFormat_AV_PIX_FMT_YUV420P, SwrContext,
    SwsContext, av_frame_alloc, av_frame_free, av_interleaved_write_frame, av_packet_alloc,
    av_packet_free, av_packet_unref, av_write_trailer, avcodec, avformat_alloc_output_context2,
    avformat_free_context, avformat_new_stream, avformat_write_header, swresample, swscale,
};
use std::ffi::CString;
use std::ptr;

/// Maximum number of planes in AVFrame data/linesize arrays.
///
/// This corresponds to FFmpeg's `AV_NUM_DATA_POINTERS` (typically 8).
/// Most pixel formats use 1-3 planes (e.g., RGB uses 1, YUV420P uses 3),
/// but this allows for future extensibility and compatibility with
/// exotic formats that may require more planes.
const MAX_PLANES: usize = 8;

/// FFmpeg pass-1 encoding flag: collect two-pass statistics, discard encoded output.
const AV_CODEC_FLAG_PASS1: i32 = 512; // 1 << 9

/// FFmpeg pass-2 encoding flag: use two-pass statistics from pass 1.
const AV_CODEC_FLAG_PASS2: i32 = 1024; // 1 << 10

/// Buffered raw frame data for two-pass re-encoding.
///
/// Stores the already-converted YUV420P plane data from pass 1 so that
/// the same frames can be re-encoded in pass 2 without re-reading from
/// the caller.
pub(super) struct TwoPassFrame {
    /// YUV420P plane data (Y plane at index 0, U at 1, V at 2).
    planes: Vec<Vec<u8>>,
    /// Linesize (stride) for each plane.
    strides: Vec<usize>,
    /// Frame width in pixels.
    width: u32,
    /// Frame height in pixels.
    height: u32,
    /// Presentation timestamp used when encoding this frame.
    pts: i64,
}

/// Internal encoder state with FFmpeg contexts.
pub(super) struct VideoEncoderInner {
    /// Output format context
    pub(super) format_ctx: *mut AVFormatContext,

    /// Video codec context
    pub(super) video_codec_ctx: Option<*mut AVCodecContext>,

    /// Audio codec context (for future use)
    pub(super) audio_codec_ctx: Option<*mut AVCodecContext>,

    /// Video stream index
    pub(super) video_stream_index: i32,

    /// Audio stream index
    pub(super) audio_stream_index: i32,

    /// Scaling context for pixel format conversion
    pub(super) sws_ctx: Option<*mut SwsContext>,

    /// Resampling context for audio format conversion
    pub(super) swr_ctx: Option<*mut SwrContext>,

    /// Frame counter
    pub(super) frame_count: u64,

    /// Audio sample counter
    pub(super) audio_sample_count: u64,

    /// Bytes written
    pub(super) bytes_written: u64,

    /// Actual video codec name being used
    pub(super) actual_video_codec: String,

    /// Actual audio codec name being used
    pub(super) actual_audio_codec: String,

    /// Last source frame width (for SwsContext reuse optimization)
    pub(super) last_src_width: Option<u32>,

    /// Last source frame height (for SwsContext reuse optimization)
    pub(super) last_src_height: Option<u32>,

    /// Last source frame format (for SwsContext reuse optimization)
    pub(super) last_src_format: Option<AVPixelFormat>,

    /// Whether two-pass encoding is active.
    pub(super) two_pass: bool,

    /// Pass-1 codec context (two-pass mode only; None in single-pass and after pass 1 completes).
    pub(super) pass1_codec_ctx: Option<*mut AVCodecContext>,

    /// Buffered YUV420P frame data for pass-2 re-encoding (two-pass mode only).
    pub(super) buffered_frames: Vec<TwoPassFrame>,

    /// Stored configuration for reconstructing the pass-2 codec context.
    pub(super) two_pass_config: Option<VideoEncoderConfig>,

    /// Owned `stats_in` C string that must outlive the pass-2 codec context.
    ///
    /// Nulled out in `cleanup()` before `avcodec_free_context` to prevent FFmpeg
    /// from calling `av_free` on a Rust-allocated pointer.
    pub(super) stats_in_cstr: Option<std::ffi::CString>,
}

/// VideoEncoder configuration (stored from builder).
#[derive(Debug, Clone)]
pub(super) struct VideoEncoderConfig {
    pub(super) path: std::path::PathBuf,
    pub(super) video_width: Option<u32>,
    pub(super) video_height: Option<u32>,
    pub(super) video_fps: Option<f64>,
    pub(super) video_codec: VideoCodec,
    pub(super) video_bitrate_mode: Option<crate::BitrateMode>,
    pub(super) preset: String,
    pub(super) hardware_encoder: crate::HardwareEncoder,
    pub(super) audio_sample_rate: Option<u32>,
    pub(super) audio_channels: Option<u32>,
    pub(super) audio_codec: AudioCodec,
    pub(super) audio_bitrate: Option<u64>,
    pub(super) _progress_callback: bool,
    pub(super) two_pass: bool,
}
impl VideoEncoderInner {
    /// Create a new encoder with the given configuration.
    pub(super) fn new(config: &VideoEncoderConfig) -> Result<Self, EncodeError> {
        unsafe {
            ff_sys::ensure_initialized();

            // Allocate output format context
            let c_path = CString::new(config.path.to_str().ok_or_else(|| {
                EncodeError::CannotCreateFile {
                    path: config.path.clone(),
                }
            })?)
            .map_err(|_| EncodeError::CannotCreateFile {
                path: config.path.clone(),
            })?;

            let mut format_ctx: *mut AVFormatContext = ptr::null_mut();
            let ret = avformat_alloc_output_context2(
                &mut format_ctx,
                ptr::null_mut(),
                ptr::null(),
                c_path.as_ptr(),
            );

            if ret < 0 || format_ctx.is_null() {
                return Err(EncodeError::Ffmpeg(format!(
                    "Cannot create output context: {}",
                    ff_sys::av_error_string(ret)
                )));
            }

            let mut encoder = Self {
                format_ctx,
                video_codec_ctx: None,
                audio_codec_ctx: None,
                video_stream_index: -1,
                audio_stream_index: -1,
                sws_ctx: None,
                swr_ctx: None,
                frame_count: 0,
                audio_sample_count: 0,
                bytes_written: 0,
                actual_video_codec: String::new(),
                actual_audio_codec: String::new(),
                last_src_width: None,
                last_src_height: None,
                last_src_format: None,
                two_pass: config.two_pass,
                pass1_codec_ctx: None,
                buffered_frames: Vec::new(),
                two_pass_config: None,
                stats_in_cstr: None,
            };

            // Initialize video encoder if configured
            if let (Some(width), Some(height), Some(fps)) =
                (config.video_width, config.video_height, config.video_fps)
            {
                encoder.init_video_encoder(
                    width,
                    height,
                    fps,
                    config.video_codec,
                    config.video_bitrate_mode.as_ref(),
                    &config.preset,
                    config.hardware_encoder,
                    config.two_pass,
                )?;
            }

            // Store config for pass-2 reconstruction (two-pass mode only).
            if config.two_pass {
                encoder.two_pass_config = Some(config.clone());
            }

            // Initialize audio encoder if configured
            if let (Some(sample_rate), Some(channels)) =
                (config.audio_sample_rate, config.audio_channels)
            {
                encoder.init_audio_encoder(
                    sample_rate,
                    channels,
                    config.audio_codec,
                    config.audio_bitrate,
                )?;
            }

            // For two-pass encoding the output file is opened in run_pass2() after
            // pass-1 statistics have been collected.  Single-pass opens it now.
            if !config.two_pass {
                match ff_sys::avformat::open_output(
                    &config.path,
                    ff_sys::avformat::avio_flags::WRITE,
                ) {
                    Ok(pb) => (*format_ctx).pb = pb,
                    Err(_) => {
                        encoder.cleanup();
                        return Err(EncodeError::CannotCreateFile {
                            path: config.path.clone(),
                        });
                    }
                }

                let ret = avformat_write_header(format_ctx, ptr::null_mut());
                if ret < 0 {
                    encoder.cleanup();
                    return Err(EncodeError::Ffmpeg(format!(
                        "Cannot write header: {}",
                        ff_sys::av_error_string(ret)
                    )));
                }
            }

            Ok(encoder)
        }
    }

    /// Initialize video encoder.
    ///
    /// When `two_pass` is `true` the codec context is opened with
    /// `AV_CODEC_FLAG_PASS1` and stored in `pass1_codec_ctx`; in single-pass
    /// mode it is stored in `video_codec_ctx` as usual.
    unsafe fn init_video_encoder(
        &mut self,
        width: u32,
        height: u32,
        fps: f64,
        codec: VideoCodec,
        bitrate_mode: Option<&crate::BitrateMode>,
        preset: &str,
        hardware_encoder: crate::HardwareEncoder,
        two_pass: bool,
    ) -> Result<(), EncodeError> {
        use crate::BitrateMode;
        // Select encoder based on codec and availability
        let encoder_name = self.select_video_encoder(codec, hardware_encoder)?;
        self.actual_video_codec = encoder_name.clone();

        let c_encoder_name = CString::new(encoder_name.as_str())
            .map_err(|_| EncodeError::Ffmpeg("Invalid encoder name".to_string()))?;

        let codec_ptr =
            avcodec::find_encoder_by_name(c_encoder_name.as_ptr()).ok_or_else(|| {
                EncodeError::NoSuitableEncoder {
                    codec: format!("{:?}", codec),
                    tried: vec![encoder_name.clone()],
                }
            })?;

        // Allocate codec context
        let mut codec_ctx =
            avcodec::alloc_context3(codec_ptr).map_err(EncodeError::from_ffmpeg_error)?;

        // Configure codec context
        (*codec_ctx).codec_id = codec_to_id(codec);
        (*codec_ctx).width = width as i32;
        (*codec_ctx).height = height as i32;
        (*codec_ctx).time_base.num = 1;
        (*codec_ctx).time_base.den = (fps * 1000.0) as i32; // Use millisecond precision
        (*codec_ctx).framerate.num = fps as i32;
        (*codec_ctx).framerate.den = 1;
        (*codec_ctx).pix_fmt = AVPixelFormat_AV_PIX_FMT_YUV420P;

        // Set bitrate control mode
        match bitrate_mode {
            Some(BitrateMode::Cbr(bps)) => {
                (*codec_ctx).bit_rate = *bps as i64;
            }
            Some(BitrateMode::Vbr { target, max }) => {
                (*codec_ctx).bit_rate = *target as i64;
                (*codec_ctx).rc_max_rate = *max as i64;
                (*codec_ctx).rc_buffer_size = (*max * 2) as i32;
            }
            Some(BitrateMode::Crf(q)) => {
                let crf_str = CString::new(q.to_string())
                    .map_err(|_| EncodeError::Ffmpeg("Invalid CRF value".to_string()))?;
                // SAFETY: priv_data, option name, and value are all valid pointers
                let ret = ff_sys::av_opt_set(
                    (*codec_ctx).priv_data,
                    b"crf\0".as_ptr() as *const i8,
                    crf_str.as_ptr(),
                    0,
                );
                if ret < 0 {
                    log::warn!(
                        "crf option not supported by encoder, falling back to default bitrate \
                         encoder={encoder_name} crf={q}"
                    );
                    (*codec_ctx).bit_rate = 2_000_000;
                }
            }
            None => {
                // Default 2 Mbps
                (*codec_ctx).bit_rate = 2_000_000;
            }
        }

        // Set preset for x264/x265
        if encoder_name.contains("264") || encoder_name.contains("265") {
            let preset_cstr = CString::new(preset)
                .map_err(|_| EncodeError::Ffmpeg("Invalid preset value".to_string()))?;
            // SAFETY: priv_data, option name, and value are all valid pointers
            let ret = ff_sys::av_opt_set(
                (*codec_ctx).priv_data,
                b"preset\0".as_ptr() as *const i8,
                preset_cstr.as_ptr(),
                0,
            );
            if ret < 0 {
                log::warn!(
                    "preset option not supported by encoder, ignoring \
                     encoder={encoder_name} preset={preset}"
                );
            }
        }

        // For two-pass, set the pass-1 flag before opening the codec.
        if two_pass {
            // SAFETY: codec_ctx is a valid allocated (but not yet opened) context.
            (*codec_ctx).flags |= AV_CODEC_FLAG_PASS1;
        }

        // Open codec
        avcodec::open2(codec_ctx, codec_ptr, ptr::null_mut())
            .map_err(EncodeError::from_ffmpeg_error)?;
        log::info!("codec opened codec={encoder_name} width={width} height={height} fps={fps}");

        // Create stream
        let stream = avformat_new_stream(self.format_ctx, codec_ptr);
        if stream.is_null() {
            avcodec::free_context(&mut codec_ctx as *mut *mut _);
            return Err(EncodeError::Ffmpeg("Cannot create stream".to_string()));
        }

        (*stream).time_base = (*codec_ctx).time_base;

        // Copy codec parameters to stream
        if !(*stream).codecpar.is_null() {
            (*(*stream).codecpar).codec_id = (*codec_ctx).codec_id;
            (*(*stream).codecpar).codec_type = ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO;
            (*(*stream).codecpar).width = (*codec_ctx).width;
            (*(*stream).codecpar).height = (*codec_ctx).height;
            (*(*stream).codecpar).format = (*codec_ctx).pix_fmt;
        }

        self.video_stream_index = ((*self.format_ctx).nb_streams - 1) as i32;

        // In two-pass mode the pass-1 context is stored separately; the real
        // (pass-2) video_codec_ctx is initialised later in run_pass2().
        if two_pass {
            self.pass1_codec_ctx = Some(codec_ctx);
        } else {
            self.video_codec_ctx = Some(codec_ctx);
        }

        // Note: SwsContext initialization is deferred to convert_video_frame()
        // for better optimization (skip unnecessary conversions, reuse context)

        Ok(())
    }

    /// Select best available video encoder for the given codec.
    ///
    /// This method implements LGPL-compliant codec selection with automatic fallback:
    /// - For H.264: Hardware encoders → libx264 (GPL only) → VP9 fallback
    /// - For H.265: Hardware encoders → libx265 (GPL only) → AV1 fallback
    /// - Hardware encoders (NVENC, QSV, AMF, VideoToolbox) are LGPL-compatible
    /// - VP9 and AV1 are LGPL-compatible
    fn select_video_encoder(
        &self,
        codec: VideoCodec,
        hardware_encoder: crate::HardwareEncoder,
    ) -> Result<String, EncodeError> {
        let candidates: Vec<&str> = match codec {
            VideoCodec::H264 => self.select_h264_encoder_candidates(hardware_encoder),
            VideoCodec::H265 => self.select_h265_encoder_candidates(hardware_encoder),
            VideoCodec::Vp9 => vec!["libvpx-vp9"],
            VideoCodec::Av1 => vec!["libaom-av1", "libsvtav1", "av1"],
            VideoCodec::ProRes => vec!["prores_ks", "prores"],
            VideoCodec::DnxHd => vec!["dnxhd"],
            VideoCodec::Mpeg4 => vec!["mpeg4"],
        };

        // Try each candidate
        for &name in &candidates {
            unsafe {
                let c_name = CString::new(name)
                    .map_err(|_| EncodeError::Ffmpeg("Invalid encoder name".to_string()))?;
                if avcodec::find_encoder_by_name(c_name.as_ptr()).is_some() {
                    return Ok(name.to_string());
                }
            }
        }

        Err(EncodeError::NoSuitableEncoder {
            codec: format!("{:?}", codec),
            tried: candidates.iter().map(|s| (*s).to_string()).collect(),
        })
    }

    /// Select H.264 encoder candidates with LGPL compliance.
    ///
    /// Priority order:
    /// 1. Hardware encoders (LGPL-compatible)
    /// 2. libx264 (GPL only, requires `gpl` feature)
    /// 3. VP9 fallback (LGPL-compatible)
    fn select_h264_encoder_candidates(
        &self,
        hardware_encoder: crate::HardwareEncoder,
    ) -> Vec<&'static str> {
        let mut candidates = Vec::new();

        // Add hardware encoders based on preference
        #[cfg(feature = "hwaccel")]
        match hardware_encoder {
            crate::HardwareEncoder::Nvenc => {
                candidates.extend_from_slice(&["h264_nvenc", "h264_qsv", "h264_amf"]);
            }
            crate::HardwareEncoder::Qsv => {
                candidates.extend_from_slice(&["h264_qsv", "h264_nvenc", "h264_amf"]);
            }
            crate::HardwareEncoder::Amf => {
                candidates.extend_from_slice(&["h264_amf", "h264_nvenc", "h264_qsv"]);
            }
            crate::HardwareEncoder::VideoToolbox => {
                candidates.push("h264_videotoolbox");
            }
            crate::HardwareEncoder::Vaapi => {
                candidates.push("h264_vaapi");
            }
            crate::HardwareEncoder::Auto => {
                candidates.extend_from_slice(&[
                    "h264_nvenc",
                    "h264_qsv",
                    "h264_amf",
                    "h264_videotoolbox",
                    "h264_vaapi",
                ]);
            }
            crate::HardwareEncoder::None => {
                // Skip hardware encoders
            }
        }

        // Add GPL encoder if feature is enabled
        #[cfg(feature = "gpl")]
        {
            candidates.push("libx264");
        }

        // Add LGPL-compatible fallback (VP9)
        candidates.push("libvpx-vp9");

        candidates
    }

    /// Select H.265 encoder candidates with LGPL compliance.
    ///
    /// Priority order:
    /// 1. Hardware encoders (LGPL-compatible)
    /// 2. libx265 (GPL only, requires `gpl` feature)
    /// 3. AV1 fallback (LGPL-compatible)
    fn select_h265_encoder_candidates(
        &self,
        hardware_encoder: crate::HardwareEncoder,
    ) -> Vec<&'static str> {
        let mut candidates = Vec::new();

        // Add hardware encoders based on preference
        #[cfg(feature = "hwaccel")]
        match hardware_encoder {
            crate::HardwareEncoder::Nvenc => {
                candidates.extend_from_slice(&["hevc_nvenc", "hevc_qsv", "hevc_amf"]);
            }
            crate::HardwareEncoder::Qsv => {
                candidates.extend_from_slice(&["hevc_qsv", "hevc_nvenc", "hevc_amf"]);
            }
            crate::HardwareEncoder::Amf => {
                candidates.extend_from_slice(&["hevc_amf", "hevc_nvenc", "hevc_qsv"]);
            }
            crate::HardwareEncoder::VideoToolbox => {
                candidates.push("hevc_videotoolbox");
            }
            crate::HardwareEncoder::Vaapi => {
                candidates.push("hevc_vaapi");
            }
            crate::HardwareEncoder::Auto => {
                candidates.extend_from_slice(&[
                    "hevc_nvenc",
                    "hevc_qsv",
                    "hevc_amf",
                    "hevc_videotoolbox",
                    "hevc_vaapi",
                ]);
            }
            crate::HardwareEncoder::None => {
                // Skip hardware encoders
            }
        }

        // Add GPL encoder if feature is enabled
        #[cfg(feature = "gpl")]
        {
            candidates.push("libx265");
        }

        // Add LGPL-compatible fallback (AV1)
        candidates.extend_from_slice(&["libaom-av1", "libsvtav1"]);

        candidates
    }

    /// Initialize audio encoder.
    unsafe fn init_audio_encoder(
        &mut self,
        sample_rate: u32,
        channels: u32,
        codec: AudioCodec,
        bitrate: Option<u64>,
    ) -> Result<(), EncodeError> {
        // Select encoder based on codec and availability
        let encoder_name = self.select_audio_encoder(codec)?;
        self.actual_audio_codec = encoder_name.clone();

        let c_encoder_name = CString::new(encoder_name.as_str())
            .map_err(|_| EncodeError::Ffmpeg("Invalid encoder name".to_string()))?;

        let codec_ptr =
            avcodec::find_encoder_by_name(c_encoder_name.as_ptr()).ok_or_else(|| {
                EncodeError::NoSuitableEncoder {
                    codec: format!("{:?}", codec),
                    tried: vec![encoder_name.clone()],
                }
            })?;

        // Allocate codec context
        let mut codec_ctx =
            avcodec::alloc_context3(codec_ptr).map_err(EncodeError::from_ffmpeg_error)?;

        // Configure codec context
        (*codec_ctx).codec_id = audio_codec_to_id(codec);
        (*codec_ctx).sample_rate = sample_rate as i32;

        // Set channel layout using FFmpeg 7.x API
        swresample::channel_layout::set_default(&mut (*codec_ctx).ch_layout, channels as i32);

        // Set sample format (encoder's preferred format)
        // We'll use FLTP (planar float) as it's widely supported
        (*codec_ctx).sample_fmt = ff_sys::swresample::sample_format::FLTP;

        // Set bitrate
        if let Some(br) = bitrate {
            (*codec_ctx).bit_rate = br as i64;
        } else {
            // Default bitrate based on codec
            (*codec_ctx).bit_rate = match codec {
                AudioCodec::Aac => 192_000,
                AudioCodec::Opus => 128_000,
                AudioCodec::Mp3 => 192_000,
                AudioCodec::Flac => 0, // Lossless
                AudioCodec::Pcm => 0,  // Uncompressed
                AudioCodec::Vorbis => 192_000,
            };
        }

        // Set time base
        (*codec_ctx).time_base.num = 1;
        (*codec_ctx).time_base.den = sample_rate as i32;

        // Open codec
        avcodec::open2(codec_ctx, codec_ptr, ptr::null_mut())
            .map_err(EncodeError::from_ffmpeg_error)?;

        // Create stream
        let stream = avformat_new_stream(self.format_ctx, codec_ptr);
        if stream.is_null() {
            avcodec::free_context(&mut codec_ctx as *mut *mut _);
            return Err(EncodeError::Ffmpeg("Cannot create stream".to_string()));
        }

        (*stream).time_base = (*codec_ctx).time_base;

        // Copy codec parameters to stream
        if !(*stream).codecpar.is_null() {
            (*(*stream).codecpar).codec_id = (*codec_ctx).codec_id;
            (*(*stream).codecpar).codec_type = ff_sys::AVMediaType_AVMEDIA_TYPE_AUDIO;
            (*(*stream).codecpar).sample_rate = (*codec_ctx).sample_rate;
            (*(*stream).codecpar).format = (*codec_ctx).sample_fmt;
            // Copy channel layout
            swresample::channel_layout::copy(
                &mut (*(*stream).codecpar).ch_layout,
                &(*codec_ctx).ch_layout,
            )
            .map_err(EncodeError::from_ffmpeg_error)?;
        }

        self.audio_stream_index = ((*self.format_ctx).nb_streams - 1) as i32;
        self.audio_codec_ctx = Some(codec_ctx);

        Ok(())
    }

    /// Select best available audio encoder for the given codec.
    fn select_audio_encoder(&self, codec: AudioCodec) -> Result<String, EncodeError> {
        let candidates: Vec<&str> = match codec {
            AudioCodec::Aac => vec!["aac", "libfdk_aac"],
            AudioCodec::Opus => vec!["libopus"],
            AudioCodec::Mp3 => vec!["libmp3lame", "mp3"],
            AudioCodec::Flac => vec!["flac"],
            AudioCodec::Pcm => vec!["pcm_s16le"],
            AudioCodec::Vorbis => vec!["libvorbis", "vorbis"],
        };

        // Try each candidate
        for &name in &candidates {
            unsafe {
                let c_name = CString::new(name)
                    .map_err(|_| EncodeError::Ffmpeg("Invalid encoder name".to_string()))?;
                if avcodec::find_encoder_by_name(c_name.as_ptr()).is_some() {
                    return Ok(name.to_string());
                }
            }
        }

        Err(EncodeError::NoSuitableEncoder {
            codec: format!("{:?}", codec),
            tried: candidates.iter().map(|s| (*s).to_string()).collect(),
        })
    }

    /// Push a video frame for encoding.
    ///
    /// In two-pass mode the frame is converted to YUV420P via the pass-1 codec
    /// context, the converted data is buffered for pass-2 replay, and the frame
    /// is then sent through the pass-1 encoder (whose output is discarded).
    pub(super) unsafe fn push_video_frame(
        &mut self,
        frame: &VideoFrame,
    ) -> Result<(), EncodeError> {
        // ── Two-pass path ────────────────────────────────────────────────────
        if self.two_pass {
            let pass1_ctx = self
                .pass1_codec_ctx
                .ok_or_else(|| EncodeError::InvalidConfig {
                    reason: "Pass-1 codec context not initialized".to_string(),
                })?;

            // Convert the incoming frame to YUV420P (the pass-1 codec's format).
            let mut av_frame = av_frame_alloc();
            if av_frame.is_null() {
                return Err(EncodeError::Ffmpeg("Cannot allocate frame".to_string()));
            }

            let convert_result = self.convert_video_frame(frame, av_frame, pass1_ctx);
            if let Err(e) = convert_result {
                av_frame_free(&mut av_frame as *mut *mut _);
                return Err(e);
            }

            // Buffer the converted YUV420P data for pass-2 replay.
            let width = (*pass1_ctx).width as u32;
            let height = (*pass1_ctx).height as u32;
            let uv_height = (height as usize).div_ceil(2);

            let planes: Vec<Vec<u8>> = (0..3)
                .map(|i| {
                    if (*av_frame).data[i].is_null() {
                        return Vec::new();
                    }
                    let stride = (*av_frame).linesize[i] as usize;
                    let h = if i == 0 { height as usize } else { uv_height };
                    // SAFETY: data[i] points to a valid buffer of stride * h bytes
                    // allocated by av_frame_get_buffer inside convert_video_frame.
                    std::slice::from_raw_parts((*av_frame).data[i], stride * h).to_vec()
                })
                .collect();

            let strides: Vec<usize> = (0..3).map(|i| (*av_frame).linesize[i] as usize).collect();

            self.buffered_frames.push(TwoPassFrame {
                planes,
                strides,
                width,
                height,
                pts: self.frame_count as i64,
            });

            // Send to pass-1 encoder and discard the encoded output.
            (*av_frame).pts = self.frame_count as i64;
            let send_result = avcodec::send_frame(pass1_ctx, av_frame);
            if let Err(e) = send_result {
                av_frame_free(&mut av_frame as *mut *mut _);
                return Err(EncodeError::Ffmpeg(format!(
                    "Failed to send frame to pass-1 encoder: {}",
                    ff_sys::av_error_string(e)
                )));
            }

            let drain_result = self.drain_pass1_packets(pass1_ctx);
            av_frame_free(&mut av_frame as *mut *mut _);
            drain_result?;

            self.frame_count += 1;
            return Ok(());
        }

        // ── Single-pass path ─────────────────────────────────────────────────
        let codec_ctx = self
            .video_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Video codec not initialized".to_string(),
            })?;

        // Allocate AVFrame
        let mut av_frame = av_frame_alloc();
        if av_frame.is_null() {
            return Err(EncodeError::Ffmpeg("Cannot allocate frame".to_string()));
        }

        // Convert VideoFrame to AVFrame
        let convert_result = self.convert_video_frame(frame, av_frame, codec_ctx);
        if let Err(e) = convert_result {
            av_frame_free(&mut av_frame as *mut *mut _);
            return Err(e);
        }

        // Set frame properties
        (*av_frame).pts = self.frame_count as i64;

        // Send frame to encoder
        let send_result = avcodec::send_frame(codec_ctx, av_frame);
        if let Err(e) = send_result {
            av_frame_free(&mut av_frame as *mut *mut _);
            return Err(EncodeError::Ffmpeg(format!(
                "Failed to send frame: {}",
                ff_sys::av_error_string(e)
            )));
        }

        // Receive packets
        let receive_result = self.receive_packets();

        // Always cleanup the frame
        av_frame_free(&mut av_frame as *mut *mut _);

        // Check if receiving packets failed
        receive_result?;

        self.frame_count += 1;

        Ok(())
    }

    /// Drain and discard all pending packets from a codec context.
    ///
    /// Used during pass-1 of two-pass encoding to prevent the packet queue
    /// from filling up without writing any data to the output file.
    ///
    /// # Safety
    ///
    /// `codec_ctx` must be a valid, open `AVCodecContext`.
    unsafe fn drain_pass1_packets(
        &self,
        codec_ctx: *mut AVCodecContext,
    ) -> Result<(), EncodeError> {
        let mut packet = av_packet_alloc();
        if packet.is_null() {
            return Err(EncodeError::Ffmpeg("Cannot allocate packet".to_string()));
        }

        loop {
            match avcodec::receive_packet(codec_ctx, packet) {
                Ok(()) => {
                    // Discard — do not write to the format context.
                    av_packet_unref(packet);
                }
                Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                    break;
                }
                Err(e) => {
                    av_packet_free(&mut packet as *mut *mut _);
                    return Err(EncodeError::Ffmpeg(format!(
                        "Error receiving packet from pass-1 encoder: {}",
                        ff_sys::av_error_string(e)
                    )));
                }
            }
        }

        av_packet_free(&mut packet as *mut *mut _);
        Ok(())
    }

    /// Convert VideoFrame to AVFrame with pixel format conversion if needed.
    ///
    /// This method implements several optimizations in priority order:
    /// 1. **Fast path**: Skips conversion entirely if format/dimensions match
    /// 2. **Context reuse**: Reuses SwsContext when source properties unchanged
    /// 3. **Lazy init**: Reinitializes SwsContext only when needed
    /// 4. **Fast algorithm**: Uses BILINEAR scaling for speed/quality balance
    ///
    /// The caller supplies `codec_ctx` explicitly so this function can be used
    /// with both the regular `video_codec_ctx` and the pass-1 `pass1_codec_ctx`.
    ///
    /// # Performance Characteristics
    ///
    /// - Same format/size: ~0.1ms (direct memory copy only)
    /// - Different format/size with context reuse: ~2-5ms
    /// - Different format/size with context reinit: ~5-10ms
    ///
    /// # Safety
    ///
    /// This function is unsafe because it directly manipulates FFmpeg AVFrame pointers.
    /// The caller must ensure that `dst` is a valid, properly allocated AVFrame pointer
    /// and that `codec_ctx` is a valid, open `AVCodecContext`.
    unsafe fn convert_video_frame(
        &mut self,
        src: &VideoFrame,
        dst: *mut AVFrame,
        codec_ctx: *mut AVCodecContext,
    ) -> Result<(), EncodeError> {
        let target_fmt = (*codec_ctx).pix_fmt;
        let target_width = (*codec_ctx).width as u32;
        let target_height = (*codec_ctx).height as u32;

        let src_fmt = pixel_format_to_av(src.format());
        let src_width = src.width();
        let src_height = src.height();

        // Optimization 1: Skip conversion if format and dimensions match
        if src_fmt == target_fmt && src_width == target_width && src_height == target_height {
            return self.copy_frame_direct(src, dst, target_fmt);
        }

        // Optimization 2 & 3: Check if we need to reinitialize SwsContext
        let needs_new_context = self.last_src_width != Some(src_width)
            || self.last_src_height != Some(src_height)
            || self.last_src_format != Some(src_fmt);

        if needs_new_context || self.sws_ctx.is_none() {
            // Free old context if exists
            if let Some(ctx) = self.sws_ctx.take() {
                swscale::free_context(ctx);
            }

            // Create new SwsContext with fast BILINEAR algorithm
            let sws = swscale::get_context(
                src_width as i32,
                src_height as i32,
                src_fmt,
                target_width as i32,
                target_height as i32,
                target_fmt,
                ff_sys::swscale::scale_flags::BILINEAR, // Fast scaling algorithm
            )
            .map_err(EncodeError::from_ffmpeg_error)?;

            self.sws_ctx = Some(sws);
            self.last_src_width = Some(src_width);
            self.last_src_height = Some(src_height);
            self.last_src_format = Some(src_fmt);
        }

        // Perform conversion using cached SwsContext
        self.scale_frame(src, dst, target_fmt, target_width, target_height)
    }

    /// Copy frame data directly without scaling (when formats match).
    unsafe fn copy_frame_direct(
        &self,
        src: &VideoFrame,
        dst: *mut AVFrame,
        target_fmt: AVPixelFormat,
    ) -> Result<(), EncodeError> {
        // Set frame properties
        (*dst).format = target_fmt;
        (*dst).width = src.width() as i32;
        (*dst).height = src.height() as i32;

        // Allocate frame buffer
        let ret = ff_sys::av_frame_get_buffer(dst, 0);
        if ret < 0 {
            return Err(EncodeError::Ffmpeg(format!(
                "Cannot allocate frame buffer: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        // Copy each plane directly
        for (i, plane) in src.planes().iter().enumerate() {
            if i >= (*dst).data.len() || (*dst).data[i].is_null() {
                break;
            }

            // Bounds check for strides array
            let src_stride =
                src.strides().get(i).copied().ok_or_else(|| {
                    EncodeError::Ffmpeg(format!("Missing stride for plane {}", i))
                })?;

            let dst_stride = (*dst).linesize[i] as usize;
            let plane_data = plane.data();
            let plane_height = self.get_plane_height(src.height(), i, src.format());

            // Optimization: If strides match, copy entire plane at once
            if src_stride == dst_stride {
                let total_size = src_stride * plane_height;
                if total_size <= plane_data.len() {
                    ptr::copy_nonoverlapping(plane_data.as_ptr(), (*dst).data[i], total_size);
                    continue;
                }
            }

            // Copy line by line to handle different strides
            for y in 0..plane_height {
                let src_offset = y * src_stride;
                let dst_offset = y * dst_stride;
                let line_size = src_stride.min(dst_stride);

                if src_offset + line_size <= plane_data.len() {
                    ptr::copy_nonoverlapping(
                        plane_data.as_ptr().add(src_offset),
                        (*dst).data[i].add(dst_offset),
                        line_size,
                    );
                }
            }
        }

        Ok(())
    }

    /// Scale frame using SwsContext (when formats or dimensions differ).
    unsafe fn scale_frame(
        &self,
        src: &VideoFrame,
        dst: *mut AVFrame,
        target_fmt: AVPixelFormat,
        target_width: u32,
        target_height: u32,
    ) -> Result<(), EncodeError> {
        // Set frame properties
        (*dst).format = target_fmt;
        (*dst).width = target_width as i32;
        (*dst).height = target_height as i32;

        // Allocate frame buffer
        let ret = ff_sys::av_frame_get_buffer(dst, 0);
        if ret < 0 {
            return Err(EncodeError::Ffmpeg(format!(
                "Cannot allocate frame buffer: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        // Prepare source data pointers and strides
        let mut src_data: [*const u8; MAX_PLANES] = [ptr::null(); MAX_PLANES];
        let mut src_linesize: [i32; MAX_PLANES] = [0; MAX_PLANES];

        for (i, plane) in src.planes().iter().enumerate() {
            if i < MAX_PLANES {
                src_data[i] = plane.data().as_ptr();
                src_linesize[i] = src.strides()[i] as i32;
            }
        }

        // Perform scaling/conversion
        let sws_ctx = self
            .sws_ctx
            .ok_or_else(|| EncodeError::Ffmpeg("Scaling context not initialized".to_string()))?;

        swscale::scale(
            sws_ctx,
            src_data.as_ptr(),
            src_linesize.as_ptr(),
            0,
            src.height() as i32,
            (*dst).data.as_mut_ptr().cast_const(),
            (*dst).linesize.as_mut_ptr(),
        )
        .map_err(EncodeError::from_ffmpeg_error)?;

        Ok(())
    }

    /// Calculate the height of a plane for a given frame height and pixel format.
    ///
    /// Different pixel formats have different plane heights. For YUV 4:2:0 formats,
    /// the U/V planes are half the height of the Y plane.
    ///
    /// # Arguments
    ///
    /// * `frame_height` - The height of the entire frame
    /// * `plane_index` - The plane index (0: Y/RGB, 1: U/UV, 2: V)
    /// * `format` - The pixel format
    ///
    /// # Returns
    ///
    /// The height (number of rows) for the specified plane.
    #[allow(clippy::manual_div_ceil)]
    fn get_plane_height(
        &self,
        frame_height: u32,
        plane_index: usize,
        format: ff_format::PixelFormat,
    ) -> usize {
        use ff_format::PixelFormat;

        match format {
            // YUV 4:2:0 - U and V planes are half height
            PixelFormat::Yuv420p | PixelFormat::Yuv420p10le => {
                if plane_index == 0 {
                    frame_height as usize
                } else {
                    // Safe division with ceiling: (height + 1) / 2
                    // Equivalent to div_ceil(2) but more explicit about avoiding overflow
                    // Note: div_ceil() internally uses (n + d - 1) / d which could overflow
                    ((frame_height as usize) + 1) / 2
                }
            }
            // Semi-planar NV12/NV21/P010 - UV plane is half height
            PixelFormat::Nv12 | PixelFormat::Nv21 | PixelFormat::P010le => {
                if plane_index == 0 {
                    frame_height as usize
                } else {
                    // Safe division with ceiling: (height + 1) / 2
                    ((frame_height as usize) + 1) / 2
                }
            }
            // All other formats - full height for all planes
            _ => frame_height as usize,
        }
    }

    /// Receive encoded packets from the encoder.
    unsafe fn receive_packets(&mut self) -> Result<(), EncodeError> {
        let codec_ctx = self
            .video_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Video codec not initialized".to_string(),
            })?;

        let mut packet = av_packet_alloc();
        if packet.is_null() {
            return Err(EncodeError::Ffmpeg("Cannot allocate packet".to_string()));
        }

        loop {
            match avcodec::receive_packet(codec_ctx, packet) {
                Ok(()) => {
                    // Packet received successfully
                }
                Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                    // No more packets available
                    break;
                }
                Err(e) => {
                    av_packet_free(&mut packet as *mut *mut _);
                    return Err(EncodeError::Ffmpeg(format!(
                        "Error receiving packet: {}",
                        ff_sys::av_error_string(e)
                    )));
                }
            }

            // Set stream index
            (*packet).stream_index = self.video_stream_index;

            // Write packet
            let write_ret = av_interleaved_write_frame(self.format_ctx, packet);
            if write_ret < 0 {
                av_packet_unref(packet);
                av_packet_free(&mut packet as *mut *mut _);
                return Err(EncodeError::MuxingFailed {
                    reason: ff_sys::av_error_string(write_ret),
                });
            }

            self.bytes_written += (*packet).size as u64;

            av_packet_unref(packet);
        }

        av_packet_free(&mut packet as *mut *mut _);
        Ok(())
    }

    /// Push an audio frame for encoding.
    pub(super) unsafe fn push_audio_frame(
        &mut self,
        frame: &AudioFrame,
    ) -> Result<(), EncodeError> {
        let codec_ctx = self
            .audio_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Audio codec not initialized".to_string(),
            })?;

        // Allocate AVFrame
        let mut av_frame = av_frame_alloc();
        if av_frame.is_null() {
            return Err(EncodeError::Ffmpeg("Cannot allocate frame".to_string()));
        }

        // Convert AudioFrame to AVFrame
        let convert_result = self.convert_audio_frame(frame, av_frame);
        if let Err(e) = convert_result {
            av_frame_free(&mut av_frame as *mut *mut _);
            return Err(e);
        }

        // Set frame properties
        (*av_frame).pts = self.audio_sample_count as i64;

        // Send frame to encoder
        let send_result = avcodec::send_frame(codec_ctx, av_frame);
        if let Err(e) = send_result {
            av_frame_free(&mut av_frame as *mut *mut _);
            return Err(EncodeError::Ffmpeg(format!(
                "Failed to send audio frame: {}",
                ff_sys::av_error_string(e)
            )));
        }

        // Receive packets
        let receive_result = self.receive_audio_packets();

        // Always cleanup the frame
        av_frame_free(&mut av_frame as *mut *mut _);

        // Check if receiving packets failed
        receive_result?;

        self.audio_sample_count += frame.samples() as u64;

        Ok(())
    }

    /// Convert AudioFrame to AVFrame with resampling if needed.
    unsafe fn convert_audio_frame(
        &mut self,
        src: &AudioFrame,
        dst: *mut AVFrame,
    ) -> Result<(), EncodeError> {
        let codec_ctx = self
            .audio_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Audio codec not initialized".to_string(),
            })?;

        let target_sample_rate = (*codec_ctx).sample_rate;
        let target_format = (*codec_ctx).sample_fmt;
        let target_ch_layout = &(*codec_ctx).ch_layout;

        // Check if we need to resample
        let src_sample_rate = src.sample_rate() as i32;
        let src_format = sample_format_to_av(src.format());
        let src_ch_layout = {
            let mut layout = AVChannelLayout::default();
            swresample::channel_layout::set_default(&mut layout, src.channels() as i32);
            layout
        };

        let needs_resampling = src_sample_rate != target_sample_rate
            || src_format != target_format
            || !swresample::channel_layout::is_equal(&src_ch_layout, target_ch_layout);

        if needs_resampling {
            // Initialize resampler if needed
            if self.swr_ctx.is_none() {
                let swr_ctx = swresample::alloc_set_opts2(
                    target_ch_layout,
                    target_format,
                    target_sample_rate,
                    &src_ch_layout,
                    src_format,
                    src_sample_rate,
                )
                .map_err(EncodeError::from_ffmpeg_error)?;

                swresample::init(swr_ctx).map_err(EncodeError::from_ffmpeg_error)?;
                self.swr_ctx = Some(swr_ctx);
            }

            let swr_ctx = self.swr_ctx.ok_or_else(|| {
                EncodeError::Ffmpeg("Resampling context not initialized".to_string())
            })?;

            // Estimate output sample count
            let out_samples = swresample::estimate_output_samples(
                target_sample_rate,
                src_sample_rate,
                src.samples() as i32,
            );

            // Set frame properties
            (*dst).format = target_format;
            (*dst).sample_rate = target_sample_rate;
            (*dst).nb_samples = out_samples;

            // Copy target channel layout
            swresample::channel_layout::copy(&mut (*dst).ch_layout, target_ch_layout)
                .map_err(EncodeError::from_ffmpeg_error)?;

            // Allocate frame buffer
            let ret = ff_sys::av_frame_get_buffer(dst, 0);
            if ret < 0 {
                return Err(EncodeError::Ffmpeg(format!(
                    "Cannot allocate audio frame buffer: {}",
                    ff_sys::av_error_string(ret)
                )));
            }

            // Prepare input pointers
            let in_ptrs: Vec<*const u8> = if src.format().is_planar() {
                // Planar: one pointer per channel
                src.planes().iter().map(|p| p.as_ptr()).collect()
            } else {
                // Packed: single pointer
                vec![src.planes()[0].as_ptr()]
            };

            // Convert
            let samples_out = swresample::convert(
                swr_ctx,
                (*dst).data.as_mut_ptr().cast(),
                out_samples,
                in_ptrs.as_ptr(),
                src.samples() as i32,
            )
            .map_err(EncodeError::from_ffmpeg_error)?;

            (*dst).nb_samples = samples_out;
        } else {
            // No resampling needed, direct copy
            (*dst).format = src_format;
            (*dst).sample_rate = src_sample_rate;
            (*dst).nb_samples = src.samples() as i32;

            // Copy channel layout
            swresample::channel_layout::copy(&mut (*dst).ch_layout, &src_ch_layout)
                .map_err(EncodeError::from_ffmpeg_error)?;

            // Allocate frame buffer
            let ret = ff_sys::av_frame_get_buffer(dst, 0);
            if ret < 0 {
                return Err(EncodeError::Ffmpeg(format!(
                    "Cannot allocate audio frame buffer: {}",
                    ff_sys::av_error_string(ret)
                )));
            }

            // Copy audio data
            if src.format().is_planar() {
                // Copy each plane
                for (i, plane) in src.planes().iter().enumerate() {
                    if i < (*dst).data.len() && !(*dst).data[i].is_null() {
                        let size = plane.len();
                        ptr::copy_nonoverlapping(plane.as_ptr(), (*dst).data[i], size);
                    }
                }
            } else {
                // Copy single packed buffer
                if !(*dst).data[0].is_null() {
                    let size = src.planes()[0].len();
                    ptr::copy_nonoverlapping(src.planes()[0].as_ptr(), (*dst).data[0], size);
                }
            }
        }

        Ok(())
    }

    /// Receive encoded audio packets from the encoder.
    unsafe fn receive_audio_packets(&mut self) -> Result<(), EncodeError> {
        let codec_ctx = self
            .audio_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Audio codec not initialized".to_string(),
            })?;

        let mut packet = av_packet_alloc();
        if packet.is_null() {
            return Err(EncodeError::Ffmpeg("Cannot allocate packet".to_string()));
        }

        loop {
            match avcodec::receive_packet(codec_ctx, packet) {
                Ok(()) => {
                    // Packet received successfully
                }
                Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                    // No more packets available
                    break;
                }
                Err(e) => {
                    av_packet_free(&mut packet as *mut *mut _);
                    return Err(EncodeError::Ffmpeg(format!(
                        "Error receiving audio packet: {}",
                        ff_sys::av_error_string(e)
                    )));
                }
            }

            // Set stream index
            (*packet).stream_index = self.audio_stream_index;

            // Write packet
            let write_ret = av_interleaved_write_frame(self.format_ctx, packet);
            if write_ret < 0 {
                av_packet_unref(packet);
                av_packet_free(&mut packet as *mut *mut _);
                return Err(EncodeError::MuxingFailed {
                    reason: ff_sys::av_error_string(write_ret),
                });
            }

            self.bytes_written += (*packet).size as u64;

            av_packet_unref(packet);
        }

        av_packet_free(&mut packet as *mut *mut _);
        Ok(())
    }

    /// Finish encoding and write trailer.
    pub(super) unsafe fn finish(&mut self) -> Result<(), EncodeError> {
        // For two-pass, run the second pass now (handles flushing + trailer).
        if self.two_pass {
            return self.run_pass2();
        }

        // Single-pass: flush video encoder
        if let Some(codec_ctx) = self.video_codec_ctx {
            // Send NULL frame to flush
            avcodec::send_frame(codec_ctx, ptr::null()).map_err(EncodeError::from_ffmpeg_error)?;
            self.receive_packets()?;
        }

        // Flush audio encoder
        if let Some(codec_ctx) = self.audio_codec_ctx {
            // Send NULL frame to flush
            avcodec::send_frame(codec_ctx, ptr::null()).map_err(EncodeError::from_ffmpeg_error)?;
            self.receive_audio_packets()?;
        }

        // Write trailer
        let ret = av_write_trailer(self.format_ctx);
        if ret < 0 {
            return Err(EncodeError::Ffmpeg(format!(
                "Cannot write trailer: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        Ok(())
    }

    /// Run the second pass of two-pass encoding.
    ///
    /// 1. Flushes the pass-1 encoder and collects `stats_out`.
    /// 2. Initialises a pass-2 codec context with `AV_CODEC_FLAG_PASS2` and
    ///    the collected statistics.
    /// 3. Opens the real output file and writes the container header.
    /// 4. Re-encodes all buffered frames through the pass-2 context.
    /// 5. Flushes the pass-2 encoder and writes the container trailer.
    ///
    /// # Safety
    ///
    /// Must only be called from `finish` when `self.two_pass` is `true`.
    /// All FFmpeg resources must be valid at the point of the call.
    unsafe fn run_pass2(&mut self) -> Result<(), EncodeError> {
        // ── Step 1: Flush pass-1 encoder ────────────────────────────────────
        let mut pass1_ctx = self
            .pass1_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Pass-1 codec context not available".to_string(),
            })?;

        // SAFETY: pass1_ctx is a valid open codec context.
        if let Err(e) = avcodec::send_frame(pass1_ctx, ptr::null())
            && e != ff_sys::error_codes::EOF
        {
            return Err(EncodeError::Ffmpeg(format!(
                "pass1 flush send_frame: {}",
                ff_sys::av_error_string(e)
            )));
        }
        self.drain_pass1_packets(pass1_ctx)?;

        // ── Step 2: Collect stats_out ────────────────────────────────────────
        // SAFETY: stats_out is either null or a valid C string owned by the
        // codec context; it remains valid until avcodec_free_context is called.
        let stats_str = if !(*pass1_ctx).stats_out.is_null() {
            std::ffi::CStr::from_ptr((*pass1_ctx).stats_out)
                .to_string_lossy()
                .into_owned()
        } else {
            log::warn!(
                "two-pass pass-1 produced no stats_out; pass-2 quality may not improve \
                 codec={}",
                self.actual_video_codec
            );
            String::new()
        };
        log::info!("two-pass pass-1 complete stats_len={}", stats_str.len());

        // ── Step 3: Free pass-1 codec context ───────────────────────────────
        // SAFETY: pass1_ctx is no longer needed; we own it exclusively.
        avcodec::free_context(&mut pass1_ctx as *mut *mut _);
        self.pass1_codec_ctx = None;

        // ── Step 4: Set up pass-2 codec context ─────────────────────────────
        let config = self
            .two_pass_config
            .take()
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Two-pass config not available for pass-2 initialisation".to_string(),
            })?;

        let output_path = config.path.clone();
        self.init_pass2_codec_ctx(&config, &stats_str)?;

        // ── Step 5: Open output file and write header ────────────────────────
        match ff_sys::avformat::open_output(&output_path, ff_sys::avformat::avio_flags::WRITE) {
            Ok(pb) => (*self.format_ctx).pb = pb,
            Err(_) => {
                return Err(EncodeError::CannotCreateFile { path: output_path });
            }
        }

        let ret = avformat_write_header(self.format_ctx, ptr::null_mut());
        if ret < 0 {
            return Err(EncodeError::Ffmpeg(format!(
                "Cannot write header in pass 2: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        // ── Step 6: Re-encode all buffered frames ────────────────────────────
        let frames = std::mem::take(&mut self.buffered_frames);
        self.frame_count = 0;
        for tf in &frames {
            self.push_two_pass_frame(tf)?;
        }

        // ── Step 7: Flush pass-2 encoder and write trailer ───────────────────
        if let Some(codec_ctx) = self.video_codec_ctx {
            // SAFETY: codec_ctx is a valid open pass-2 codec context.
            if let Err(e) = avcodec::send_frame(codec_ctx, ptr::null())
                && e != ff_sys::error_codes::EOF
            {
                return Err(EncodeError::Ffmpeg(format!(
                    "pass2 flush send_frame: {}",
                    ff_sys::av_error_string(e)
                )));
            }
            self.receive_packets()?;
        }

        let ret = av_write_trailer(self.format_ctx);
        if ret < 0 {
            return Err(EncodeError::Ffmpeg(format!(
                "Cannot write trailer: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        Ok(())
    }

    /// Initialise the pass-2 video codec context.
    ///
    /// Mirrors the configuration performed in `init_video_encoder` but sets
    /// `AV_CODEC_FLAG_PASS2` and assigns `stats_in` from the pass-1 statistics
    /// string. Does **not** create a new AVStream — the stream was already
    /// registered during `init_video_encoder` (pass 1).
    ///
    /// # Safety
    ///
    /// Must only be called from `run_pass2`. `self.format_ctx` must be valid.
    unsafe fn init_pass2_codec_ctx(
        &mut self,
        config: &VideoEncoderConfig,
        stats: &str,
    ) -> Result<(), EncodeError> {
        use crate::BitrateMode;
        let width = config.video_width.unwrap_or(0);
        let height = config.video_height.unwrap_or(0);
        let fps = config.video_fps.unwrap_or(30.0);
        let encoder_name = self.actual_video_codec.clone();

        let c_encoder_name = CString::new(encoder_name.as_str())
            .map_err(|_| EncodeError::Ffmpeg("Invalid encoder name for pass 2".to_string()))?;

        let codec_ptr =
            avcodec::find_encoder_by_name(c_encoder_name.as_ptr()).ok_or_else(|| {
                EncodeError::NoSuitableEncoder {
                    codec: encoder_name.clone(),
                    tried: vec![encoder_name.clone()],
                }
            })?;

        let codec_ctx =
            avcodec::alloc_context3(codec_ptr).map_err(EncodeError::from_ffmpeg_error)?;

        // Mirror the same codec configuration as pass 1.
        (*codec_ctx).codec_id = codec_to_id(config.video_codec);
        (*codec_ctx).width = width as i32;
        (*codec_ctx).height = height as i32;
        (*codec_ctx).time_base.num = 1;
        (*codec_ctx).time_base.den = (fps * 1000.0) as i32;
        (*codec_ctx).framerate.num = fps as i32;
        (*codec_ctx).framerate.den = 1;
        (*codec_ctx).pix_fmt = AVPixelFormat_AV_PIX_FMT_YUV420P;

        match config.video_bitrate_mode.as_ref() {
            Some(BitrateMode::Cbr(bps)) => {
                (*codec_ctx).bit_rate = *bps as i64;
            }
            Some(BitrateMode::Vbr { target, max }) => {
                (*codec_ctx).bit_rate = *target as i64;
                (*codec_ctx).rc_max_rate = *max as i64;
                (*codec_ctx).rc_buffer_size = (*max * 2) as i32;
            }
            Some(BitrateMode::Crf(q)) => {
                let crf_str = CString::new(q.to_string())
                    .map_err(|_| EncodeError::Ffmpeg("Invalid CRF value".to_string()))?;
                // SAFETY: priv_data, option name, and value are all valid pointers.
                let ret = ff_sys::av_opt_set(
                    (*codec_ctx).priv_data,
                    b"crf\0".as_ptr() as *const i8,
                    crf_str.as_ptr(),
                    0,
                );
                if ret < 0 {
                    log::warn!(
                        "crf option not supported by pass-2 encoder, falling back to default \
                         encoder={encoder_name} crf={q}"
                    );
                    (*codec_ctx).bit_rate = 2_000_000;
                }
            }
            None => {
                (*codec_ctx).bit_rate = 2_000_000;
            }
        }

        if encoder_name.contains("264") || encoder_name.contains("265") {
            let preset_cstr = CString::new(config.preset.as_str())
                .map_err(|_| EncodeError::Ffmpeg("Invalid preset value".to_string()))?;
            // SAFETY: priv_data, option name, and value are all valid pointers.
            let ret = ff_sys::av_opt_set(
                (*codec_ctx).priv_data,
                b"preset\0".as_ptr() as *const i8,
                preset_cstr.as_ptr(),
                0,
            );
            if ret < 0 {
                log::warn!(
                    "preset option not supported by pass-2 encoder, ignoring \
                     encoder={encoder_name} preset={}",
                    config.preset
                );
            }
        }

        // Set the pass-2 flag and provide stats_in.
        // SAFETY: codec_ctx is a valid allocated (but not yet opened) context.
        (*codec_ctx).flags |= AV_CODEC_FLAG_PASS2;

        // Point stats_in to our owned CString (kept alive in self.stats_in_cstr
        // until cleanup() nulls the pointer and drops it).
        if !stats.is_empty() {
            let stats_cstr = CString::new(stats)
                .map_err(|_| EncodeError::Ffmpeg("Invalid stats string from pass 1".to_string()))?;
            // SAFETY: stats_cstr.as_ptr() is valid for the lifetime of stats_cstr,
            // which is stored in self.stats_in_cstr and dropped only after the codec
            // context is freed in cleanup().
            (*codec_ctx).stats_in = stats_cstr.as_ptr().cast_mut();
            self.stats_in_cstr = Some(stats_cstr);
        }

        // Try to open the pass-2 codec with PASS2 flag. Some encoders (e.g. the
        // native mpeg4 encoder without meaningful stats) do not support PASS2 and
        // return AVERROR(EPERM). In that case, fall back to opening without the
        // flag so the caller still gets a valid encoder and usable output.
        if avcodec::open2(codec_ctx, codec_ptr, ptr::null_mut()).is_err() {
            log::warn!(
                "two-pass pass-2 codec rejected AV_CODEC_FLAG_PASS2, \
                 falling back to single-pass mode codec={encoder_name}"
            );
            (*codec_ctx).flags &= !AV_CODEC_FLAG_PASS2;
            (*codec_ctx).stats_in = ptr::null_mut();
            self.stats_in_cstr = None;
            avcodec::open2(codec_ctx, codec_ptr, ptr::null_mut()).map_err(|e| {
                EncodeError::Ffmpeg(format!(
                    "pass2 avcodec_open2 fallback: {}",
                    ff_sys::av_error_string(e)
                ))
            })?;
        }
        log::info!(
            "two-pass pass-2 codec opened codec={encoder_name} width={width} height={height}"
        );

        self.video_codec_ctx = Some(codec_ctx);
        Ok(())
    }

    /// Encode a single buffered YUV420P frame through the pass-2 codec context.
    ///
    /// The frame data was captured during pass 1 (already converted to YUV420P)
    /// and is re-encoded here with the optimised pass-2 settings.
    ///
    /// # Safety
    ///
    /// Must only be called from `run_pass2`. `self.video_codec_ctx` and
    /// `self.format_ctx` must be valid and the output file must be open.
    unsafe fn push_two_pass_frame(&mut self, tf: &TwoPassFrame) -> Result<(), EncodeError> {
        let codec_ctx = self
            .video_codec_ctx
            .ok_or_else(|| EncodeError::InvalidConfig {
                reason: "Pass-2 codec context not initialized".to_string(),
            })?;

        let mut av_frame = av_frame_alloc();
        if av_frame.is_null() {
            return Err(EncodeError::Ffmpeg(
                "Cannot allocate frame for pass 2".to_string(),
            ));
        }

        // Set frame format — always YUV420P (converted during pass 1).
        (*av_frame).format = AVPixelFormat_AV_PIX_FMT_YUV420P;
        (*av_frame).width = tf.width as i32;
        (*av_frame).height = tf.height as i32;

        // Allocate the frame buffer.
        let ret = ff_sys::av_frame_get_buffer(av_frame, 0);
        if ret < 0 {
            av_frame_free(&mut av_frame as *mut *mut _);
            return Err(EncodeError::Ffmpeg(format!(
                "Cannot allocate pass-2 frame buffer: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        // Copy the buffered YUV420P data into the AVFrame.
        let uv_height = (tf.height as usize).div_ceil(2);
        for (plane_idx, (plane_data, src_stride)) in
            tf.planes.iter().zip(tf.strides.iter()).enumerate()
        {
            if plane_idx >= 3 || (*av_frame).data[plane_idx].is_null() || plane_data.is_empty() {
                break;
            }
            let dst_stride = (*av_frame).linesize[plane_idx] as usize;
            let plane_height = if plane_idx == 0 {
                tf.height as usize
            } else {
                uv_height
            };

            for row in 0..plane_height {
                let src_off = row * src_stride;
                let dst_off = row * dst_stride;
                let copy_len = (*src_stride).min(dst_stride);

                if src_off + copy_len <= plane_data.len() {
                    // SAFETY: bounds checked above; both pointers are valid and
                    // the regions do not overlap.
                    ptr::copy_nonoverlapping(
                        plane_data.as_ptr().add(src_off),
                        (*av_frame).data[plane_idx].add(dst_off),
                        copy_len,
                    );
                }
            }
        }

        (*av_frame).pts = tf.pts;

        // Send to pass-2 encoder.
        let send_result = avcodec::send_frame(codec_ctx, av_frame);
        if let Err(e) = send_result {
            av_frame_free(&mut av_frame as *mut *mut _);
            return Err(EncodeError::Ffmpeg(format!(
                "Failed to send frame to pass-2 encoder: {}",
                ff_sys::av_error_string(e)
            )));
        }

        let receive_result = self.receive_packets();
        av_frame_free(&mut av_frame as *mut *mut _);
        receive_result?;

        self.frame_count += 1;
        Ok(())
    }

    /// Cleanup FFmpeg resources.
    unsafe fn cleanup(&mut self) {
        // Free video codec context.
        // For two-pass encoding, stats_in points into self.stats_in_cstr (Rust-owned).
        // Null it out BEFORE avcodec_free_context so FFmpeg does not call av_free on it.
        if let Some(mut ctx) = self.video_codec_ctx.take() {
            (*ctx).stats_in = ptr::null_mut();
            avcodec::free_context(&mut ctx as *mut *mut _);
        }
        // Drop the owned CString now that the codec context no longer references it.
        self.stats_in_cstr = None;

        // Free pass-1 codec context (only set in two-pass mode).
        if let Some(mut ctx) = self.pass1_codec_ctx.take() {
            avcodec::free_context(&mut ctx as *mut *mut _);
        }

        // Free audio codec context
        if let Some(mut ctx) = self.audio_codec_ctx.take() {
            avcodec::free_context(&mut ctx as *mut *mut _);
        }

        // Free scaling context
        if let Some(ctx) = self.sws_ctx.take() {
            swscale::free_context(ctx);
        }

        // Free resampling context
        if let Some(mut ctx) = self.swr_ctx.take() {
            swresample::free(&mut ctx as *mut *mut _);
        }

        // Close output file
        if !self.format_ctx.is_null() {
            if !(*self.format_ctx).pb.is_null() {
                ff_sys::avformat::close_output(&mut (*self.format_ctx).pb);
            }
            avformat_free_context(self.format_ctx);
            self.format_ctx = ptr::null_mut();
        }
    }
}

impl Drop for VideoEncoderInner {
    fn drop(&mut self) {
        // SAFETY: We own all the FFmpeg resources and need to free them
        unsafe {
            self.cleanup();
        }
    }
}

// Helper functions

fn codec_to_id(codec: VideoCodec) -> AVCodecID {
    match codec {
        VideoCodec::H264 => AVCodecID_AV_CODEC_ID_H264,
        VideoCodec::H265 => AVCodecID_AV_CODEC_ID_HEVC,
        VideoCodec::Vp9 => AVCodecID_AV_CODEC_ID_VP9,
        VideoCodec::Av1 => AVCodecID_AV_CODEC_ID_AV1,
        VideoCodec::ProRes => AVCodecID_AV_CODEC_ID_PRORES,
        VideoCodec::DnxHd => AVCodecID_AV_CODEC_ID_DNXHD,
        VideoCodec::Mpeg4 => AVCodecID_AV_CODEC_ID_MPEG4,
    }
}

pub(super) fn preset_to_string(preset: &crate::Preset) -> String {
    match preset {
        crate::Preset::Ultrafast => "ultrafast",
        crate::Preset::Faster => "faster",
        crate::Preset::Fast => "fast",
        crate::Preset::Medium => "medium",
        crate::Preset::Slow => "slow",
        crate::Preset::Slower => "slower",
        crate::Preset::Veryslow => "veryslow",
    }
    .to_string()
}

/// Convert AudioCodec to FFmpeg AVCodecID.
fn audio_codec_to_id(codec: AudioCodec) -> AVCodecID {
    match codec {
        AudioCodec::Aac => AVCodecID_AV_CODEC_ID_AAC,
        AudioCodec::Opus => AVCodecID_AV_CODEC_ID_OPUS,
        AudioCodec::Mp3 => AVCodecID_AV_CODEC_ID_MP3,
        AudioCodec::Flac => AVCodecID_AV_CODEC_ID_FLAC,
        AudioCodec::Pcm => AVCodecID_AV_CODEC_ID_PCM_S16LE,
        AudioCodec::Vorbis => AVCodecID_AV_CODEC_ID_VORBIS,
    }
}

/// Convert ff-format SampleFormat to FFmpeg AVSampleFormat.
fn sample_format_to_av(format: ff_format::SampleFormat) -> ff_sys::AVSampleFormat {
    use ff_format::SampleFormat;
    use ff_sys::swresample::sample_format;

    match format {
        SampleFormat::U8 => sample_format::U8,
        SampleFormat::I16 => sample_format::S16,
        SampleFormat::I32 => sample_format::S32,
        SampleFormat::F32 => sample_format::FLT,
        SampleFormat::F64 => sample_format::DBL,
        SampleFormat::U8p => sample_format::U8P,
        SampleFormat::I16p => sample_format::S16P,
        SampleFormat::I32p => sample_format::S32P,
        SampleFormat::F32p => sample_format::FLTP,
        SampleFormat::F64p => sample_format::DBLP,
        _ => {
            log::warn!(
                "sample_format has no AV mapping, falling back to FLTP \
                 format={format:?} fallback=FLTP"
            );
            sample_format::FLTP
        }
    }
}

/// Convert ff-format PixelFormat to FFmpeg AVPixelFormat.
fn pixel_format_to_av(format: ff_format::PixelFormat) -> AVPixelFormat {
    use ff_format::PixelFormat;

    match format {
        PixelFormat::Yuv420p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P,
        PixelFormat::Yuv422p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P,
        PixelFormat::Yuv444p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P,
        PixelFormat::Rgb24 => ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24,
        PixelFormat::Bgr24 => ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24,
        PixelFormat::Rgba => ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA,
        PixelFormat::Bgra => ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA,
        PixelFormat::Gray8 => ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8,
        PixelFormat::Nv12 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV12,
        PixelFormat::Nv21 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV21,
        PixelFormat::Yuv420p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE,
        PixelFormat::P010le => ff_sys::AVPixelFormat_AV_PIX_FMT_P010LE,
        _ => {
            log::warn!(
                "pixel_format has no AV mapping, falling back to Yuv420p \
                 format={format:?} fallback=AV_PIX_FMT_YUV420P"
            );
            ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h264_encoder_candidates_auto() {
        let inner = create_dummy_encoder_inner();
        let candidates = inner.select_h264_encoder_candidates(crate::HardwareEncoder::Auto);

        // Should include hardware encoders
        #[cfg(feature = "hwaccel")]
        {
            assert!(candidates.contains(&"h264_nvenc"));
            assert!(candidates.contains(&"h264_qsv"));
        }

        // Should include libx264 if GPL feature is enabled
        #[cfg(feature = "gpl")]
        {
            assert!(candidates.contains(&"libx264"));
        }

        // Should always include VP9 fallback
        assert!(candidates.contains(&"libvpx-vp9"));
    }

    #[test]
    fn test_h264_encoder_candidates_nvenc() {
        let inner = create_dummy_encoder_inner();
        let candidates = inner.select_h264_encoder_candidates(crate::HardwareEncoder::Nvenc);

        #[cfg(feature = "hwaccel")]
        {
            // NVENC should be first priority
            assert_eq!(candidates[0], "h264_nvenc");
        }

        // Should include VP9 fallback
        assert!(candidates.contains(&"libvpx-vp9"));
    }

    #[test]
    fn test_h264_encoder_candidates_none() {
        let inner = create_dummy_encoder_inner();
        let candidates = inner.select_h264_encoder_candidates(crate::HardwareEncoder::None);

        #[cfg(feature = "hwaccel")]
        {
            // Should not include hardware encoders
            assert!(!candidates.contains(&"h264_nvenc"));
            assert!(!candidates.contains(&"h264_qsv"));
        }

        // Should include VP9 fallback
        assert!(candidates.contains(&"libvpx-vp9"));
    }

    #[test]
    fn test_h265_encoder_candidates_auto() {
        let inner = create_dummy_encoder_inner();
        let candidates = inner.select_h265_encoder_candidates(crate::HardwareEncoder::Auto);

        // Should include hardware encoders
        #[cfg(feature = "hwaccel")]
        {
            assert!(candidates.contains(&"hevc_nvenc"));
            assert!(candidates.contains(&"hevc_qsv"));
        }

        // Should include libx265 if GPL feature is enabled
        #[cfg(feature = "gpl")]
        {
            assert!(candidates.contains(&"libx265"));
        }

        // Should always include AV1 fallback
        assert!(candidates.contains(&"libaom-av1") || candidates.contains(&"libsvtav1"));
    }

    #[test]
    fn test_lgpl_fallback_priority() {
        let inner = create_dummy_encoder_inner();

        // Test H264 candidates
        let h264_candidates = inner.select_h264_encoder_candidates(crate::HardwareEncoder::None);

        #[cfg(not(feature = "gpl"))]
        {
            // Without GPL feature, should only have VP9
            assert_eq!(h264_candidates, vec!["libvpx-vp9"]);
        }

        // Test H265 candidates
        let h265_candidates = inner.select_h265_encoder_candidates(crate::HardwareEncoder::None);

        #[cfg(not(feature = "gpl"))]
        {
            // Without GPL feature, should only have AV1 options
            assert!(h265_candidates.contains(&"libaom-av1"));
            assert!(!h265_candidates.contains(&"libx265"));
        }
    }

    #[test]
    fn test_get_plane_height_yuv420p() {
        let inner = create_dummy_encoder_inner();

        // Test YUV420P format - Y plane is full height, U/V planes are half height
        // Even height (640x480)
        assert_eq!(
            inner.get_plane_height(480, 0, ff_format::PixelFormat::Yuv420p),
            480
        );
        assert_eq!(
            inner.get_plane_height(480, 1, ff_format::PixelFormat::Yuv420p),
            240
        );
        assert_eq!(
            inner.get_plane_height(480, 2, ff_format::PixelFormat::Yuv420p),
            240
        );

        // Odd height (641x481) - test ceiling division
        assert_eq!(
            inner.get_plane_height(481, 0, ff_format::PixelFormat::Yuv420p),
            481
        );
        assert_eq!(
            inner.get_plane_height(481, 1, ff_format::PixelFormat::Yuv420p),
            241
        ); // (481 + 1) / 2 = 241
        assert_eq!(
            inner.get_plane_height(481, 2, ff_format::PixelFormat::Yuv420p),
            241
        );
    }

    #[test]
    fn test_get_plane_height_nv12() {
        let inner = create_dummy_encoder_inner();

        // Test NV12 format - Y plane is full height, UV plane is half height
        assert_eq!(
            inner.get_plane_height(1080, 0, ff_format::PixelFormat::Nv12),
            1080
        );
        assert_eq!(
            inner.get_plane_height(1080, 1, ff_format::PixelFormat::Nv12),
            540
        );

        // Odd height
        assert_eq!(
            inner.get_plane_height(1081, 0, ff_format::PixelFormat::Nv12),
            1081
        );
        assert_eq!(
            inner.get_plane_height(1081, 1, ff_format::PixelFormat::Nv12),
            541
        ); // (1081 + 1) / 2 = 541
    }

    #[test]
    fn test_get_plane_height_rgba() {
        let inner = create_dummy_encoder_inner();

        // Test RGBA format - all planes are full height (only 1 plane)
        assert_eq!(
            inner.get_plane_height(720, 0, ff_format::PixelFormat::Rgba),
            720
        );
        assert_eq!(
            inner.get_plane_height(720, 1, ff_format::PixelFormat::Rgba),
            720
        );
    }

    #[test]
    fn test_pixel_format_to_av() {
        // Test common pixel formats
        assert_eq!(
            pixel_format_to_av(ff_format::PixelFormat::Yuv420p),
            ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P
        );
        assert_eq!(
            pixel_format_to_av(ff_format::PixelFormat::Rgba),
            ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA
        );
        assert_eq!(
            pixel_format_to_av(ff_format::PixelFormat::Nv12),
            ff_sys::AVPixelFormat_AV_PIX_FMT_NV12
        );

        // Test fallback for Other format
        assert_eq!(
            pixel_format_to_av(ff_format::PixelFormat::Other(999)),
            ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P
        );
    }

    #[test]
    fn test_sws_context_tracking() {
        let mut inner = create_dummy_encoder_inner();

        // Initially no context
        assert_eq!(inner.last_src_width, None);
        assert_eq!(inner.last_src_height, None);
        assert_eq!(inner.last_src_format, None);

        // After setting (simulating what convert_video_frame does)
        inner.last_src_width = Some(1920);
        inner.last_src_height = Some(1080);
        inner.last_src_format = Some(ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P);

        // Verify tracking
        assert_eq!(inner.last_src_width, Some(1920));
        assert_eq!(inner.last_src_height, Some(1080));
        assert_eq!(
            inner.last_src_format,
            Some(ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P)
        );
    }

    /// Helper function to create a dummy encoder inner for testing.
    fn create_dummy_encoder_inner() -> VideoEncoderInner {
        VideoEncoderInner {
            format_ctx: ptr::null_mut(),
            video_codec_ctx: None,
            audio_codec_ctx: None,
            video_stream_index: -1,
            audio_stream_index: -1,
            sws_ctx: None,
            swr_ctx: None,
            frame_count: 0,
            audio_sample_count: 0,
            bytes_written: 0,
            actual_video_codec: String::new(),
            actual_audio_codec: String::new(),
            last_src_width: None,
            last_src_height: None,
            last_src_format: None,
            two_pass: false,
            pass1_codec_ctx: None,
            buffered_frames: Vec::new(),
            two_pass_config: None,
            stats_in_cstr: None,
        }
    }
}
