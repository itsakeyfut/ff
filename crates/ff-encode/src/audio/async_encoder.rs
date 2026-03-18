//! Async audio encoder backed by a bounded `tokio::sync::mpsc` channel.

use ff_format::AudioFrame;
use tokio::sync::mpsc;

use super::builder::{AudioEncoder, AudioEncoderBuilder};
use crate::EncodeError;

/// Messages sent from the async front-end to the worker thread.
enum WorkerMsg {
    Frame(AudioFrame),
}

/// Async wrapper around [`AudioEncoder`].
///
/// Frames are queued into a bounded channel (capacity 8) and encoded by a
/// dedicated worker thread. When the channel is full, [`push`] suspends the
/// caller, providing natural back-pressure.
///
/// # Construction
///
/// Use [`AudioEncoder::create`] to configure the encoder, then call
/// [`AsyncAudioEncoder::from_builder`]:
///
/// ```ignore
/// use ff_encode::{AsyncAudioEncoder, AudioEncoder, AudioCodec};
///
/// let mut encoder = AsyncAudioEncoder::from_builder(
///     AudioEncoder::create("output.m4a")
///         .audio(48000, 2)
///         .audio_codec(AudioCodec::Aac),
/// )?;
///
/// encoder.push(frame).await?;
/// encoder.finish().await?;
/// ```
///
/// # Back-pressure
///
/// The internal channel holds at most 8 frames. Once that buffer is full,
/// [`push`] yields until the worker drains a slot. This prevents unbounded
/// memory growth when the encoder cannot keep up with the incoming frame rate.
///
/// [`push`]: AsyncAudioEncoder::push
pub struct AsyncAudioEncoder {
    sender: mpsc::Sender<WorkerMsg>,
    join_handle: Option<std::thread::JoinHandle<Result<(), EncodeError>>>,
}

impl std::fmt::Debug for AsyncAudioEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncAudioEncoder").finish_non_exhaustive()
    }
}

impl AsyncAudioEncoder {
    /// Builds an async encoder from a configured builder.
    ///
    /// Consumes the builder, validates the configuration, opens the output
    /// file, and starts the worker thread. The worker runs the synchronous
    /// FFmpeg encode loop in the background.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if the builder configuration is invalid or
    /// the output file cannot be created.
    pub fn from_builder(builder: AudioEncoderBuilder) -> Result<Self, EncodeError> {
        let encoder = builder.build()?;
        let (tx, rx) = mpsc::channel::<WorkerMsg>(8);

        let handle = std::thread::spawn(move || -> Result<(), EncodeError> {
            let mut encoder: AudioEncoder = encoder;
            let mut rx = rx;
            while let Some(WorkerMsg::Frame(frame)) = rx.blocking_recv() {
                encoder.push(&frame)?;
            }
            // Channel closed (sender dropped) → flush remaining frames and write trailer.
            encoder.finish()
        });

        Ok(Self {
            sender: tx,
            join_handle: Some(handle),
        })
    }

    /// Queues an audio frame for encoding.
    ///
    /// If the internal channel (capacity 8) is full, this method suspends
    /// the caller until the worker drains a slot.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::WorkerPanicked`] if the worker thread has
    /// exited unexpectedly.
    pub async fn push(&mut self, frame: AudioFrame) -> Result<(), EncodeError> {
        self.sender
            .send(WorkerMsg::Frame(frame))
            .await
            .map_err(|_| EncodeError::WorkerPanicked)
    }

    /// Signals end-of-stream, flushes remaining frames, and writes the file trailer.
    ///
    /// Drops the channel sender (signalling EOF to the worker), then waits
    /// for the worker thread to finish without blocking the async executor.
    /// Any error from the worker is propagated back to the caller.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if encoding fails during flush or if the
    /// worker thread panicked.
    pub async fn finish(self) -> Result<(), EncodeError> {
        let Self {
            sender,
            join_handle,
        } = self;
        // Dropping the sender closes the channel; the worker's blocking_recv()
        // returns None, breaks out of the loop, and calls encoder.finish().
        drop(sender);
        if let Some(handle) = join_handle {
            // Join on a spawn_blocking thread so the async executor is not blocked.
            tokio::task::spawn_blocking(move || {
                handle.join().map_err(|_| EncodeError::WorkerPanicked)?
            })
            .await
            .map_err(|_| EncodeError::WorkerPanicked)?
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time proof that AsyncAudioEncoder satisfies Send.
    fn _assert_send() {
        fn is_send<T: Send>() {}
        is_send::<AsyncAudioEncoder>();
    }

    #[test]
    fn from_builder_should_fail_on_invalid_config() {
        // A builder with no streams configured is rejected at build time,
        // not in the worker thread — the error surfaces from from_builder.
        let result = AsyncAudioEncoder::from_builder(AudioEncoder::create("out.m4a"));
        assert!(
            result.is_err(),
            "expected error for unconfigured builder, got Ok"
        );
        assert!(
            matches!(result.unwrap_err(), EncodeError::InvalidConfig { .. }),
            "expected InvalidConfig"
        );
    }
}
