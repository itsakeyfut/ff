//! Media file information extraction.
//!
//! This module provides the [`open`] function for extracting metadata from media files
//! using `FFmpeg`. It creates a [`MediaInfo`] struct containing all relevant information
//! about the media file, including container format, duration, file size, and stream details.
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```no_run
//! use ff_probe::open;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let info = open("video.mp4")?;
//!
//!     println!("Format: {}", info.format());
//!     println!("Duration: {:?}", info.duration());
//!
//!     // Access video stream information
//!     if let Some(video) = info.primary_video() {
//!         println!("Video: {} {}x{} @ {:.2} fps",
//!             video.codec_name(),
//!             video.width(),
//!             video.height(),
//!             video.fps()
//!         );
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Checking for Video vs Audio-Only Files
//!
//! ```no_run
//! use ff_probe::open;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let info = open("media_file.mp4")?;
//!
//!     if info.has_video() {
//!         println!("This is a video file");
//!     } else if info.has_audio() {
//!         println!("This is an audio-only file");
//!     }
//!
//!     Ok(())
//! }
//! ```

// This module requires unsafe code for FFmpeg FFI interactions
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::CStr;
use std::path::Path;
use std::time::Duration;

use ff_format::channel::ChannelLayout;
use ff_format::codec::{AudioCodec, VideoCodec};
use ff_format::color::{ColorPrimaries, ColorRange, ColorSpace};
use ff_format::stream::{AudioStreamInfo, VideoStreamInfo};
use ff_format::{MediaInfo, PixelFormat, Rational, SampleFormat};

use crate::error::ProbeError;

/// `AV_TIME_BASE` constant from `FFmpeg` (microseconds per second).
const AV_TIME_BASE: i64 = 1_000_000;

/// Opens a media file and extracts its metadata.
///
/// This function opens the file at the given path using `FFmpeg`, reads the container
/// format information, and returns a [`MediaInfo`] struct containing all extracted
/// metadata.
///
/// # Arguments
///
/// * `path` - Path to the media file to probe. Accepts anything that can be converted
///   to a [`Path`], including `&str`, `String`, `PathBuf`, etc.
///
/// # Returns
///
/// Returns `Ok(MediaInfo)` on success, or a [`ProbeError`] on failure.
///
/// # Errors
///
/// - [`ProbeError::FileNotFound`] if the file does not exist
/// - [`ProbeError::CannotOpen`] if `FFmpeg` cannot open the file
/// - [`ProbeError::InvalidMedia`] if stream information cannot be read
/// - [`ProbeError::Io`] if there's an I/O error accessing the file
///
/// # Examples
///
/// ## Opening a Video File
///
/// ```no_run
/// use ff_probe::open;
/// use std::path::Path;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Open by string path
///     let info = open("video.mp4")?;
///
///     // Or by Path
///     let path = Path::new("/path/to/video.mkv");
///     let info = open(path)?;
///
///     if let Some(video) = info.primary_video() {
///         println!("Resolution: {}x{}", video.width(), video.height());
///     }
///
///     Ok(())
/// }
/// ```
///
/// ## Handling Errors
///
/// ```
/// use ff_probe::{open, ProbeError};
///
/// // Non-existent file returns FileNotFound
/// let result = open("/this/file/does/not/exist.mp4");
/// assert!(matches!(result, Err(ProbeError::FileNotFound { .. })));
/// ```
pub fn open(path: impl AsRef<Path>) -> Result<MediaInfo, ProbeError> {
    let path = path.as_ref();

    // Check if file exists
    if !path.exists() {
        return Err(ProbeError::FileNotFound {
            path: path.to_path_buf(),
        });
    }

    // Get file size - propagate error since file may exist but be inaccessible (permission denied, etc.)
    let file_size = std::fs::metadata(path).map(|m| m.len())?;

    // Open file with FFmpeg
    // SAFETY: We verified the file exists, and we properly close the context on all paths
    let ctx = unsafe { ff_sys::avformat::open_input(path) }.map_err(|err_code| {
        ProbeError::CannotOpen {
            path: path.to_path_buf(),
            reason: ff_sys::av_error_string(err_code),
        }
    })?;

    // Find stream info - this populates codec information
    // SAFETY: ctx is valid from open_input
    if let Err(err_code) = unsafe { ff_sys::avformat::find_stream_info(ctx) } {
        // SAFETY: ctx is valid
        unsafe {
            let mut ctx_ptr = ctx;
            ff_sys::avformat::close_input(&raw mut ctx_ptr);
        }
        return Err(ProbeError::InvalidMedia {
            path: path.to_path_buf(),
            reason: ff_sys::av_error_string(err_code),
        });
    }

    // Extract basic information from AVFormatContext
    // SAFETY: ctx is valid and find_stream_info succeeded
    let (format, format_long_name, duration) = unsafe { extract_format_info(ctx) };

    // Calculate container bitrate
    // SAFETY: ctx is valid and find_stream_info succeeded
    let bitrate = unsafe { calculate_container_bitrate(ctx, file_size, duration) };

    // Extract container metadata
    // SAFETY: ctx is valid and find_stream_info succeeded
    let metadata = unsafe { extract_metadata(ctx) };

    // Extract video streams
    // SAFETY: ctx is valid and find_stream_info succeeded
    let video_streams = unsafe { extract_video_streams(ctx) };

    // Extract audio streams
    // SAFETY: ctx is valid and find_stream_info succeeded
    let audio_streams = unsafe { extract_audio_streams(ctx) };

    // Close the format context
    // SAFETY: ctx is valid
    unsafe {
        let mut ctx_ptr = ctx;
        ff_sys::avformat::close_input(&raw mut ctx_ptr);
    }

    // Build MediaInfo
    let mut builder = MediaInfo::builder()
        .path(path)
        .format(format)
        .duration(duration)
        .file_size(file_size)
        .video_streams(video_streams)
        .audio_streams(audio_streams)
        .metadata_map(metadata);

    if let Some(name) = format_long_name {
        builder = builder.format_long_name(name);
    }

    if let Some(bps) = bitrate {
        builder = builder.bitrate(bps);
    }

    Ok(builder.build())
}

/// Extracts format information from an `AVFormatContext`.
///
/// # Safety
///
/// The `ctx` pointer must be valid and properly initialized by `avformat_open_input`.
unsafe fn extract_format_info(
    ctx: *mut ff_sys::AVFormatContext,
) -> (String, Option<String>, Duration) {
    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let format = extract_format_name(ctx);
        let format_long_name = extract_format_long_name(ctx);
        let duration = extract_duration(ctx);

        (format, format_long_name, duration)
    }
}

/// Extracts the format name from an `AVFormatContext`.
///
/// # Safety
///
/// The `ctx` pointer must be valid.
unsafe fn extract_format_name(ctx: *mut ff_sys::AVFormatContext) -> String {
    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let iformat = (*ctx).iformat;
        if iformat.is_null() {
            return String::from("unknown");
        }

        let name_ptr = (*iformat).name;
        if name_ptr.is_null() {
            return String::from("unknown");
        }

        CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
    }
}

/// Extracts the long format name from an `AVFormatContext`.
///
/// # Safety
///
/// The `ctx` pointer must be valid.
unsafe fn extract_format_long_name(ctx: *mut ff_sys::AVFormatContext) -> Option<String> {
    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let iformat = (*ctx).iformat;
        if iformat.is_null() {
            return None;
        }

        let long_name_ptr = (*iformat).long_name;
        if long_name_ptr.is_null() {
            return None;
        }

        Some(CStr::from_ptr(long_name_ptr).to_string_lossy().into_owned())
    }
}

/// Extracts the duration from an `AVFormatContext`.
///
/// The duration is stored in `AV_TIME_BASE` units (microseconds).
/// If the duration is not available or is invalid, returns `Duration::ZERO`.
///
/// # Safety
///
/// The `ctx` pointer must be valid.
unsafe fn extract_duration(ctx: *mut ff_sys::AVFormatContext) -> Duration {
    // SAFETY: Caller guarantees ctx is valid
    let duration_us = unsafe { (*ctx).duration };

    // duration_us == 0: Container does not provide duration info (e.g., live streams)
    // duration_us < 0: AV_NOPTS_VALUE (typically i64::MIN), indicating unknown duration
    if duration_us <= 0 {
        return Duration::ZERO;
    }

    // Convert from microseconds to Duration
    // duration is in AV_TIME_BASE units (1/1000000 seconds)
    // Safe cast: we verified duration_us > 0 above
    #[expect(clippy::cast_sign_loss, reason = "verified duration_us > 0")]
    let secs = (duration_us / AV_TIME_BASE) as u64;
    #[expect(clippy::cast_sign_loss, reason = "verified duration_us > 0")]
    let micros = (duration_us % AV_TIME_BASE) as u32;

    Duration::new(secs, micros * 1000)
}

/// Calculates the overall bitrate for a media file.
///
/// This function first tries to get the bitrate directly from the `AVFormatContext`.
/// If the bitrate is not available (i.e., 0 or negative), it falls back to calculating
/// the bitrate from the file size and duration: `bitrate = file_size * 8 / duration`.
///
/// # Arguments
///
/// * `ctx` - The `AVFormatContext` to extract bitrate from
/// * `file_size` - The file size in bytes
/// * `duration` - The duration of the media
///
/// # Returns
///
/// Returns `Some(bitrate)` in bits per second, or `None` if neither method can determine
/// the bitrate (e.g., if duration is zero).
///
/// # Safety
///
/// The `ctx` pointer must be valid.
unsafe fn calculate_container_bitrate(
    ctx: *mut ff_sys::AVFormatContext,
    file_size: u64,
    duration: Duration,
) -> Option<u64> {
    // SAFETY: Caller guarantees ctx is valid
    let bitrate = unsafe { (*ctx).bit_rate };

    // If bitrate is available from FFmpeg, use it directly
    if bitrate > 0 {
        #[expect(clippy::cast_sign_loss, reason = "verified bitrate > 0")]
        return Some(bitrate as u64);
    }

    // Fallback: calculate from file size and duration
    // bitrate (bps) = file_size (bytes) * 8 (bits/byte) / duration (seconds)
    let duration_secs = duration.as_secs_f64();
    if duration_secs > 0.0 && file_size > 0 {
        // Note: Precision loss from u64->f64 is acceptable here because:
        // 1. For files up to 9 PB, f64 provides sufficient precision
        // 2. The result is used for display/metadata purposes, not exact calculations
        #[expect(
            clippy::cast_precision_loss,
            reason = "precision loss acceptable for file size; f64 handles up to 9 PB"
        )]
        let file_size_f64 = file_size as f64;

        #[expect(
            clippy::cast_possible_truncation,
            reason = "bitrate values are bounded by practical file sizes"
        )]
        #[expect(
            clippy::cast_sign_loss,
            reason = "result is always positive since both operands are positive"
        )]
        let calculated_bitrate = (file_size_f64 * 8.0 / duration_secs) as u64;
        Some(calculated_bitrate)
    } else {
        None
    }
}

// ============================================================================
// Container Metadata Extraction
// ============================================================================

/// Extracts container-level metadata from an `AVFormatContext`.
///
/// This function reads all metadata entries from the container's `AVDictionary`,
/// including standard keys (title, artist, album, date, etc.) and custom metadata.
///
/// # Safety
///
/// The `ctx` pointer must be valid.
///
/// # Returns
///
/// Returns a `HashMap` containing all metadata key-value pairs.
/// If no metadata is present, returns an empty `HashMap`.
unsafe fn extract_metadata(ctx: *mut ff_sys::AVFormatContext) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let dict = (*ctx).metadata;
        if dict.is_null() {
            return metadata;
        }

        // Iterate through all dictionary entries using av_dict_get with AV_DICT_IGNORE_SUFFIX
        // This iterates all entries when starting with an empty key and passing the previous entry
        let mut entry: *const ff_sys::AVDictionaryEntry = std::ptr::null();

        // AV_DICT_IGNORE_SUFFIX is a small constant (2) that safely fits in i32
        let flags = ff_sys::AV_DICT_IGNORE_SUFFIX.cast_signed();

        loop {
            // Get the next entry by passing the previous one
            // Using empty string as key and AV_DICT_IGNORE_SUFFIX to iterate all entries
            entry = ff_sys::av_dict_get(dict, c"".as_ptr(), entry, flags);

            if entry.is_null() {
                break;
            }

            // Extract key and value from the entry
            let key_ptr = (*entry).key;
            let value_ptr = (*entry).value;

            if key_ptr.is_null() || value_ptr.is_null() {
                continue;
            }

            // Convert C strings to Rust strings
            let key = CStr::from_ptr(key_ptr).to_string_lossy().into_owned();
            let value = CStr::from_ptr(value_ptr).to_string_lossy().into_owned();

            metadata.insert(key, value);
        }
    }

    metadata
}

// ============================================================================
// Video Stream Extraction
// ============================================================================

/// Extracts all video streams from an `AVFormatContext`.
///
/// This function iterates through all streams in the container and extracts
/// detailed information for each video stream.
///
/// # Safety
///
/// The `ctx` pointer must be valid and `avformat_find_stream_info` must have been called.
unsafe fn extract_video_streams(ctx: *mut ff_sys::AVFormatContext) -> Vec<VideoStreamInfo> {
    // SAFETY: Caller guarantees ctx is valid
    unsafe {
        let nb_streams = (*ctx).nb_streams;
        let streams_ptr = (*ctx).streams;

        if streams_ptr.is_null() || nb_streams == 0 {
            return Vec::new();
        }

        let mut video_streams = Vec::new();

        for i in 0..nb_streams {
            // SAFETY: i < nb_streams, so this is within bounds
            let stream = *streams_ptr.add(i as usize);
            if stream.is_null() {
                continue;
            }

            let codecpar = (*stream).codecpar;
            if codecpar.is_null() {
                continue;
            }

            // Check if this is a video stream
            if (*codecpar).codec_type != ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO {
                continue;
            }

            // Extract video stream info
            let stream_info = extract_single_video_stream(stream, codecpar, i);
            video_streams.push(stream_info);
        }

        video_streams
    }
}

/// Extracts information from a single video stream.
///
/// # Safety
///
/// Both `stream` and `codecpar` pointers must be valid.
unsafe fn extract_single_video_stream(
    stream: *mut ff_sys::AVStream,
    codecpar: *mut ff_sys::AVCodecParameters,
    index: u32,
) -> VideoStreamInfo {
    // SAFETY: Caller guarantees pointers are valid
    unsafe {
        // Extract codec info
        let codec_id = (*codecpar).codec_id;
        let codec = map_video_codec(codec_id);
        let codec_name = extract_codec_name(codec_id);

        // Extract dimensions
        #[expect(clippy::cast_sign_loss, reason = "width/height are always positive")]
        let width = (*codecpar).width as u32;
        #[expect(clippy::cast_sign_loss, reason = "width/height are always positive")]
        let height = (*codecpar).height as u32;

        // Extract pixel format
        let pixel_format = map_pixel_format((*codecpar).format);

        // Extract frame rate
        let frame_rate = extract_frame_rate(stream);

        // Extract bitrate
        let bitrate = extract_stream_bitrate(codecpar);

        // Extract color information
        let color_space = map_color_space((*codecpar).color_space);
        let color_range = map_color_range((*codecpar).color_range);
        let color_primaries = map_color_primaries((*codecpar).color_primaries);

        // Extract duration if available
        let duration = extract_stream_duration(stream);

        // Extract frame count if available
        let frame_count = extract_frame_count(stream);

        // Build the VideoStreamInfo
        let mut builder = VideoStreamInfo::builder()
            .index(index)
            .codec(codec)
            .codec_name(codec_name)
            .width(width)
            .height(height)
            .pixel_format(pixel_format)
            .frame_rate(frame_rate)
            .color_space(color_space)
            .color_range(color_range)
            .color_primaries(color_primaries);

        if let Some(d) = duration {
            builder = builder.duration(d);
        }

        if let Some(b) = bitrate {
            builder = builder.bitrate(b);
        }

        if let Some(c) = frame_count {
            builder = builder.frame_count(c);
        }

        builder.build()
    }
}

/// Extracts the codec name from an `AVCodecID`.
///
/// # Safety
///
/// This function calls `FFmpeg`'s `avcodec_get_name` which is safe for any codec ID.
unsafe fn extract_codec_name(codec_id: ff_sys::AVCodecID) -> String {
    // SAFETY: avcodec_get_name is safe for any codec ID value
    let name_ptr = unsafe { ff_sys::avcodec_get_name(codec_id) };

    if name_ptr.is_null() {
        return String::from("unknown");
    }

    // SAFETY: avcodec_get_name returns a valid C string
    unsafe { CStr::from_ptr(name_ptr).to_string_lossy().into_owned() }
}

/// Extracts the frame rate from an `AVStream`.
///
/// Tries to get the real frame rate (`r_frame_rate`), falling back to average
/// frame rate (`avg_frame_rate`), and finally to a default of 30/1.
///
/// # Safety
///
/// The `stream` pointer must be valid.
unsafe fn extract_frame_rate(stream: *mut ff_sys::AVStream) -> Rational {
    // SAFETY: Caller guarantees stream is valid
    unsafe {
        // Try r_frame_rate first (real frame rate, most accurate for video)
        let r_frame_rate = (*stream).r_frame_rate;
        if r_frame_rate.den > 0 && r_frame_rate.num > 0 {
            return Rational::new(r_frame_rate.num, r_frame_rate.den);
        }

        // Fall back to avg_frame_rate
        let avg_frame_rate = (*stream).avg_frame_rate;
        if avg_frame_rate.den > 0 && avg_frame_rate.num > 0 {
            return Rational::new(avg_frame_rate.num, avg_frame_rate.den);
        }

        // Default to 30 fps
        {
            log::warn!(
                "frame_rate unavailable, falling back to 30fps \
                 r_frame_rate={}/{} avg_frame_rate={}/{} fallback=30/1",
                r_frame_rate.num,
                r_frame_rate.den,
                avg_frame_rate.num,
                avg_frame_rate.den
            );
            Rational::new(30, 1)
        }
    }
}

/// Extracts the bitrate from an `AVCodecParameters`.
///
/// Returns `None` if the bitrate is not available or is zero.
///
/// # Safety
///
/// The `codecpar` pointer must be valid.
unsafe fn extract_stream_bitrate(codecpar: *mut ff_sys::AVCodecParameters) -> Option<u64> {
    // SAFETY: Caller guarantees codecpar is valid
    let bitrate = unsafe { (*codecpar).bit_rate };

    if bitrate > 0 {
        #[expect(clippy::cast_sign_loss, reason = "verified bitrate > 0")]
        Some(bitrate as u64)
    } else {
        None
    }
}

/// Extracts the duration from an `AVStream`.
///
/// Returns `None` if the duration is not available.
///
/// # Safety
///
/// The `stream` pointer must be valid.
unsafe fn extract_stream_duration(stream: *mut ff_sys::AVStream) -> Option<Duration> {
    // SAFETY: Caller guarantees stream is valid
    unsafe {
        let duration_pts = (*stream).duration;

        // AV_NOPTS_VALUE indicates unknown duration
        if duration_pts <= 0 {
            return None;
        }

        // Get stream time base
        let time_base = (*stream).time_base;
        if time_base.den == 0 {
            return None;
        }

        // Convert to seconds: pts * num / den
        // Note: i64 to f64 cast may lose precision for very large values,
        // but this is acceptable for media timestamps which are bounded
        #[expect(clippy::cast_precision_loss, reason = "media timestamps are bounded")]
        let secs = (duration_pts as f64) * f64::from(time_base.num) / f64::from(time_base.den);

        if secs > 0.0 {
            Some(Duration::from_secs_f64(secs))
        } else {
            None
        }
    }
}

/// Extracts the frame count from an `AVStream`.
///
/// Returns `None` if the frame count is not available.
///
/// # Safety
///
/// The `stream` pointer must be valid.
unsafe fn extract_frame_count(stream: *mut ff_sys::AVStream) -> Option<u64> {
    // SAFETY: Caller guarantees stream is valid
    let nb_frames = unsafe { (*stream).nb_frames };

    if nb_frames > 0 {
        #[expect(clippy::cast_sign_loss, reason = "verified nb_frames > 0")]
        Some(nb_frames as u64)
    } else {
        None
    }
}

// ============================================================================
// Type Mapping Functions
// ============================================================================

/// Maps an `FFmpeg` `AVCodecID` to our [`VideoCodec`] enum.
fn map_video_codec(codec_id: ff_sys::AVCodecID) -> VideoCodec {
    match codec_id {
        ff_sys::AVCodecID_AV_CODEC_ID_H264 => VideoCodec::H264,
        ff_sys::AVCodecID_AV_CODEC_ID_HEVC => VideoCodec::H265,
        ff_sys::AVCodecID_AV_CODEC_ID_VP8 => VideoCodec::Vp8,
        ff_sys::AVCodecID_AV_CODEC_ID_VP9 => VideoCodec::Vp9,
        ff_sys::AVCodecID_AV_CODEC_ID_AV1 => VideoCodec::Av1,
        ff_sys::AVCodecID_AV_CODEC_ID_PRORES => VideoCodec::ProRes,
        ff_sys::AVCodecID_AV_CODEC_ID_MPEG4 => VideoCodec::Mpeg4,
        ff_sys::AVCodecID_AV_CODEC_ID_MPEG2VIDEO => VideoCodec::Mpeg2,
        ff_sys::AVCodecID_AV_CODEC_ID_MJPEG => VideoCodec::Mjpeg,
        _ => {
            log::warn!(
                "video_codec has no mapping, using Unknown \
                 codec_id={codec_id}"
            );
            VideoCodec::Unknown
        }
    }
}

/// Maps an `FFmpeg` `AVPixelFormat` to our [`PixelFormat`] enum.
fn map_pixel_format(format: i32) -> PixelFormat {
    #[expect(clippy::cast_sign_loss, reason = "AVPixelFormat values are positive")]
    let format_u32 = format as u32;

    match format_u32 {
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24 as u32 => PixelFormat::Rgb24,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA as u32 => PixelFormat::Rgba,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24 as u32 => PixelFormat::Bgr24,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA as u32 => PixelFormat::Bgra,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P as u32 => PixelFormat::Yuv420p,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P as u32 => PixelFormat::Yuv422p,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P as u32 => PixelFormat::Yuv444p,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_NV12 as u32 => PixelFormat::Nv12,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_NV21 as u32 => PixelFormat::Nv21,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE as u32 => PixelFormat::Yuv420p10le,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_P010LE as u32 => PixelFormat::P010le,
        x if x == ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8 as u32 => PixelFormat::Gray8,
        _ => {
            log::warn!(
                "pixel_format has no mapping, using Other \
                 format={format_u32}"
            );
            PixelFormat::Other(format_u32)
        }
    }
}

/// Maps an `FFmpeg` `AVColorSpace` to our [`ColorSpace`] enum.
fn map_color_space(color_space: ff_sys::AVColorSpace) -> ColorSpace {
    match color_space {
        ff_sys::AVColorSpace_AVCOL_SPC_BT709 => ColorSpace::Bt709,
        ff_sys::AVColorSpace_AVCOL_SPC_BT470BG | ff_sys::AVColorSpace_AVCOL_SPC_SMPTE170M => {
            ColorSpace::Bt601
        }
        ff_sys::AVColorSpace_AVCOL_SPC_BT2020_NCL | ff_sys::AVColorSpace_AVCOL_SPC_BT2020_CL => {
            ColorSpace::Bt2020
        }
        ff_sys::AVColorSpace_AVCOL_SPC_RGB => ColorSpace::Srgb,
        _ => {
            log::warn!(
                "color_space has no mapping, using Unknown \
                 color_space={color_space}"
            );
            ColorSpace::Unknown
        }
    }
}

/// Maps an `FFmpeg` `AVColorRange` to our [`ColorRange`] enum.
fn map_color_range(color_range: ff_sys::AVColorRange) -> ColorRange {
    match color_range {
        ff_sys::AVColorRange_AVCOL_RANGE_MPEG => ColorRange::Limited,
        ff_sys::AVColorRange_AVCOL_RANGE_JPEG => ColorRange::Full,
        _ => {
            log::warn!(
                "color_range has no mapping, using Unknown \
                 color_range={color_range}"
            );
            ColorRange::Unknown
        }
    }
}

/// Maps an `FFmpeg` `AVColorPrimaries` to our [`ColorPrimaries`] enum.
fn map_color_primaries(color_primaries: ff_sys::AVColorPrimaries) -> ColorPrimaries {
    match color_primaries {
        ff_sys::AVColorPrimaries_AVCOL_PRI_BT709 => ColorPrimaries::Bt709,
        ff_sys::AVColorPrimaries_AVCOL_PRI_BT470BG
        | ff_sys::AVColorPrimaries_AVCOL_PRI_SMPTE170M => ColorPrimaries::Bt601,
        ff_sys::AVColorPrimaries_AVCOL_PRI_BT2020 => ColorPrimaries::Bt2020,
        _ => {
            log::warn!(
                "color_primaries has no mapping, using Unknown \
                 color_primaries={color_primaries}"
            );
            ColorPrimaries::Unknown
        }
    }
}

// ============================================================================
// Audio Stream Extraction
// ============================================================================

/// Extracts all audio streams from an `AVFormatContext`.
///
/// This function iterates through all streams in the container and extracts
/// detailed information for each audio stream.
///
/// # Safety
///
/// The `ctx` pointer must be valid and `avformat_find_stream_info` must have been called.
unsafe fn extract_audio_streams(ctx: *mut ff_sys::AVFormatContext) -> Vec<AudioStreamInfo> {
    // SAFETY: Caller guarantees ctx is valid and find_stream_info was called
    unsafe {
        let nb_streams = (*ctx).nb_streams;
        let streams_ptr = (*ctx).streams;

        if streams_ptr.is_null() || nb_streams == 0 {
            return Vec::new();
        }

        let mut audio_streams = Vec::new();

        for i in 0..nb_streams {
            // SAFETY: i < nb_streams, so this is within bounds
            let stream = *streams_ptr.add(i as usize);
            if stream.is_null() {
                continue;
            }

            let codecpar = (*stream).codecpar;
            if codecpar.is_null() {
                continue;
            }

            // Check if this is an audio stream
            if (*codecpar).codec_type != ff_sys::AVMediaType_AVMEDIA_TYPE_AUDIO {
                continue;
            }

            // Extract audio stream info
            let stream_info = extract_single_audio_stream(stream, codecpar, i);
            audio_streams.push(stream_info);
        }

        audio_streams
    }
}

/// Extracts information from a single audio stream.
///
/// # Safety
///
/// Both `stream` and `codecpar` pointers must be valid.
unsafe fn extract_single_audio_stream(
    stream: *mut ff_sys::AVStream,
    codecpar: *mut ff_sys::AVCodecParameters,
    index: u32,
) -> AudioStreamInfo {
    // SAFETY: Caller guarantees pointers are valid
    unsafe {
        // Extract codec info
        let codec_id = (*codecpar).codec_id;
        let codec = map_audio_codec(codec_id);
        let codec_name = extract_codec_name(codec_id);

        // Extract audio parameters
        #[expect(clippy::cast_sign_loss, reason = "sample_rate is always positive")]
        let sample_rate = (*codecpar).sample_rate as u32;

        // FFmpeg 5.1+ uses ch_layout, older versions use channels
        let channels = extract_channel_count(codecpar);

        // Extract channel layout
        let channel_layout = extract_channel_layout(codecpar, channels);

        // Extract sample format
        let sample_format = map_sample_format((*codecpar).format);

        // Extract bitrate
        let bitrate = extract_stream_bitrate(codecpar);

        // Extract duration if available
        let duration = extract_stream_duration(stream);

        // Extract language from stream metadata
        let language = extract_language(stream);

        // Build the AudioStreamInfo
        let mut builder = AudioStreamInfo::builder()
            .index(index)
            .codec(codec)
            .codec_name(codec_name)
            .sample_rate(sample_rate)
            .channels(channels)
            .channel_layout(channel_layout)
            .sample_format(sample_format);

        if let Some(d) = duration {
            builder = builder.duration(d);
        }

        if let Some(b) = bitrate {
            builder = builder.bitrate(b);
        }

        if let Some(lang) = language {
            builder = builder.language(lang);
        }

        builder.build()
    }
}

/// Extracts the channel count from `AVCodecParameters`.
///
/// `FFmpeg` 5.1+ uses `ch_layout.nb_channels`, older versions used `channels` directly.
///
/// Returns the actual channel count from `FFmpeg`. If the channel count is 0 (which
/// indicates uninitialized or unknown), returns 1 (mono) as a safe minimum.
///
/// # Safety
///
/// The `codecpar` pointer must be valid.
unsafe fn extract_channel_count(codecpar: *mut ff_sys::AVCodecParameters) -> u32 {
    // SAFETY: Caller guarantees codecpar is valid
    // FFmpeg 5.1+ uses ch_layout structure
    #[expect(clippy::cast_sign_loss, reason = "channel count is always positive")]
    let channels = unsafe { (*codecpar).ch_layout.nb_channels as u32 };

    // If channel count is 0 (uninitialized/unknown), use 1 (mono) as safe minimum
    if channels > 0 {
        channels
    } else {
        log::warn!(
            "channel_count is 0 (uninitialized), falling back to mono \
             fallback=1"
        );
        1
    }
}

/// Extracts the channel layout from `AVCodecParameters`.
///
/// # Safety
///
/// The `codecpar` pointer must be valid.
unsafe fn extract_channel_layout(
    codecpar: *mut ff_sys::AVCodecParameters,
    channels: u32,
) -> ChannelLayout {
    // SAFETY: Caller guarantees codecpar is valid
    // FFmpeg 5.1+ uses ch_layout structure with channel masks
    let ch_layout = unsafe { &(*codecpar).ch_layout };

    // Check if we have a specific channel layout mask
    // AV_CHANNEL_ORDER_NATIVE means we have a valid channel mask
    if ch_layout.order == ff_sys::AVChannelOrder_AV_CHANNEL_ORDER_NATIVE {
        // Map common FFmpeg channel masks to our ChannelLayout
        // These are AVChannelLayout masks for standard configurations
        // SAFETY: When order is AV_CHANNEL_ORDER_NATIVE, the mask field is valid
        let mask = unsafe { ch_layout.u.mask };
        match mask {
            // AV_CH_LAYOUT_MONO = 0x4 (front center)
            0x4 => ChannelLayout::Mono,
            // AV_CH_LAYOUT_STEREO = 0x3 (front left + front right)
            0x3 => ChannelLayout::Stereo,
            // AV_CH_LAYOUT_2_1 = 0x103 (stereo + LFE)
            0x103 => ChannelLayout::Stereo2_1,
            // AV_CH_LAYOUT_SURROUND = 0x7 (FL + FR + FC)
            0x7 => ChannelLayout::Surround3_0,
            // AV_CH_LAYOUT_QUAD = 0x33 (FL + FR + BL + BR)
            0x33 => ChannelLayout::Quad,
            // AV_CH_LAYOUT_5POINT0 = 0x37 (FL + FR + FC + BL + BR)
            0x37 => ChannelLayout::Surround5_0,
            // AV_CH_LAYOUT_5POINT1 = 0x3F (FL + FR + FC + LFE + BL + BR)
            0x3F => ChannelLayout::Surround5_1,
            // AV_CH_LAYOUT_6POINT1 = 0x13F (FL + FR + FC + LFE + BC + SL + SR)
            0x13F => ChannelLayout::Surround6_1,
            // AV_CH_LAYOUT_7POINT1 = 0x63F (FL + FR + FC + LFE + BL + BR + SL + SR)
            0x63F => ChannelLayout::Surround7_1,
            _ => {
                log::warn!(
                    "channel_layout mask has no mapping, deriving from channel count \
                     mask={mask} channels={channels}"
                );
                ChannelLayout::from_channels(channels)
            }
        }
    } else {
        log::warn!(
            "channel_layout order is not NATIVE, deriving from channel count \
             order={order} channels={channels}",
            order = ch_layout.order
        );
        ChannelLayout::from_channels(channels)
    }
}

/// Extracts the language tag from stream metadata.
///
/// # Safety
///
/// The `stream` pointer must be valid.
unsafe fn extract_language(stream: *mut ff_sys::AVStream) -> Option<String> {
    // SAFETY: Caller guarantees stream is valid
    unsafe {
        let metadata = (*stream).metadata;
        if metadata.is_null() {
            return None;
        }

        // Look for "language" tag in the stream metadata
        let key = c"language";
        let entry = ff_sys::av_dict_get(metadata, key.as_ptr(), std::ptr::null(), 0);

        if entry.is_null() {
            return None;
        }

        let value_ptr = (*entry).value;
        if value_ptr.is_null() {
            return None;
        }

        Some(CStr::from_ptr(value_ptr).to_string_lossy().into_owned())
    }
}

// ============================================================================
// Audio Type Mapping Functions
// ============================================================================

/// Maps an `FFmpeg` `AVCodecID` to our [`AudioCodec`] enum.
fn map_audio_codec(codec_id: ff_sys::AVCodecID) -> AudioCodec {
    match codec_id {
        ff_sys::AVCodecID_AV_CODEC_ID_AAC => AudioCodec::Aac,
        ff_sys::AVCodecID_AV_CODEC_ID_MP3 => AudioCodec::Mp3,
        ff_sys::AVCodecID_AV_CODEC_ID_OPUS => AudioCodec::Opus,
        ff_sys::AVCodecID_AV_CODEC_ID_FLAC => AudioCodec::Flac,
        ff_sys::AVCodecID_AV_CODEC_ID_VORBIS => AudioCodec::Vorbis,
        ff_sys::AVCodecID_AV_CODEC_ID_AC3 => AudioCodec::Ac3,
        ff_sys::AVCodecID_AV_CODEC_ID_EAC3 => AudioCodec::Eac3,
        ff_sys::AVCodecID_AV_CODEC_ID_DTS => AudioCodec::Dts,
        ff_sys::AVCodecID_AV_CODEC_ID_ALAC => AudioCodec::Alac,
        // PCM variants
        ff_sys::AVCodecID_AV_CODEC_ID_PCM_S16LE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_S16BE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_S24LE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_S24BE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_S32LE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_S32BE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_F32LE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_F32BE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_F64LE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_F64BE
        | ff_sys::AVCodecID_AV_CODEC_ID_PCM_U8 => AudioCodec::Pcm,
        _ => {
            log::warn!(
                "audio_codec has no mapping, using Unknown \
                 codec_id={codec_id}"
            );
            AudioCodec::Unknown
        }
    }
}

/// Maps an `FFmpeg` `AVSampleFormat` to our [`SampleFormat`] enum.
fn map_sample_format(format: i32) -> SampleFormat {
    #[expect(clippy::cast_sign_loss, reason = "AVSampleFormat values are positive")]
    let format_u32 = format as u32;

    match format_u32 {
        // Packed (interleaved) formats
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8 as u32 => SampleFormat::U8,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16 as u32 => SampleFormat::I16,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32 as u32 => SampleFormat::I32,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLT as u32 => SampleFormat::F32,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBL as u32 => SampleFormat::F64,
        // Planar formats
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8P as u32 => SampleFormat::U8p,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16P as u32 => SampleFormat::I16p,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32P as u32 => SampleFormat::I32p,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP as u32 => SampleFormat::F32p,
        x if x == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBLP as u32 => SampleFormat::F64p,
        // Unknown format
        _ => {
            log::warn!(
                "sample_format has no mapping, using Other \
                 format={format_u32}"
            );
            SampleFormat::Other(format_u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_nonexistent_file() {
        let result = open("/nonexistent/path/to/video.mp4");
        assert!(result.is_err());
        match result {
            Err(ProbeError::FileNotFound { path }) => {
                assert!(path.to_string_lossy().contains("video.mp4"));
            }
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_open_invalid_file() {
        // Create a temporary file with invalid content
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("ff_probe_test_invalid.mp4");
        std::fs::write(&temp_file, b"not a valid video file").ok();

        let result = open(&temp_file);

        // Clean up
        std::fs::remove_file(&temp_file).ok();

        // FFmpeg should fail to open this as a valid media file
        assert!(result.is_err());
        match result {
            Err(ProbeError::CannotOpen { .. }) | Err(ProbeError::InvalidMedia { .. }) => {}
            _ => panic!("Expected CannotOpen or InvalidMedia error"),
        }
    }

    #[test]
    fn test_av_time_base_constant() {
        // Verify our constant matches the expected value
        assert_eq!(AV_TIME_BASE, 1_000_000);
    }

    #[test]
    fn test_duration_conversion() {
        // Test duration calculation logic
        let duration_us: i64 = 5_500_000; // 5.5 seconds
        let secs = (duration_us / AV_TIME_BASE) as u64;
        let micros = (duration_us % AV_TIME_BASE) as u32;
        let duration = Duration::new(secs, micros * 1000);

        assert_eq!(duration.as_secs(), 5);
        assert_eq!(duration.subsec_micros(), 500_000);
    }

    // ========================================================================
    // Video Codec Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_video_codec_h264() {
        let codec = map_video_codec(ff_sys::AVCodecID_AV_CODEC_ID_H264);
        assert_eq!(codec, VideoCodec::H264);
    }

    #[test]
    fn test_map_video_codec_hevc() {
        let codec = map_video_codec(ff_sys::AVCodecID_AV_CODEC_ID_HEVC);
        assert_eq!(codec, VideoCodec::H265);
    }

    #[test]
    fn test_map_video_codec_vp9() {
        let codec = map_video_codec(ff_sys::AVCodecID_AV_CODEC_ID_VP9);
        assert_eq!(codec, VideoCodec::Vp9);
    }

    #[test]
    fn test_map_video_codec_av1() {
        let codec = map_video_codec(ff_sys::AVCodecID_AV_CODEC_ID_AV1);
        assert_eq!(codec, VideoCodec::Av1);
    }

    #[test]
    fn test_map_video_codec_unknown() {
        // Use a codec ID that's not explicitly mapped
        let codec = map_video_codec(ff_sys::AVCodecID_AV_CODEC_ID_THEORA);
        assert_eq!(codec, VideoCodec::Unknown);
    }

    // ========================================================================
    // Pixel Format Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_pixel_format_yuv420p() {
        let format = map_pixel_format(ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P as i32);
        assert_eq!(format, PixelFormat::Yuv420p);
    }

    #[test]
    fn test_map_pixel_format_rgba() {
        let format = map_pixel_format(ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA as i32);
        assert_eq!(format, PixelFormat::Rgba);
    }

    #[test]
    fn test_map_pixel_format_nv12() {
        let format = map_pixel_format(ff_sys::AVPixelFormat_AV_PIX_FMT_NV12 as i32);
        assert_eq!(format, PixelFormat::Nv12);
    }

    #[test]
    fn test_map_pixel_format_yuv420p10le() {
        let format = map_pixel_format(ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE as i32);
        assert_eq!(format, PixelFormat::Yuv420p10le);
    }

    #[test]
    fn test_map_pixel_format_unknown() {
        // Use a pixel format that's not explicitly mapped
        let format = map_pixel_format(ff_sys::AVPixelFormat_AV_PIX_FMT_PAL8 as i32);
        assert!(matches!(format, PixelFormat::Other(_)));
    }

    // ========================================================================
    // Color Space Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_color_space_bt709() {
        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_BT709);
        assert_eq!(space, ColorSpace::Bt709);
    }

    #[test]
    fn test_map_color_space_bt601() {
        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_BT470BG);
        assert_eq!(space, ColorSpace::Bt601);

        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_SMPTE170M);
        assert_eq!(space, ColorSpace::Bt601);
    }

    #[test]
    fn test_map_color_space_bt2020() {
        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_BT2020_NCL);
        assert_eq!(space, ColorSpace::Bt2020);

        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_BT2020_CL);
        assert_eq!(space, ColorSpace::Bt2020);
    }

    #[test]
    fn test_map_color_space_srgb() {
        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_RGB);
        assert_eq!(space, ColorSpace::Srgb);
    }

    #[test]
    fn test_map_color_space_unknown() {
        let space = map_color_space(ff_sys::AVColorSpace_AVCOL_SPC_UNSPECIFIED);
        assert_eq!(space, ColorSpace::Unknown);
    }

    // ========================================================================
    // Color Range Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_color_range_limited() {
        let range = map_color_range(ff_sys::AVColorRange_AVCOL_RANGE_MPEG);
        assert_eq!(range, ColorRange::Limited);
    }

    #[test]
    fn test_map_color_range_full() {
        let range = map_color_range(ff_sys::AVColorRange_AVCOL_RANGE_JPEG);
        assert_eq!(range, ColorRange::Full);
    }

    #[test]
    fn test_map_color_range_unknown() {
        let range = map_color_range(ff_sys::AVColorRange_AVCOL_RANGE_UNSPECIFIED);
        assert_eq!(range, ColorRange::Unknown);
    }

    // ========================================================================
    // Color Primaries Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_color_primaries_bt709() {
        let primaries = map_color_primaries(ff_sys::AVColorPrimaries_AVCOL_PRI_BT709);
        assert_eq!(primaries, ColorPrimaries::Bt709);
    }

    #[test]
    fn test_map_color_primaries_bt601() {
        let primaries = map_color_primaries(ff_sys::AVColorPrimaries_AVCOL_PRI_BT470BG);
        assert_eq!(primaries, ColorPrimaries::Bt601);

        let primaries = map_color_primaries(ff_sys::AVColorPrimaries_AVCOL_PRI_SMPTE170M);
        assert_eq!(primaries, ColorPrimaries::Bt601);
    }

    #[test]
    fn test_map_color_primaries_bt2020() {
        let primaries = map_color_primaries(ff_sys::AVColorPrimaries_AVCOL_PRI_BT2020);
        assert_eq!(primaries, ColorPrimaries::Bt2020);
    }

    #[test]
    fn test_map_color_primaries_unknown() {
        let primaries = map_color_primaries(ff_sys::AVColorPrimaries_AVCOL_PRI_UNSPECIFIED);
        assert_eq!(primaries, ColorPrimaries::Unknown);
    }

    // ========================================================================
    // Audio Codec Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_audio_codec_aac() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_AAC);
        assert_eq!(codec, AudioCodec::Aac);
    }

    #[test]
    fn test_map_audio_codec_mp3() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_MP3);
        assert_eq!(codec, AudioCodec::Mp3);
    }

    #[test]
    fn test_map_audio_codec_opus() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_OPUS);
        assert_eq!(codec, AudioCodec::Opus);
    }

    #[test]
    fn test_map_audio_codec_flac() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_FLAC);
        assert_eq!(codec, AudioCodec::Flac);
    }

    #[test]
    fn test_map_audio_codec_vorbis() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_VORBIS);
        assert_eq!(codec, AudioCodec::Vorbis);
    }

    #[test]
    fn test_map_audio_codec_ac3() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_AC3);
        assert_eq!(codec, AudioCodec::Ac3);
    }

    #[test]
    fn test_map_audio_codec_eac3() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_EAC3);
        assert_eq!(codec, AudioCodec::Eac3);
    }

    #[test]
    fn test_map_audio_codec_dts() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_DTS);
        assert_eq!(codec, AudioCodec::Dts);
    }

    #[test]
    fn test_map_audio_codec_alac() {
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_ALAC);
        assert_eq!(codec, AudioCodec::Alac);
    }

    #[test]
    fn test_map_audio_codec_pcm() {
        // Test various PCM formats
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_PCM_S16LE);
        assert_eq!(codec, AudioCodec::Pcm);

        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_PCM_F32LE);
        assert_eq!(codec, AudioCodec::Pcm);

        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_PCM_U8);
        assert_eq!(codec, AudioCodec::Pcm);
    }

    #[test]
    fn test_map_audio_codec_unknown() {
        // Use a codec ID that's not explicitly mapped
        let codec = map_audio_codec(ff_sys::AVCodecID_AV_CODEC_ID_WMAV2);
        assert_eq!(codec, AudioCodec::Unknown);
    }

    // ========================================================================
    // Sample Format Mapping Tests
    // ========================================================================

    #[test]
    fn test_map_sample_format_u8() {
        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8 as i32);
        assert_eq!(format, SampleFormat::U8);
    }

    #[test]
    fn test_map_sample_format_i16() {
        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16 as i32);
        assert_eq!(format, SampleFormat::I16);
    }

    #[test]
    fn test_map_sample_format_i32() {
        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32 as i32);
        assert_eq!(format, SampleFormat::I32);
    }

    #[test]
    fn test_map_sample_format_f32() {
        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLT as i32);
        assert_eq!(format, SampleFormat::F32);
    }

    #[test]
    fn test_map_sample_format_f64() {
        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBL as i32);
        assert_eq!(format, SampleFormat::F64);
    }

    #[test]
    fn test_map_sample_format_planar() {
        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8P as i32);
        assert_eq!(format, SampleFormat::U8p);

        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16P as i32);
        assert_eq!(format, SampleFormat::I16p);

        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32P as i32);
        assert_eq!(format, SampleFormat::I32p);

        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP as i32);
        assert_eq!(format, SampleFormat::F32p);

        let format = map_sample_format(ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBLP as i32);
        assert_eq!(format, SampleFormat::F64p);
    }

    #[test]
    fn test_map_sample_format_unknown() {
        // Use a format value that's not explicitly mapped
        let format = map_sample_format(999);
        assert!(matches!(format, SampleFormat::Other(_)));
    }

    // ========================================================================
    // Bitrate Calculation Tests
    // ========================================================================

    #[test]
    fn test_bitrate_fallback_calculation() {
        // Test the fallback bitrate calculation logic:
        // bitrate = file_size (bytes) * 8 (bits/byte) / duration (seconds)
        //
        // Example: 10 MB file, 10 second duration
        // Expected: 10_000_000 bytes * 8 / 10 seconds = 8_000_000 bps
        let file_size: u64 = 10_000_000;
        let duration = Duration::from_secs(10);
        let duration_secs = duration.as_secs_f64();

        let calculated_bitrate = (file_size as f64 * 8.0 / duration_secs) as u64;
        assert_eq!(calculated_bitrate, 8_000_000);
    }

    #[test]
    fn test_bitrate_fallback_with_subsecond_duration() {
        // Test with sub-second duration
        // 1 MB file, 0.5 second duration
        // Expected: 1_000_000 * 8 / 0.5 = 16_000_000 bps
        let file_size: u64 = 1_000_000;
        let duration = Duration::from_millis(500);
        let duration_secs = duration.as_secs_f64();

        let calculated_bitrate = (file_size as f64 * 8.0 / duration_secs) as u64;
        assert_eq!(calculated_bitrate, 16_000_000);
    }

    #[test]
    fn test_bitrate_zero_duration() {
        // When duration is zero, we cannot calculate bitrate
        let duration = Duration::ZERO;
        let duration_secs = duration.as_secs_f64();

        // Should not divide when duration is zero
        assert!(duration_secs == 0.0);
    }

    #[test]
    fn test_bitrate_zero_file_size() {
        // When file size is zero, bitrate should also be zero
        let file_size: u64 = 0;
        let duration = Duration::from_secs(10);
        let duration_secs = duration.as_secs_f64();

        if duration_secs > 0.0 && file_size > 0 {
            let calculated_bitrate = (file_size as f64 * 8.0 / duration_secs) as u64;
            assert_eq!(calculated_bitrate, 0);
        } else {
            // file_size is 0, so we should not have calculated a bitrate
            assert_eq!(file_size, 0);
        }
    }

    #[test]
    fn test_bitrate_typical_video_file() {
        // Test with typical video file parameters:
        // 100 MB file, 5 minute duration
        // Expected: 100_000_000 * 8 / 300 = 2_666_666 bps (~2.67 Mbps)
        let file_size: u64 = 100_000_000;
        let duration = Duration::from_secs(300); // 5 minutes
        let duration_secs = duration.as_secs_f64();

        let calculated_bitrate = (file_size as f64 * 8.0 / duration_secs) as u64;
        assert_eq!(calculated_bitrate, 2_666_666);
    }

    #[test]
    fn test_bitrate_high_quality_video() {
        // Test with high-quality video parameters:
        // 5 GB file, 2 hour duration
        // Expected: 5_000_000_000 * 8 / 7200 = 5_555_555 bps (~5.6 Mbps)
        let file_size: u64 = 5_000_000_000;
        let duration = Duration::from_secs(7200); // 2 hours
        let duration_secs = duration.as_secs_f64();

        let calculated_bitrate = (file_size as f64 * 8.0 / duration_secs) as u64;
        assert_eq!(calculated_bitrate, 5_555_555);
    }
}
