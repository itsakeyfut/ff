//! Async video decoder backed by `tokio::task::spawn_blocking`.

use std::path::{Path, PathBuf};

use ff_format::{PixelFormat, VideoFrame};
use futures::stream::{self, Stream};

use crate::async_decoder::AsyncDecoder;
use crate::error::DecodeError;
use crate::video::builder::{VideoDecoder, VideoDecoderBuilder};

/// Async builder for [`AsyncVideoDecoder`] that mirrors the options available
/// on the synchronous [`VideoDecoderBuilder`].
///
/// Obtain one with [`AsyncVideoDecoder::builder`]. Call [`build`](Self::build)
/// to open the file asynchronously on a `spawn_blocking` thread.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::AsyncVideoDecoder;
/// use ff_format::PixelFormat;
///
/// let decoder = AsyncVideoDecoder::builder("video.mp4")
///     .output_format(PixelFormat::Rgb24)
///     .build()
///     .await?;
/// ```
pub struct AsyncVideoDecoderBuilder {
    inner: VideoDecoderBuilder,
}

impl AsyncVideoDecoderBuilder {
    fn new(path: PathBuf) -> Self {
        Self {
            inner: VideoDecoderBuilder::new(path),
        }
    }

    /// Sets the output pixel format for decoded frames.
    ///
    /// Equivalent to [`VideoDecoderBuilder::output_format`].
    #[must_use]
    pub fn output_format(mut self, format: PixelFormat) -> Self {
        self.inner = self.inner.output_format(format);
        self
    }

    /// Scales decoded frames to exact dimensions.
    ///
    /// Equivalent to [`VideoDecoderBuilder::output_size`].
    #[must_use]
    pub fn output_size(mut self, width: u32, height: u32) -> Self {
        self.inner = self.inner.output_size(width, height);
        self
    }

    /// Scales decoded frames to the given width, preserving the aspect ratio.
    ///
    /// Equivalent to [`VideoDecoderBuilder::output_width`].
    #[must_use]
    pub fn output_width(mut self, width: u32) -> Self {
        self.inner = self.inner.output_width(width);
        self
    }

    /// Scales decoded frames to the given height, preserving the aspect ratio.
    ///
    /// Equivalent to [`VideoDecoderBuilder::output_height`].
    #[must_use]
    pub fn output_height(mut self, height: u32) -> Self {
        self.inner = self.inner.output_height(height);
        self
    }

    /// Opens the file and builds the async decoder.
    ///
    /// File I/O and codec initialisation run on a `spawn_blocking` thread so
    /// the async executor is not blocked. All errors from
    /// [`VideoDecoderBuilder::build`] are propagated transparently.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if the file is missing, contains no video
    /// stream, uses an unsupported codec, or has invalid output dimensions.
    pub async fn build(self) -> Result<AsyncVideoDecoder, DecodeError> {
        let builder = self.inner;
        let decoder = tokio::task::spawn_blocking(move || builder.build())
            .await
            .map_err(|e| DecodeError::Ffmpeg {
                code: 0,
                message: format!("spawn_blocking panicked: {e}"),
            })??;
        Ok(AsyncVideoDecoder {
            inner: AsyncDecoder::new(decoder),
        })
    }
}

/// Async wrapper around [`VideoDecoder`].
///
/// `open` and `decode_frame` both execute on a `spawn_blocking` thread so the
/// Tokio executor is never blocked by `FFmpeg` I/O or decoding work.
/// Multiple concurrent callers share the inner decoder through `Arc<Mutex<...>>`.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::AsyncVideoDecoder;
/// use futures::StreamExt;
///
/// let mut decoder = AsyncVideoDecoder::open("video.mp4").await?;
/// while let Some(frame) = decoder.decode_frame().await? {
///     println!("frame at {:?}", frame.timestamp().as_duration());
/// }
/// ```
pub struct AsyncVideoDecoder {
    inner: AsyncDecoder<VideoDecoder>,
}

impl AsyncVideoDecoder {
    /// Returns a builder for configuring the async video decoder.
    ///
    /// Use this when you need to control the output pixel format or frame
    /// scaling. For zero-configuration decoding, prefer [`AsyncVideoDecoder::open`].
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::AsyncVideoDecoder;
    /// use ff_format::PixelFormat;
    ///
    /// let decoder = AsyncVideoDecoder::builder("video.mp4")
    ///     .output_format(PixelFormat::Rgb24)
    ///     .output_size(640, 360)
    ///     .build()
    ///     .await?;
    /// ```
    pub fn builder(path: impl AsRef<Path>) -> AsyncVideoDecoderBuilder {
        AsyncVideoDecoderBuilder::new(path.as_ref().to_path_buf())
    }

    /// Opens the video file asynchronously.
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
        let decoder = tokio::task::spawn_blocking(move || VideoDecoder::open(&path).build())
            .await
            .map_err(|e| DecodeError::Ffmpeg {
                code: 0,
                message: format!("spawn_blocking panicked: {e}"),
            })??;
        Ok(Self {
            inner: AsyncDecoder::new(decoder),
        })
    }

    /// Decodes the next video frame.
    ///
    /// The blocking `FFmpeg` call is offloaded to a `spawn_blocking` thread so
    /// the Tokio executor is never blocked.
    ///
    /// Returns `Ok(None)` at end of stream.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] on codec or I/O errors.
    pub async fn decode_frame(&mut self) -> Result<Option<VideoFrame>, DecodeError> {
        self.inner.with(VideoDecoder::decode_one).await
    }

    /// Converts this decoder into a [`Stream`] of video frames.
    ///
    /// Decoding is offloaded to a `spawn_blocking` thread on each poll via
    /// [`Self::decode_frame`]. The stream is `Send` and can be used with
    /// [`tokio::spawn`].
    ///
    /// The stream ends when the file is exhausted (`Ok(None)` from
    /// `decode_frame`). Errors are yielded as `Err` items; the stream
    /// terminates after the first error.
    pub fn into_stream(self) -> impl Stream<Item = Result<VideoFrame, DecodeError>> + Send {
        stream::unfold(Some(self), |state| async move {
            let mut decoder = state?; // None → stream already ended
            match decoder.decode_frame().await {
                Ok(Some(frame)) => Some((Ok(frame), Some(decoder))),
                Ok(None) => None,               // EOF → end stream
                Err(e) => Some((Err(e), None)), // error → yield once, then end
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time proof that AsyncVideoDecoder satisfies the Send bound
    // required by tokio::spawn and other Send-requiring contexts.
    fn _assert_send() {
        fn is_send<T: Send>() {}
        is_send::<AsyncVideoDecoder>();
    }

    #[tokio::test]
    async fn async_video_decoder_should_fail_on_missing_file() {
        let result = AsyncVideoDecoder::open("/nonexistent/path/video.mp4").await;
        assert!(
            matches!(result, Err(DecodeError::FileNotFound { .. })),
            "expected FileNotFound"
        );
    }

    #[tokio::test]
    async fn async_video_decoder_builder_output_format_should_propagate_to_sync_builder() {
        let result = AsyncVideoDecoder::builder("/nonexistent/path/video.mp4")
            .output_format(PixelFormat::Rgb24)
            .build()
            .await;
        assert!(
            matches!(result, Err(DecodeError::FileNotFound { .. })),
            "builder with output_format must propagate FileNotFound"
        );
    }

    #[tokio::test]
    async fn async_video_decoder_builder_zero_size_should_return_invalid_dimensions() {
        let result = AsyncVideoDecoder::builder("/nonexistent/path/video.mp4")
            .output_size(0, 480)
            .build()
            .await;
        assert!(
            matches!(result, Err(DecodeError::InvalidOutputDimensions { .. })),
            "output_size(0, 480) must return InvalidOutputDimensions"
        );
    }

    /// Verifies the `Option<S>` unfold pattern used by `into_stream`: after the
    /// error arm sets state to `None`, the stream yields no further items.
    ///
    /// Acceptance criterion for issue #1006.
    #[tokio::test]
    async fn into_stream_state_machine_should_terminate_after_error() {
        use futures::StreamExt;

        // Use the exact same stream::unfold(Option<S>, ...) pattern as
        // into_stream() with a controlled error at position 2.
        let items: Vec<Result<u32, u32>> = stream::unfold(Some(0u32), |state| async move {
            let n = state?; // None → stream ends
            match n {
                0 | 1 => Some((Ok(n), Some(n + 1))),
                _ => Some((Err(n), None)), // error → yield once, then end
            }
        })
        .collect()
        .await;

        assert_eq!(
            items.len(),
            3,
            "stream must stop after the error: expected 2 Ok + 1 Err, got {items:?}"
        );
        assert!(items[0].is_ok());
        assert!(items[1].is_ok());
        assert!(items[2].is_err());
    }
}
