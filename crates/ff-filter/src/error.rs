//! Error types for filter graph operations.

use thiserror::Error;

/// Errors that can occur during filter graph construction and processing.
#[derive(Debug, Error)]
pub enum FilterError {
    /// Failed to build the filter graph (invalid filter chain or `FFmpeg` error
    /// during graph creation).
    #[error("failed to build filter graph")]
    BuildFailed,

    /// A frame processing operation (push or pull) failed.
    #[error("failed to process frame")]
    ProcessFailed,

    /// A frame was pushed to an invalid input slot.
    #[error("invalid input: slot={slot} reason={reason}")]
    InvalidInput {
        /// The slot index that was out of range or otherwise invalid.
        slot: usize,
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// An underlying `FFmpeg` function returned an error code.
    #[error("ffmpeg error: {message} (code={code})")]
    Ffmpeg {
        /// The raw `FFmpeg` error code.
        code: i32,
        /// Human-readable description of the error.
        message: String,
    },
}
