//! Video stream extraction.

use std::ffi::CStr;
use std::time::Duration;

use ff_format::Rational;
use ff_format::stream::VideoStreamInfo;

use super::mapping::{
    map_color_primaries, map_color_range, map_color_space, map_pixel_format, map_video_codec,
};

/// Extracts all video streams from an `AVFormatContext`.
///
/// This function iterates through all streams in the container and extracts
/// detailed information for each video stream.
///
/// # Safety
///
/// The `ctx` pointer must be valid and `avformat_find_stream_info` must have been called.
pub(super) unsafe fn extract_video_streams(
    ctx: *mut ff_sys::AVFormatContext,
) -> Vec<VideoStreamInfo> {
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
pub(super) unsafe fn extract_codec_name(codec_id: ff_sys::AVCodecID) -> String {
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
pub(super) unsafe fn extract_stream_bitrate(
    codecpar: *mut ff_sys::AVCodecParameters,
) -> Option<u64> {
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
pub(super) unsafe fn extract_stream_duration(stream: *mut ff_sys::AVStream) -> Option<Duration> {
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
