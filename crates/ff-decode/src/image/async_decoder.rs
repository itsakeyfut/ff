//! Async image decoder backed by `tokio::task::spawn_blocking`.

use std::path::{Path, PathBuf};

use ff_format::VideoFrame;

use crate::error::DecodeError;
use crate::image::builder::ImageDecoder;

/// Async wrapper around [`ImageDecoder`].
///
/// Both `open` (file I/O + codec init) and `decode` (pixel conversion) are
/// performed on `spawn_blocking` threads so the async executor is not blocked.
///
/// There is no `into_stream` method because an image is a single frame, not a
/// sequence.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::AsyncImageDecoder;
///
/// let frame = AsyncImageDecoder::open("photo.png").await?.decode().await?;
/// println!("{}x{}", frame.width(), frame.height());
/// ```
pub struct AsyncImageDecoder {
    inner: ImageDecoder,
}

impl AsyncImageDecoder {
    /// Opens the image file asynchronously.
    ///
    /// File I/O and codec initialisation are performed on a `spawn_blocking`
    /// thread so the async executor is not blocked.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if the file is missing, contains no video
    /// stream, or uses an unsupported codec.
    pub async fn open(path: impl AsRef<Path> + Send + 'static) -> Result<Self, DecodeError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        let decoder = tokio::task::spawn_blocking(move || ImageDecoder::open(&path).build())
            .await
            .map_err(|e| DecodeError::Ffmpeg {
                code: 0,
                message: format!("spawn_blocking panicked: {e}"),
            })??;
        Ok(Self { inner: decoder })
    }

    /// Decodes the image into a [`VideoFrame`].
    ///
    /// This consuming method runs on a `spawn_blocking` thread.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] on codec or I/O errors.
    pub async fn decode(self) -> Result<VideoFrame, DecodeError> {
        tokio::task::spawn_blocking(move || self.inner.decode())
            .await
            .map_err(|e| DecodeError::Ffmpeg {
                code: 0,
                message: format!("spawn_blocking panicked: {e}"),
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn async_image_decoder_should_fail_on_missing_file() {
        let result = AsyncImageDecoder::open("/nonexistent/path/photo.png").await;
        assert!(
            matches!(result, Err(DecodeError::FileNotFound { .. })),
            "expected FileNotFound"
        );
    }
}
