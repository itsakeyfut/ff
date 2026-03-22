//! Error types for decoding operations.
//!
//! This module provides the [`DecodeError`] enum which represents all
//! possible errors that can occur during video/audio decoding.

use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

use crate::HardwareAccel;

/// Errors that can occur during decoding operations.
///
/// This enum covers all error conditions that may arise when opening,
/// configuring, or decoding media files.
///
/// # Error Categories
///
/// - **File errors**: [`FileNotFound`](Self::FileNotFound)
/// - **Stream errors**: [`NoVideoStream`](Self::NoVideoStream), [`NoAudioStream`](Self::NoAudioStream)
/// - **Codec errors**: [`UnsupportedCodec`](Self::UnsupportedCodec)
/// - **Runtime errors**: [`DecodingFailed`](Self::DecodingFailed), [`SeekFailed`](Self::SeekFailed)
/// - **Hardware errors**: [`HwAccelUnavailable`](Self::HwAccelUnavailable)
/// - **Configuration errors**: [`InvalidOutputDimensions`](Self::InvalidOutputDimensions)
/// - **Internal errors**: [`Ffmpeg`](Self::Ffmpeg), [`Io`](Self::Io)
#[derive(Error, Debug)]
pub enum DecodeError {
    /// File was not found at the specified path.
    ///
    /// This error occurs when attempting to open a file that doesn't exist.
    #[error("File not found: {path}")]
    FileNotFound {
        /// Path that was not found.
        path: PathBuf,
    },

    /// No video stream exists in the media file.
    ///
    /// This error occurs when trying to decode video from a file that
    /// only contains audio or other non-video streams.
    #[error("No video stream found in: {path}")]
    NoVideoStream {
        /// Path to the media file.
        path: PathBuf,
    },

    /// No audio stream exists in the media file.
    ///
    /// This error occurs when trying to decode audio from a file that
    /// only contains video or other non-audio streams.
    #[error("No audio stream found in: {path}")]
    NoAudioStream {
        /// Path to the media file.
        path: PathBuf,
    },

    /// The codec is not supported by this decoder.
    ///
    /// This may occur for uncommon or proprietary codecs that are not
    /// included in the `FFmpeg` build.
    #[error("Codec not supported: {codec}")]
    UnsupportedCodec {
        /// Name of the unsupported codec.
        codec: String,
    },

    /// The decoder for a known codec is absent from this `FFmpeg` build.
    ///
    /// Unlike [`UnsupportedCodec`](Self::UnsupportedCodec), the codec ID is
    /// recognised by `FFmpeg` but the decoder was not compiled in (e.g.
    /// `--enable-decoder=exr` was omitted from the build).
    #[error("Decoder unavailable: {codec} — {hint}")]
    DecoderUnavailable {
        /// Short name of the codec (e.g. `"exr"`).
        codec: String,
        /// Human-readable suggestion for the caller.
        hint: String,
    },

    /// Decoding operation failed at a specific point.
    ///
    /// This can occur due to corrupted data, unexpected stream format,
    /// or internal decoder errors.
    #[error("Decoding failed at {timestamp:?}: {reason}")]
    DecodingFailed {
        /// Timestamp where decoding failed (if known).
        timestamp: Option<Duration>,
        /// Reason for the failure.
        reason: String,
    },

    /// Seek operation failed.
    ///
    /// Seeking may fail for various reasons including corrupted index,
    /// seeking beyond file bounds, or stream format limitations.
    #[error("Seek failed to {target:?}: {reason}")]
    SeekFailed {
        /// Target position of the seek.
        target: Duration,
        /// Reason for the failure.
        reason: String,
    },

    /// Requested hardware acceleration is not available.
    ///
    /// This error occurs when a specific hardware accelerator is requested
    /// but the system doesn't support it. Consider using [`HardwareAccel::Auto`]
    /// for automatic fallback.
    #[error("Hardware acceleration unavailable: {accel:?}")]
    HwAccelUnavailable {
        /// The unavailable hardware acceleration type.
        accel: HardwareAccel,
    },

    /// Output dimensions are invalid.
    ///
    /// Width and height passed to [`output_size`](crate::video::builder::VideoDecoderBuilder::output_size),
    /// [`output_width`](crate::video::builder::VideoDecoderBuilder::output_width), or
    /// [`output_height`](crate::video::builder::VideoDecoderBuilder::output_height) must be
    /// greater than zero and even (required by most pixel formats).
    #[error("Invalid output dimensions: {width}x{height} (must be > 0 and even)")]
    InvalidOutputDimensions {
        /// Requested output width.
        width: u32,
        /// Requested output height.
        height: u32,
    },

    /// `FFmpeg` internal error.
    ///
    /// This wraps errors from the underlying `FFmpeg` library that don't
    /// fit into other categories.
    #[error("ffmpeg error: {message} (code={code})")]
    Ffmpeg {
        /// Raw `FFmpeg` error code (negative integer). `0` when no numeric code is available.
        code: i32,
        /// Human-readable error message from `av_strerror` or an internal description.
        message: String,
    },

    /// I/O error during file operations.
    ///
    /// This wraps standard I/O errors such as permission denied,
    /// disk full, or network errors for remote files.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The connection timed out before a response was received.
    ///
    /// Maps from `FFmpeg` error code `AVERROR(ETIMEDOUT)`.
    /// `endpoint` is the sanitized URL (password replaced with `***`,
    /// query string removed).
    #[error("network timeout: endpoint={endpoint} — {message} (code={code})")]
    NetworkTimeout {
        /// Raw `FFmpeg` error code.
        code: i32,
        /// Sanitized endpoint URL (no credentials, no query string).
        endpoint: String,
        /// Human-readable error message from `av_strerror`.
        message: String,
    },

    /// The connection was refused or the host could not be reached.
    ///
    /// Maps from `FFmpeg` error codes `AVERROR(ECONNREFUSED)`,
    /// `AVERROR(EHOSTUNREACH)`, `AVERROR(ENETUNREACH)`, and DNS failures.
    /// `endpoint` is the sanitized URL (password replaced with `***`,
    /// query string removed).
    #[error("connection failed: endpoint={endpoint} — {message} (code={code})")]
    ConnectionFailed {
        /// Raw `FFmpeg` error code.
        code: i32,
        /// Sanitized endpoint URL (no credentials, no query string).
        endpoint: String,
        /// Human-readable error message from `av_strerror`.
        message: String,
    },

    /// The stream was interrupted after a connection was established.
    ///
    /// Maps from `AVERROR(EIO)` and `AVERROR_EOF` in a network context.
    /// `endpoint` is the sanitized URL (password replaced with `***`,
    /// query string removed).
    #[error("stream interrupted: endpoint={endpoint} — {message} (code={code})")]
    StreamInterrupted {
        /// Raw `FFmpeg` error code.
        code: i32,
        /// Sanitized endpoint URL (no credentials, no query string).
        endpoint: String,
        /// Human-readable error message from `av_strerror`.
        message: String,
    },

    /// Seeking was requested on a live stream where seeking is not supported.
    ///
    /// Returned by `VideoDecoder::seek()` and `AudioDecoder::seek()` when
    /// `is_live()` returns `true`.
    #[error("seek is not supported on live streams")]
    SeekNotSupported,
}

impl DecodeError {
    /// Creates a new [`DecodeError::DecodingFailed`] with the given reason.
    ///
    /// # Arguments
    ///
    /// * `reason` - Description of why decoding failed.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::DecodeError;
    ///
    /// let error = DecodeError::decoding_failed("Corrupted frame data");
    /// assert!(error.to_string().contains("Corrupted frame data"));
    /// assert!(error.is_recoverable());
    /// ```
    #[must_use]
    pub fn decoding_failed(reason: impl Into<String>) -> Self {
        Self::DecodingFailed {
            timestamp: None,
            reason: reason.into(),
        }
    }

    /// Creates a new [`DecodeError::DecodingFailed`] with timestamp and reason.
    ///
    /// # Arguments
    ///
    /// * `timestamp` - The timestamp where decoding failed.
    /// * `reason` - Description of why decoding failed.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::DecodeError;
    /// use std::time::Duration;
    ///
    /// let error = DecodeError::decoding_failed_at(
    ///     Duration::from_secs(30),
    ///     "Invalid packet size"
    /// );
    /// assert!(error.to_string().contains("30s"));
    /// assert!(error.is_recoverable());
    /// ```
    #[must_use]
    pub fn decoding_failed_at(timestamp: Duration, reason: impl Into<String>) -> Self {
        Self::DecodingFailed {
            timestamp: Some(timestamp),
            reason: reason.into(),
        }
    }

    /// Creates a new [`DecodeError::SeekFailed`].
    ///
    /// # Arguments
    ///
    /// * `target` - The target position of the failed seek.
    /// * `reason` - Description of why the seek failed.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::DecodeError;
    /// use std::time::Duration;
    ///
    /// let error = DecodeError::seek_failed(
    ///     Duration::from_secs(60),
    ///     "Index not found"
    /// );
    /// assert!(error.to_string().contains("60s"));
    /// assert!(error.is_recoverable());
    /// ```
    #[must_use]
    pub fn seek_failed(target: Duration, reason: impl Into<String>) -> Self {
        Self::SeekFailed {
            target,
            reason: reason.into(),
        }
    }

    /// Creates a new [`DecodeError::DecoderUnavailable`].
    ///
    /// # Arguments
    ///
    /// * `codec` — Short codec name (e.g. `"exr"`).
    /// * `hint` — Human-readable suggestion for the user.
    #[must_use]
    pub fn decoder_unavailable(codec: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::DecoderUnavailable {
            codec: codec.into(),
            hint: hint.into(),
        }
    }

    /// Creates a new [`DecodeError::Ffmpeg`].
    ///
    /// # Arguments
    ///
    /// * `code` - The raw `FFmpeg` error code (negative integer). Pass `0` when no
    ///   numeric code is available.
    /// * `message` - Human-readable description of the error.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::DecodeError;
    ///
    /// let error = DecodeError::ffmpeg(-22, "Invalid data found when processing input");
    /// assert!(error.to_string().contains("Invalid data"));
    /// assert!(error.to_string().contains("code=-22"));
    /// ```
    #[must_use]
    pub fn ffmpeg(code: i32, message: impl Into<String>) -> Self {
        Self::Ffmpeg {
            code,
            message: message.into(),
        }
    }

    /// Returns `true` if this error is recoverable.
    ///
    /// Recoverable errors are those where the decoder can continue
    /// operating after the error, such as corrupted frames that can
    /// be skipped.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::DecodeError;
    /// use std::time::Duration;
    ///
    /// // Decoding failures are recoverable
    /// assert!(DecodeError::decoding_failed("test").is_recoverable());
    ///
    /// // Seek failures are recoverable
    /// assert!(DecodeError::seek_failed(Duration::from_secs(1), "test").is_recoverable());
    ///
    /// ```
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::DecodingFailed { .. } | Self::SeekFailed { .. })
    }

    /// Returns `true` if this error is fatal.
    ///
    /// Fatal errors indicate that the decoder cannot continue and
    /// must be recreated or the file reopened.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::DecodeError;
    /// use std::path::PathBuf;
    ///
    /// // File not found is fatal
    /// assert!(DecodeError::FileNotFound { path: PathBuf::new() }.is_fatal());
    ///
    /// // Unsupported codec is fatal
    /// assert!(DecodeError::UnsupportedCodec { codec: "test".to_string() }.is_fatal());
    ///
    /// ```
    #[must_use]
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::FileNotFound { .. }
                | Self::NoVideoStream { .. }
                | Self::NoAudioStream { .. }
                | Self::UnsupportedCodec { .. }
                | Self::DecoderUnavailable { .. }
        )
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_error_display() {
        let error = DecodeError::FileNotFound {
            path: PathBuf::from("/path/to/video.mp4"),
        };
        assert!(error.to_string().contains("File not found"));
        assert!(error.to_string().contains("/path/to/video.mp4"));

        let error = DecodeError::NoVideoStream {
            path: PathBuf::from("/path/to/audio.mp3"),
        };
        assert!(error.to_string().contains("No video stream"));

        let error = DecodeError::UnsupportedCodec {
            codec: "unknown_codec".to_string(),
        };
        assert!(error.to_string().contains("Codec not supported"));
        assert!(error.to_string().contains("unknown_codec"));
    }

    #[test]
    fn test_decoding_failed_constructor() {
        let error = DecodeError::decoding_failed("Corrupted frame data");
        match error {
            DecodeError::DecodingFailed { timestamp, reason } => {
                assert!(timestamp.is_none());
                assert_eq!(reason, "Corrupted frame data");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_decoding_failed_at_constructor() {
        let error = DecodeError::decoding_failed_at(Duration::from_secs(30), "Invalid packet size");
        match error {
            DecodeError::DecodingFailed { timestamp, reason } => {
                assert_eq!(timestamp, Some(Duration::from_secs(30)));
                assert_eq!(reason, "Invalid packet size");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_seek_failed_constructor() {
        let error = DecodeError::seek_failed(Duration::from_secs(60), "Index not found");
        match error {
            DecodeError::SeekFailed { target, reason } => {
                assert_eq!(target, Duration::from_secs(60));
                assert_eq!(reason, "Index not found");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_ffmpeg_constructor() {
        let error = DecodeError::ffmpeg(-22, "AVERROR_INVALIDDATA");
        match error {
            DecodeError::Ffmpeg { code, message } => {
                assert_eq!(code, -22);
                assert_eq!(message, "AVERROR_INVALIDDATA");
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn ffmpeg_should_format_with_code_and_message() {
        let error = DecodeError::ffmpeg(-22, "Invalid data");
        assert!(error.to_string().contains("code=-22"));
        assert!(error.to_string().contains("Invalid data"));
    }

    #[test]
    fn ffmpeg_with_zero_code_should_be_constructible() {
        let error = DecodeError::ffmpeg(0, "allocation failed");
        assert!(matches!(error, DecodeError::Ffmpeg { code: 0, .. }));
    }

    #[test]
    fn decoder_unavailable_should_include_codec_and_hint() {
        let e = DecodeError::decoder_unavailable(
            "exr",
            "Requires FFmpeg built with EXR support (--enable-decoder=exr)",
        );
        assert!(e.to_string().contains("exr"));
        assert!(e.to_string().contains("Requires FFmpeg"));
    }

    #[test]
    fn decoder_unavailable_should_be_fatal() {
        let e = DecodeError::decoder_unavailable("exr", "hint");
        assert!(e.is_fatal());
        assert!(!e.is_recoverable());
    }

    #[test]
    fn test_is_recoverable() {
        assert!(DecodeError::decoding_failed("test").is_recoverable());
        assert!(DecodeError::seek_failed(Duration::from_secs(1), "test").is_recoverable());
        assert!(
            !DecodeError::FileNotFound {
                path: PathBuf::new()
            }
            .is_recoverable()
        );
    }

    #[test]
    fn test_is_fatal() {
        assert!(
            DecodeError::FileNotFound {
                path: PathBuf::new()
            }
            .is_fatal()
        );
        assert!(
            DecodeError::NoVideoStream {
                path: PathBuf::new()
            }
            .is_fatal()
        );
        assert!(
            DecodeError::NoAudioStream {
                path: PathBuf::new()
            }
            .is_fatal()
        );
        assert!(
            DecodeError::UnsupportedCodec {
                codec: "test".to_string()
            }
            .is_fatal()
        );
        assert!(!DecodeError::decoding_failed("test").is_fatal());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let decode_error: DecodeError = io_error.into();
        assert!(matches!(decode_error, DecodeError::Io(_)));
    }

    #[test]
    fn test_hw_accel_unavailable() {
        let error = DecodeError::HwAccelUnavailable {
            accel: HardwareAccel::Nvdec,
        };
        assert!(
            error
                .to_string()
                .contains("Hardware acceleration unavailable")
        );
        assert!(error.to_string().contains("Nvdec"));
    }
}
