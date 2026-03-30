//! Generic async decoder helper backed by `tokio::task::spawn_blocking`.

use std::sync::{Arc, Mutex};

use crate::error::DecodeError;

/// Generic async wrapper for a synchronous decoder `T`.
///
/// Holds the decoder behind `Arc<Mutex<T>>` and offloads every blocking call
/// to a `spawn_blocking` thread so the Tokio executor is never blocked.
///
/// This is a crate-internal helper used by [`AsyncVideoDecoder`] and
/// [`AsyncAudioDecoder`]; it is not part of the public API.
///
/// [`AsyncVideoDecoder`]: crate::video::AsyncVideoDecoder
/// [`AsyncAudioDecoder`]: crate::audio::AsyncAudioDecoder
pub(crate) struct AsyncDecoder<T> {
    pub(crate) inner: Arc<Mutex<T>>,
}

impl<T: Send + 'static> AsyncDecoder<T> {
    /// Wraps an already-opened synchronous decoder.
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Runs a blocking closure on the inner decoder in a `spawn_blocking` thread.
    ///
    /// Acquires the mutex, calls `f` with a mutable reference to `T`, and
    /// returns the result.  Both mutex poisoning and `spawn_blocking` panics are
    /// surfaced as [`DecodeError::Ffmpeg`] variants with `code = 0`.
    pub(crate) async fn with<F, R>(&self, f: F) -> Result<R, DecodeError>
    where
        F: FnOnce(&mut T) -> Result<R, DecodeError> + Send + 'static,
        R: Send + 'static,
    {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().map_err(|_| DecodeError::Ffmpeg {
                code: 0,
                message: "mutex poisoned".to_string(),
            })?;
            f(&mut *guard)
        })
        .await
        .map_err(|e| DecodeError::Ffmpeg {
            code: 0,
            message: format!("spawn_blocking panicked: {e}"),
        })?
    }
}
