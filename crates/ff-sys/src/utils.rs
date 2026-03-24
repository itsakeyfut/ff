//! FFmpeg initialization and error conversion utilities.

use std::ffi::CStr;
use std::sync::Once;

/// FFmpeg initialization guard.
/// Ensures FFmpeg is initialized exactly once.
static INIT: Once = Once::new();

/// Ensure FFmpeg is initialized.
///
/// This function is idempotent and can be called multiple times safely.
/// It will only perform initialization once.
pub fn ensure_initialized() {
    INIT.call_once(|| {
        // FFmpeg 4.0+ deprecated av_register_all() and it was removed in later versions.
        // Modern FFmpeg automatically registers codecs/formats at startup.
        // This function is kept for potential future initialization needs
        // (e.g., logging configuration, thread settings).
    });
}

/// Convert an FFmpeg error code to a human-readable string.
///
/// # Arguments
///
/// * `errnum` - The FFmpeg error code (negative value)
///
/// # Returns
///
/// A string describing the error.
///
/// # Safety
///
/// This function calls FFmpeg's `av_strerror` which is thread-safe.
pub fn av_error_string(errnum: i32) -> String {
    const BUF_SIZE: usize = 256;
    let mut buf = [0i8; BUF_SIZE];

    // SAFETY: av_strerror writes to the buffer and is thread-safe
    unsafe {
        crate::av_strerror(errnum, buf.as_mut_ptr(), BUF_SIZE);
    }

    // SAFETY: av_strerror null-terminates the buffer
    let c_str = unsafe { CStr::from_ptr(buf.as_ptr()) };
    c_str.to_string_lossy().into_owned()
}

/// Macro to check FFmpeg return values and convert to `Result`.
///
/// # Example
///
/// ```ignore
/// let result = check_av_error!(avformat_open_input(...));
/// ```
#[macro_export]
macro_rules! check_av_error {
    ($expr:expr) => {{
        let ret = $expr;
        if ret < 0 {
            Err($crate::av_error_string(ret))
        } else {
            Ok(ret)
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_codes;

    #[test]
    fn ensure_initialized_should_be_idempotent() {
        // Should not panic when called multiple times
        ensure_initialized();
        ensure_initialized();
        ensure_initialized();
    }

    #[test]
    fn av_error_string_should_return_non_empty_message() {
        let error_str = av_error_string(error_codes::ENOMEM);
        assert!(!error_str.is_empty());
    }
}
