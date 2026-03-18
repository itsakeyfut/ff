//! Image decoder builder for constructing image decoders.
//!
//! This module provides the [`ImageDecoderBuilder`] type which enables fluent
//! configuration of image decoders. Use [`ImageDecoder::open()`] to start building.

use std::path::{Path, PathBuf};

use ff_format::VideoFrame;

use crate::error::DecodeError;
use crate::image::decoder_inner::ImageDecoderInner;

/// Builder for configuring and constructing an [`ImageDecoder`].
///
/// Created by calling [`ImageDecoder::open()`]. Call [`build()`](Self::build)
/// to open the file and prepare for decoding.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::ImageDecoder;
///
/// let frame = ImageDecoder::open("photo.png").build()?.decode()?;
/// println!("{}x{}", frame.width(), frame.height());
/// ```
#[derive(Debug)]
pub struct ImageDecoderBuilder {
    path: PathBuf,
}

impl ImageDecoderBuilder {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Opens the image file and returns an [`ImageDecoder`] ready to decode.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if the file cannot be opened, contains no
    /// video stream, or uses an unsupported codec.
    pub fn build(self) -> Result<ImageDecoder, DecodeError> {
        if !self.path.exists() {
            return Err(DecodeError::FileNotFound {
                path: self.path.clone(),
            });
        }
        let inner = ImageDecoderInner::new(&self.path)?;
        let width = inner.width();
        let height = inner.height();
        Ok(ImageDecoder {
            inner: Some(inner),
            width,
            height,
        })
    }
}

/// Decodes a single still image into a [`VideoFrame`].
///
/// Supports common image formats: JPEG, PNG, BMP, TIFF, WebP.
///
/// # Construction
///
/// Use [`ImageDecoder::open()`] to create a builder, then call
/// [`ImageDecoderBuilder::build()`]:
///
/// ```ignore
/// use ff_decode::ImageDecoder;
///
/// let frame = ImageDecoder::open("photo.png").build()?.decode()?;
/// println!("{}x{}", frame.width(), frame.height());
/// ```
///
/// # Frame Decoding
///
/// The image can be decoded as a single frame or via an iterator:
///
/// ```ignore
/// // Single frame (consuming)
/// let frame = decoder.decode()?;
///
/// // Via iterator (for API consistency with VideoDecoder / AudioDecoder)
/// for frame in decoder.frames() {
///     let frame = frame?;
/// }
/// ```
pub struct ImageDecoder {
    /// Inner `FFmpeg` state; `None` after the frame has been decoded.
    inner: Option<ImageDecoderInner>,
    /// Cached width so it remains accessible after `decode_one` consumes `inner`.
    width: u32,
    /// Cached height so it remains accessible after `decode_one` consumes `inner`.
    height: u32,
}

impl ImageDecoder {
    /// Creates a builder for the specified image file path.
    ///
    /// # Note
    ///
    /// This method does not validate that the file exists or is a valid image.
    /// Validation occurs when [`ImageDecoderBuilder::build()`] is called.
    pub fn open(path: impl AsRef<Path>) -> ImageDecoderBuilder {
        ImageDecoderBuilder::new(path.as_ref().to_path_buf())
    }

    /// Returns the image width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the image height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Decodes the image frame.
    ///
    /// Returns `Ok(Some(frame))` on the first call, then `Ok(None)` on
    /// subsequent calls (the underlying `FFmpeg` context is consumed on first
    /// decode).
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if `FFmpeg` fails to decode the image.
    pub fn decode_one(&mut self) -> Result<Option<VideoFrame>, DecodeError> {
        let Some(inner) = self.inner.take() else {
            return Ok(None);
        };
        Ok(Some(inner.decode()?))
    }

    /// Returns an iterator that yields the single decoded image frame.
    ///
    /// This method exists for API consistency with [`VideoDecoder`] and
    /// [`AudioDecoder`].  The iterator yields at most one item.
    ///
    /// [`VideoDecoder`]: crate::VideoDecoder
    /// [`AudioDecoder`]: crate::AudioDecoder
    pub fn frames(&mut self) -> impl Iterator<Item = Result<VideoFrame, DecodeError>> + '_ {
        ImageFrameIterator { decoder: self }
    }

    /// Decodes the image, consuming `self` and returning the [`VideoFrame`].
    ///
    /// This is a convenience wrapper around [`decode_one`](Self::decode_one)
    /// for the common single-frame use-case.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if the image cannot be decoded or was already
    /// decoded.
    pub fn decode(mut self) -> Result<VideoFrame, DecodeError> {
        self.decode_one()?.ok_or_else(|| DecodeError::Ffmpeg {
            code: 0,
            message: "Image already decoded".to_string(),
        })
    }
}

/// Iterator over the decoded image frame.
///
/// Created by calling [`ImageDecoder::frames()`]. Yields exactly one item —
/// the decoded [`VideoFrame`] — then returns `None`.
///
/// This type exists for API consistency with `VideoDecoder::frames()` and
/// `AudioDecoder::frames()`.
pub(crate) struct ImageFrameIterator<'a> {
    decoder: &'a mut ImageDecoder,
}

impl Iterator for ImageFrameIterator<'_> {
    type Item = Result<VideoFrame, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.decoder.decode_one() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn open_nonexistent_file_should_return_file_not_found() {
        let result = ImageDecoder::open("nonexistent_image_12345.png").build();
        assert!(result.is_err());
        assert!(matches!(result, Err(DecodeError::FileNotFound { .. })));
    }

    #[test]
    fn builder_new_should_store_path() {
        let builder = ImageDecoderBuilder::new(PathBuf::from("photo.png"));
        assert_eq!(builder.path, PathBuf::from("photo.png"));
    }
}
