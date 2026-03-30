//! Core format-level extraction: format name, duration, and container bitrate.

use std::ffi::CStr;
use std::time::Duration;

/// `AV_TIME_BASE` constant from `FFmpeg` (microseconds per second).
const AV_TIME_BASE: i64 = 1_000_000;

/// Extracts format information from an `AVFormatContext`.
///
/// # Safety
///
/// The `ctx` pointer must be valid and properly initialized by `avformat_open_input`.
pub(super) unsafe fn extract_format_info(
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
pub(super) unsafe fn calculate_container_bitrate(
    ctx: *mut ff_sys::AVFormatContext,
    file_size: u64,
    duration: std::time::Duration,
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
