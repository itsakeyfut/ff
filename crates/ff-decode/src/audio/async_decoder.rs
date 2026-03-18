//! Async audio decoder backed by `tokio::task::spawn_blocking`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ff_format::AudioFrame;
use futures::stream::{self, Stream};

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
    inner: Arc<Mutex<AudioDecoder>>,
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
            inner: Arc::new(Mutex::new(decoder)),
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
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .map_err(|_| DecodeError::Ffmpeg {
                    code: 0,
                    message: "mutex poisoned".to_string(),
                })?
                .decode_one()
        })
        .await
        .map_err(|e| DecodeError::Ffmpeg {
            code: 0,
            message: format!("spawn_blocking panicked: {e}"),
        })?
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
        stream::unfold(self, |mut decoder| async move {
            match decoder.decode_frame().await {
                Ok(Some(frame)) => Some((Ok(frame), decoder)),
                Ok(None) => None,
                Err(e) => Some((Err(e), decoder)),
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
