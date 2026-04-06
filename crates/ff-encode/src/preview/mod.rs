//! Preview generation — sprite sheets and animated GIFs.
//!
//! [`SpriteSheet`] samples evenly-spaced frames from a video and tiles them
//! into a single PNG image suitable for video-player scrub-bar hover previews.

mod preview_inner;

use std::path::{Path, PathBuf};

use crate::EncodeError;

/// Generates a thumbnail sprite sheet from a video file.
///
/// Frames are sampled at evenly-spaced intervals across the full video
/// duration and tiled into a single PNG image of size
/// `cols × frame_width` × `rows × frame_height`.
///
/// # Examples
///
/// ```ignore
/// use ff_encode::SpriteSheet;
///
/// SpriteSheet::new("video.mp4")
///     .cols(5)
///     .rows(4)
///     .frame_width(160)
///     .frame_height(90)
///     .output("sprites.png")
///     .run()?;
/// ```
pub struct SpriteSheet {
    input: PathBuf,
    cols: u32,
    rows: u32,
    frame_width: u32,
    frame_height: u32,
    output: PathBuf,
}

impl SpriteSheet {
    /// Creates a new `SpriteSheet` for the given input file.
    ///
    /// Defaults: `cols=10`, `rows=10`, `frame_width=160`, `frame_height=90`,
    /// no output path set.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            cols: 10,
            rows: 10,
            frame_width: 160,
            frame_height: 90,
            output: PathBuf::new(),
        }
    }

    /// Sets the number of columns in the sprite grid (default: 10).
    #[must_use]
    pub fn cols(self, n: u32) -> Self {
        Self { cols: n, ..self }
    }

    /// Sets the number of rows in the sprite grid (default: 10).
    #[must_use]
    pub fn rows(self, n: u32) -> Self {
        Self { rows: n, ..self }
    }

    /// Sets the width of each individual thumbnail frame in pixels (default: 160).
    #[must_use]
    pub fn frame_width(self, w: u32) -> Self {
        Self {
            frame_width: w,
            ..self
        }
    }

    /// Sets the height of each individual thumbnail frame in pixels (default: 90).
    #[must_use]
    pub fn frame_height(self, h: u32) -> Self {
        Self {
            frame_height: h,
            ..self
        }
    }

    /// Sets the output path for the generated PNG file.
    #[must_use]
    pub fn output(self, path: impl AsRef<Path>) -> Self {
        Self {
            output: path.as_ref().to_path_buf(),
            ..self
        }
    }

    /// Runs the sprite sheet generation.
    ///
    /// Output image dimensions: `cols × frame_width` × `rows × frame_height`.
    ///
    /// # Errors
    ///
    /// - [`EncodeError::MediaOperationFailed`] — `cols` or `rows` is zero,
    ///   `frame_width` or `frame_height` is zero, or `output` path is not set.
    /// - [`EncodeError::Ffmpeg`] — any FFmpeg filter graph or encoding call fails.
    pub fn run(self) -> Result<(), EncodeError> {
        if self.cols == 0 || self.rows == 0 {
            return Err(EncodeError::MediaOperationFailed {
                reason: "cols/rows must be > 0".to_string(),
            });
        }
        if self.frame_width == 0 || self.frame_height == 0 {
            return Err(EncodeError::MediaOperationFailed {
                reason: "frame_width/frame_height must be > 0".to_string(),
            });
        }
        if self.output.as_os_str().is_empty() {
            return Err(EncodeError::MediaOperationFailed {
                reason: "output path not set".to_string(),
            });
        }
        // SAFETY: preview_inner manages all raw pointer lifetimes per avfilter rules.
        unsafe {
            preview_inner::generate_sprite_sheet_unsafe(
                &self.input,
                self.cols,
                self.rows,
                self.frame_width,
                self.frame_height,
                &self.output,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_sheet_zero_cols_should_return_media_operation_failed() {
        let result = SpriteSheet::new("irrelevant.mp4")
            .cols(0)
            .output("out.png")
            .run();
        assert!(
            matches!(result, Err(EncodeError::MediaOperationFailed { .. })),
            "expected MediaOperationFailed for cols=0, got {result:?}"
        );
    }

    #[test]
    fn sprite_sheet_zero_frame_width_should_return_media_operation_failed() {
        let result = SpriteSheet::new("irrelevant.mp4")
            .frame_width(0)
            .output("out.png")
            .run();
        assert!(
            matches!(result, Err(EncodeError::MediaOperationFailed { .. })),
            "expected MediaOperationFailed for frame_width=0, got {result:?}"
        );
    }

    #[test]
    fn sprite_sheet_missing_output_should_return_media_operation_failed() {
        let result = SpriteSheet::new("irrelevant.mp4").run();
        assert!(
            matches!(result, Err(EncodeError::MediaOperationFailed { .. })),
            "expected MediaOperationFailed for empty output path, got {result:?}"
        );
    }
}
