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

    /// The requested codec is not supported by the output format.
    ///
    /// For example, RTMP/FLV requires H.264 video and AAC audio; requesting
    /// any other codec returns this error from `build()`.
    #[error("unsupported codec: {codec} — {reason}")]
    UnsupportedCodec {
        /// Name of the codec that was rejected.
        codec: String,
        /// Human-readable explanation of the constraint.
        reason: String,
    },

    /// An `FFmpeg` runtime error occurred during muxing or transcoding.
    ///
    /// `code` is the raw `FFmpeg` negative error value returned by the failing
    /// function (e.g. `AVERROR(EINVAL)`).  `message` is the human-readable
    /// string produced by `av_strerror`.  Exposing the numeric code lets
    /// engineers cross-reference `FFmpeg` documentation and source directly.
    #[error("ffmpeg error: {message} (code={code})")]
    Ffmpeg {
        /// Raw `FFmpeg` error code (negative integer).
        code: i32,
        /// Human-readable description of the `FFmpeg` error.
        message: String,
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

    #[test]
    fn unsupported_codec_should_display_codec_and_reason() {
        let err = StreamError::UnsupportedCodec {
            codec: "Vp9".into(),
            reason: "RTMP/FLV requires H.264 video".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Vp9"), "got: {msg}");
        assert!(msg.contains("H.264"), "got: {msg}");
    }

    #[test]
    fn ffmpeg_error_should_display_code_and_message() {
        let err = StreamError::Ffmpeg {
            code: -22,
            message: "Cannot open codec".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Cannot open codec"), "got: {msg}");
        assert!(msg.contains("code=-22"), "got: {msg}");
    }
}
