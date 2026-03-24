//! Safe public API for media file probing.
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

use std::path::Path;

use ff_format::MediaInfo;

use crate::error::ProbeError;

use super::probe_inner;

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

    log::debug!("probing media file path={}", path.display());

    // Check if file exists
    if !path.exists() {
        return Err(ProbeError::FileNotFound {
            path: path.to_path_buf(),
        });
    }

    // Get file size - propagate error since file may exist but be inaccessible (permission denied, etc.)
    let file_size = std::fs::metadata(path).map(|m| m.len())?;

    probe_inner::probe_file(path, file_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_should_return_file_not_found_for_nonexistent_path() {
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
    fn open_should_return_error_for_invalid_media() {
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
}
