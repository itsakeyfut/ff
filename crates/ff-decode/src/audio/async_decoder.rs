//! Async audio decoder backed by `tokio::task::spawn_blocking`.

use std::path::{Path, PathBuf};

use ff_format::AudioFrame;
use futures::stream::{self, Stream};

use crate::audio::builder::AudioDecoder;
use crate::error::DecodeError;

/// Async wrapper around [`AudioDecoder`].
///
/// All blocking `FFmpeg` calls are offloaded to a `spawn_blocking` thread during
/// `open`. Frame decoding calls `decode_one` directly on the async thread —
/// each call takes microseconds, so the brief blocking is acceptable.
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
    inner: AudioDecoder,
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
        Ok(Self { inner: decoder })
    }

    /// Decodes the next audio frame.
    ///
    /// Returns `Ok(None)` at end of stream.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] on codec or I/O errors.
    // decode_one() is synchronous but the method is intentionally `async` so
    // callers can uniformly `.await` it alongside other async operations.
    #[allow(clippy::unused_async)]
    pub async fn decode_frame(&mut self) -> Result<Option<AudioFrame>, DecodeError> {
        self.inner.decode_one()
    }

    /// Converts this decoder into a [`Stream`] of audio frames.
    ///
    /// The stream ends when the decoder reaches end-of-file or encounters an
    /// error.
    pub fn into_stream(self) -> impl Stream<Item = Result<AudioFrame, DecodeError>> {
        stream::unfold(Some(self), |state| async move {
            let mut decoder = state?;
            match decoder.decode_frame().await {
                Ok(Some(frame)) => Some((Ok(frame), Some(decoder))),
                Ok(None) => None,
                Err(e) => Some((Err(e), None)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn async_audio_decoder_should_fail_on_missing_file() {
        let result = AsyncAudioDecoder::open("/nonexistent/path/audio.mp3").await;
        assert!(
            matches!(result, Err(DecodeError::FileNotFound { .. })),
            "expected FileNotFound"
        );
    }
}
