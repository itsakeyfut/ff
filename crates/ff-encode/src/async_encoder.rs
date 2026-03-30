//! Generic async encoder helper backed by a bounded `tokio::sync::mpsc` channel.

use tokio::sync::mpsc;

use crate::EncodeError;

/// Abstraction over a synchronous encoder that accepts frames of type `F`.
///
/// Implement this trait on a synchronous encoder to make it usable with
/// [`AsyncEncoder`].  Both [`VideoEncoder`] and [`AudioEncoder`] implement it.
///
/// [`VideoEncoder`]: crate::video::VideoEncoder
/// [`AudioEncoder`]: crate::audio::AudioEncoder
pub(crate) trait SyncEncoder<F> {
    /// Push one frame into the encoder.
    fn push_frame(&mut self, frame: &F) -> Result<(), EncodeError>;

    /// Flush all buffered frames and write the container trailer.
    fn drain_and_finish(self) -> Result<(), EncodeError>;
}

/// Messages sent from the async front-end to the worker thread.
enum WorkerMsg<F> {
    Frame(F),
    Finish,
}

/// Generic async wrapper for a synchronous encoder `E` that encodes frames of
/// type `F`.
///
/// Frames are queued into a bounded channel (capacity 8) and encoded by a
/// dedicated worker thread.  When the channel is full, [`push`] suspends the
/// caller, providing natural back-pressure.
///
/// This is a crate-internal helper used by [`AsyncVideoEncoder`] and
/// [`AsyncAudioEncoder`]; it is not part of the public API.
///
/// [`push`]: AsyncEncoder::push
/// [`AsyncVideoEncoder`]: crate::video::AsyncVideoEncoder
/// [`AsyncAudioEncoder`]: crate::audio::AsyncAudioEncoder
pub(crate) struct AsyncEncoder<F> {
    sender: mpsc::Sender<WorkerMsg<F>>,
    join_handle: Option<std::thread::JoinHandle<Result<(), EncodeError>>>,
}

impl<F: Send + 'static> AsyncEncoder<F> {
    /// Wraps an already-opened synchronous encoder and starts the worker thread.
    pub(crate) fn new<E>(encoder: E) -> Self
    where
        E: SyncEncoder<F> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<WorkerMsg<F>>(8);

        let handle = std::thread::spawn(move || -> Result<(), EncodeError> {
            let mut enc = encoder;
            let mut rx = rx;
            #[allow(clippy::while_let_loop)]
            loop {
                match rx.blocking_recv() {
                    Some(WorkerMsg::Frame(frame)) => enc.push_frame(&frame)?,
                    Some(WorkerMsg::Finish) | None => break,
                }
            }
            enc.drain_and_finish()
        });

        Self {
            sender: tx,
            join_handle: Some(handle),
        }
    }

    /// Queues a frame for encoding.
    ///
    /// If the internal channel (capacity 8) is full, this method suspends
    /// the caller until the worker drains a slot.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError::WorkerPanicked`] if the worker thread has
    /// exited unexpectedly.
    pub(crate) async fn push(&mut self, frame: F) -> Result<(), EncodeError> {
        self.sender
            .send(WorkerMsg::Frame(frame))
            .await
            .map_err(|_| EncodeError::WorkerPanicked)
    }

    /// Signals end-of-stream, flushes remaining frames, and writes the file trailer.
    ///
    /// Sends the `Finish` sentinel to the worker, drops the sender to close
    /// the channel, then waits for the worker thread on a `spawn_blocking`
    /// thread so the async executor is not blocked.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] if encoding fails during flush or if the
    /// worker thread panicked.
    pub(crate) async fn finish(self) -> Result<(), EncodeError> {
        let Self {
            sender,
            join_handle,
        } = self;
        // Send the Finish sentinel so the worker exits its loop cleanly, then
        // drop the sender to close the channel.
        sender
            .send(WorkerMsg::Finish)
            .await
            .map_err(|_| EncodeError::WorkerPanicked)?;
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
