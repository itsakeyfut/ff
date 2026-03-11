//! Error types for encoding operations.

use std::path::PathBuf;
use thiserror::Error;

/// Encoding error type.
#[derive(Error, Debug)]
pub enum EncodeError {
    /// Cannot create output file
    #[error("Cannot create output file: {path}")]
    CannotCreateFile {
        /// File path that failed
        path: PathBuf,
    },

    /// Unsupported codec
    #[error("Unsupported codec: {codec}")]
    UnsupportedCodec {
        /// Codec name
        codec: String,
    },

    /// No suitable encoder found
    #[error("No suitable encoder found for {codec} (tried: {tried:?})")]
    NoSuitableEncoder {
        /// Requested codec
        codec: String,
        /// Attempted encoders
        tried: Vec<String>,
    },

    /// Encoding failed at specific frame
    #[error("Encoding failed at frame {frame}: {reason}")]
    EncodingFailed {
        /// Frame number where encoding failed
        frame: u64,
        /// Failure reason
        reason: String,
    },

    /// Invalid configuration
    #[error("Invalid configuration: {reason}")]
    InvalidConfig {
        /// Configuration issue description
        reason: String,
    },

    /// Hardware encoder unavailable
    #[error("Hardware encoder unavailable: {encoder}")]
    HwEncoderUnavailable {
        /// Hardware encoder name
        encoder: String,
    },

    /// Muxing failed
    #[error("Muxing failed: {reason}")]
    MuxingFailed {
        /// Failure reason
        reason: String,
    },

    /// `FFmpeg` error
    #[error("FFmpeg error: {0}")]
    Ffmpeg(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Encoding cancelled by user
    #[error("Encoding cancelled by user")]
    Cancelled,
}

impl EncodeError {
    /// Create an error from an FFmpeg error code.
    ///
    /// This is more type-safe than implementing `From<i32>` globally,
    /// as it makes the conversion explicit and prevents accidental
    /// conversion of arbitrary i32 values.
    pub(crate) fn from_ffmpeg_error(errnum: i32) -> Self {
        let error_msg = ff_sys::av_error_string(errnum);
        EncodeError::Ffmpeg(format!("{} (code: {})", error_msg, errnum))
    }
}
