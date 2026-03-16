//! Error types for streaming operations.
//!
//! This module provides the [`StreamError`] enum which represents all
//! possible errors that can occur during HLS / DASH output and ABR ladder
//! generation.

/// Errors that can occur during streaming output operations.
///
/// This enum covers all error conditions that may arise when configuring,
/// building, or writing HLS / DASH output.
///
/// # Error Categories
///
/// - **Encoding errors**: [`Encode`](Self::Encode) — wraps errors from `ff-encode`
/// - **I/O errors**: [`Io`](Self::Io) — file system errors during segment writing
/// - **Configuration errors**: [`InvalidConfig`](Self::InvalidConfig) — missing or
///   invalid builder options, or not-yet-implemented stubs
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    /// An encoding operation in the underlying `ff-encode` crate failed.
    ///
    /// This error propagates from [`ff_encode::EncodeError`] when the encoder
    /// cannot open a codec or write frames.
    #[error("encode failed: {0}")]
    Encode(#[from] ff_encode::EncodeError),

    /// An I/O operation failed during segment or playlist writing.
    ///
    /// Typical causes include missing output directories, permission errors,
    /// or a full disk.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A configuration value is missing or invalid, or the feature is not yet
    /// implemented.
    ///
    /// This variant is also used as a stub return value for `write()` / `hls()`
    /// / `dash()` methods that await `FFmpeg` muxing integration.
    #[error("invalid config: {reason}")]
    InvalidConfig {
        /// Human-readable description of the configuration problem.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_config_should_display_reason() {
        let err = StreamError::InvalidConfig {
            reason: "missing input path".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("missing input path"), "got: {msg}");
    }

    #[test]
    fn io_error_should_convert_via_from() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let err: StreamError = io.into();
        assert!(matches!(err, StreamError::Io(_)));
    }

    #[test]
    fn encode_error_should_convert_via_from() {
        let enc = ff_encode::EncodeError::Cancelled;
        let err: StreamError = enc.into();
        assert!(matches!(err, StreamError::Encode(_)));
    }

    #[test]
    fn display_io_should_contain_message() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: StreamError = io.into();
        assert!(err.to_string().contains("access denied"), "got: {err}");
    }
}
