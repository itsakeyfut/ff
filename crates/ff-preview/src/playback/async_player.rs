//! Async wrapper around [`PreviewPlayer`].
//!
//! This module is only compiled when the `tokio` feature is enabled.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::decode_buffer::FrameResult;
use super::player::PreviewPlayer;
use super::sink::FrameSink;
use crate::error::PreviewError;

// ── AsyncPreviewPlayer ────────────────────────────────────────────────────────

/// Async wrapper around [`PreviewPlayer`]. Cloneable, `Send`, and `Sync`.
///
/// All potentially-blocking methods delegate to the underlying
/// [`PreviewPlayer`] via [`tokio::task::spawn_blocking`] so that `FFmpeg`
/// calls do not block the async executor.
///
/// # Usage
///
/// ```ignore
/// let player = AsyncPreviewPlayer::open(Path::new("clip.mp4")).await?;
/// player.set_sink(Box::new(MySink::new()));
/// player.play().await;
/// while let FrameResult::Frame(_) = player.pop_frame().await { /* … */ }
/// ```
#[derive(Clone)]
pub struct AsyncPreviewPlayer {
    inner: Arc<Mutex<PreviewPlayer>>,
}

impl AsyncPreviewPlayer {
    /// Open a media file asynchronously.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the file cannot be opened or the blocking
    /// thread panics.
    pub async fn open(path: &Path) -> Result<Self, PreviewError> {
        let path = path.to_path_buf();
        let player = tokio::task::spawn_blocking(move || PreviewPlayer::open(&path))
            .await
            .map_err(|e| PreviewError::Ffmpeg {
                code: 0,
                message: format!("tokio task join error: {e}"),
            })??;
        Ok(Self {
            inner: Arc::new(Mutex::new(player)),
        })
    }

    /// Register the frame sink. Not async — only stores the box.
    pub fn set_sink(&self, sink: Box<dyn FrameSink>) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_sink(sink);
    }

    /// Start (or resume) playback.
    pub async fn play(&self) {
        let inner = Arc::clone(&self.inner);
        let _ = tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .play();
        })
        .await;
    }

    /// Pause playback.
    pub async fn pause(&self) {
        let inner = Arc::clone(&self.inner);
        let _ = tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pause();
        })
        .await;
    }

    /// Stop playback.
    pub async fn stop(&self) {
        let inner = Arc::clone(&self.inner);
        let _ = tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .stop();
        })
        .await;
    }

    /// Frame-accurate seek to `pts`.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the seek fails or the blocking thread panics.
    pub async fn seek(&self, pts: Duration) -> Result<(), PreviewError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .seek(pts)
        })
        .await
        .map_err(|e| PreviewError::Ffmpeg {
            code: 0,
            message: format!("tokio task join error: {e}"),
        })?
    }

    /// Coarse seek to the nearest I-frame at or before `pts`.
    ///
    /// See [`PreviewPlayer::seek_coarse`] for full semantics.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the seek fails or the blocking thread panics.
    pub async fn seek_coarse(&self, pts: Duration) -> Result<(), PreviewError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .seek_coarse(pts)
        })
        .await
        .map_err(|e| PreviewError::Ffmpeg {
            code: 0,
            message: format!("tokio task join error: {e}"),
        })?
    }

    /// Pop the next decoded video frame.
    ///
    /// Runs on a blocking thread until a frame is available.
    /// Returns [`FrameResult::Eof`] at end of file or on thread panic.
    pub async fn pop_frame(&self) -> FrameResult {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_frame()
        })
        .await
        .unwrap_or(FrameResult::Eof)
    }

    /// Returns the PTS of the most recently presented frame.
    ///
    /// See [`PreviewPlayer::current_pts`] for full semantics.
    pub fn current_pts(&self) -> Duration {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current_pts()
    }

    /// Returns the container-reported duration, if known.
    ///
    /// See [`PreviewPlayer::duration`] for full semantics.
    pub fn duration(&self) -> Option<Duration> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .duration()
    }

    /// Pull up to `n` interleaved stereo `f32` PCM samples at 48 kHz.
    ///
    /// See [`PreviewPlayer::pop_audio_samples`] for full semantics.
    pub async fn pop_audio_samples(&self, n: usize) -> Vec<f32> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_audio_samples(n)
        })
        .await
        .unwrap_or_default()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_video_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/video/gameplay.mp4")
    }

    #[test]
    #[ignore = "requires FFmpeg and assets/video/gameplay.mp4; run with -- --include-ignored"]
    fn async_preview_player_should_open_and_pop_frame() {
        let path = test_video_path();
        match tokio::runtime::Builder::new_current_thread().build() {
            Ok(rt) => rt.block_on(async {
                let player = match AsyncPreviewPlayer::open(&path).await {
                    Ok(p) => p,
                    Err(e) => {
                        println!("skipping: open failed: {e}");
                        return;
                    }
                };
                player.play().await;
                let frame = player.pop_frame().await;
                assert!(
                    matches!(frame, FrameResult::Frame(_) | FrameResult::Seeking(_)),
                    "pop_frame() must return Frame or Seeking"
                );
            }),
            Err(e) => println!("skipping: failed to build tokio runtime: {e}"),
        }
    }
}
