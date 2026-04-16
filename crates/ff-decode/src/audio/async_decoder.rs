//! Async audio decoder backed by `tokio::task::spawn_blocking`.

use std::path::{Path, PathBuf};

use ff_format::AudioFrame;
use futures::stream::{self, Stream};

use crate::async_decoder::AsyncDecoder;
use crate::audio::builder::AudioDecoder;
use crate::error::DecodeError;

/// Async wrapper around [`AudioDecoder`].
///
/// `open` and `decode_frame` both execute on a `spawn_blocking` thread so the
/// Tokio executor is never blocked by `FFmpeg` I/O or decoding work.
/// Multiple concurrent callers share the inner decoder through `Arc<Mutex<...>>`.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::AsyncAudioDecoder;
/// use futures::StreamExt;
///
/// let mut decoder = AsyncAudioDecoder::open("audio.mp3").await?;
/// while let Some(frame) = decoder.decode_frame().await? {
///     println!("audio frame with {} samples", frame.samples());
/// }
/// ```
pub struct AsyncAudioDecoder {
    inner: AsyncDecoder<AudioDecoder>,
}

impl AsyncAudioDecoder {
    /// Opens the audio file asynchronously.
    ///
    /// File I/O and codec initialisation are performed on a `spawn_blocking`
    /// thread so the async executor is not blocked.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if the file is missing, contains no audio
    /// stream, or uses an unsupported codec.
    pub async fn open(path: impl AsRef<Path> + Send + 'static) -> Result<Self, DecodeError> {
        let path: PathBuf = path.as_ref().to_path_buf();
        let decoder = tokio::task::spawn_blocking(move || AudioDecoder::open(&path).build())
            .await
            .map_err(|e| DecodeError::Ffmpeg {
                code: 0,
                message: format!("spawn_blocking panicked: {e}"),
            })??;
        Ok(Self {
            inner: AsyncDecoder::new(decoder),
        })
    }

    /// Decodes the next audio frame.
    ///
    /// The blocking `FFmpeg` call is offloaded to a `spawn_blocking` thread so
    /// the Tokio executor is never blocked.
    ///
    /// Returns `Ok(None)` at end of stream.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] on codec or I/O errors.
    pub async fn decode_frame(&mut self) -> Result<Option<AudioFrame>, DecodeError> {
        self.inner.with(AudioDecoder::decode_one).await
    }

    /// Converts this decoder into a [`Stream`] of audio frames.
    ///
    /// Decoding is offloaded to a `spawn_blocking` thread on each poll via
    /// [`Self::decode_frame`]. The stream is `Send` and can be used with
    /// [`tokio::spawn`].
    ///
    /// The stream ends when the file is exhausted (`Ok(None)` from
    /// `decode_frame`). Errors are yielded as `Err` items; the stream
    /// terminates after the first error.
    pub fn into_stream(self) -> impl Stream<Item = Result<AudioFrame, DecodeError>> + Send {
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

    // Compile-time proof that AsyncAudioDecoder satisfies the Send bound
    // required by tokio::spawn and other Send-requiring contexts.
    fn _assert_send() {
        fn is_send<T: Send>() {}
        is_send::<AsyncAudioDecoder>();
    }

    #[tokio::test]
    async fn async_audio_decoder_should_fail_on_missing_file() {
        let result = AsyncAudioDecoder::open("/nonexistent/path/audio.mp3").await;
        assert!(
            matches!(result, Err(DecodeError::FileNotFound { .. })),
            "expected FileNotFound"
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
