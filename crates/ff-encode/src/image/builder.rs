//! Image encoder builder and public API.

use std::path::{Path, PathBuf};

use ff_format::VideoFrame;

use crate::EncodeError;

use super::encoder_inner;

/// Builder for [`ImageEncoder`].
///
/// Created via [`ImageEncoder::create`]. Validates the output path extension
/// at [`build`](ImageEncoderBuilder::build) time so errors surface early.
#[derive(Debug)]
pub struct ImageEncoderBuilder {
    path: PathBuf,
}

impl ImageEncoderBuilder {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Validate the file extension and return an [`ImageEncoder`].
    ///
    /// Returns [`EncodeError::InvalidConfig`] when the path has no extension
    /// and [`EncodeError::UnsupportedCodec`] for unrecognised extensions.
    pub fn build(self) -> Result<ImageEncoder, EncodeError> {
        // Validate at build time — fail fast before touching the filesystem.
        encoder_inner::codec_from_extension(&self.path)?;
        Ok(ImageEncoder { path: self.path })
    }
}

/// Encodes a single [`VideoFrame`] to a still image file.
///
/// The output format is inferred from the file extension: `.jpg`/`.jpeg`,
/// `.png`, `.bmp`, `.tif`/`.tiff`, or `.webp`.
///
/// # Example
///
/// ```ignore
/// use ff_encode::ImageEncoder;
///
/// let encoder = ImageEncoder::create("thumbnail.png").build()?;
/// encoder.encode(&frame)?;
/// ```
#[derive(Debug)]
pub struct ImageEncoder {
    path: PathBuf,
}

impl ImageEncoder {
    /// Start building an image encoder that writes to `path`.
    ///
    /// This is infallible; extension validation happens in
    /// [`ImageEncoderBuilder::build`].
    pub fn create(path: impl AsRef<Path>) -> ImageEncoderBuilder {
        ImageEncoderBuilder::new(path.as_ref().to_path_buf())
    }

    /// Encode `frame` and write it to the output file.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFmpeg encoder is unavailable, the output file
    /// cannot be created, or encoding fails.
    pub fn encode(self, frame: &VideoFrame) -> Result<(), EncodeError> {
        // SAFETY: encode_image manages all FFmpeg resources internally and
        // frees them before returning, whether on success or error.
        unsafe { encoder_inner::encode_image(&self.path, frame) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_should_return_builder() {
        // ImageEncoder::create is infallible
        let _builder = ImageEncoder::create("out.png");
    }

    #[test]
    fn build_with_unsupported_extension_should_return_error() {
        let result = ImageEncoder::create("out.avi").build();
        assert!(
            matches!(result, Err(EncodeError::UnsupportedCodec { .. })),
            "expected UnsupportedCodec, got {result:?}"
        );
    }

    #[test]
    fn build_with_no_extension_should_return_error() {
        let result = ImageEncoder::create("out_no_ext").build();
        assert!(
            matches!(result, Err(EncodeError::InvalidConfig { .. })),
            "expected InvalidConfig, got {result:?}"
        );
    }
}
