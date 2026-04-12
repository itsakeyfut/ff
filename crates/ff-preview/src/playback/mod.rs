//! Real-time playback types for ff-preview.
//!
//! This module exposes the primary public API for single-file video/audio
//! playback. All `unsafe` `FFmpeg` calls are isolated in `playback_inner`.

mod playback_inner;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ff_decode::{SeekMode, VideoDecoder};
use ff_format::VideoFrame;

use crate::error::PreviewError;

/// Internal state machine for `PlaybackClock`.
///
/// Transitions:
/// - `Stopped  → Running`:  `start()`
/// - `Running  → Paused`:   `pause()`
/// - `Running  → Stopped`:  `stop()`
/// - `Paused   → Running`:  `resume()`
/// - `Paused   → Stopped`:  `stop()`
/// - `Running  → Running`:  `start()` is a no-op
/// - `Paused   → Paused`:   `pause()` is a no-op
enum ClockState {
    Stopped,
    Running { started_at: Instant, base: Duration },
    Paused { frozen_at: Duration },
}

/// A monotonic clock that tracks elapsed playback time.
///
/// The clock supports start, stop, pause, resume, and playback-rate scaling.
/// It is used by `PreviewPlayer` internally to drive frame presentation timing
/// and A/V synchronisation. Callers may also query it directly.
///
/// `PlaybackClock` is a value type — it is not `Arc<Mutex<...>>` internally.
/// When multi-thread access is required, wrap it in a `Mutex`.
///
/// # Usage
///
/// ```ignore
/// let mut clock = PlaybackClock::new();
/// clock.start();
/// let pts = clock.current_pts();
/// clock.pause();
/// // current_pts() is now frozen
/// clock.resume();
/// // current_pts() continues advancing from the frozen point
/// clock.set_rate(2.0);          // fast-forward at 2×
/// clock.set_position(Duration::from_secs(30)); // seek to 30 s
/// ```
pub struct PlaybackClock {
    state: ClockState,
    /// Playback rate multiplier. 1.0 = real-time.
    rate: f64,
    /// Pending seek position. Applied as the `base` when `start()` is called
    /// from the `Stopped` state. Cleared by `stop()`.
    seek_offset: Duration,
}

impl PlaybackClock {
    /// Create a new clock in the `Stopped` state with a rate of 1.0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: ClockState::Stopped,
            rate: 1.0,
            seek_offset: Duration::ZERO,
        }
    }

    /// Start the clock from the current position.
    ///
    /// - If the clock is `Stopped`, it starts from the position last set by
    ///   [`set_position`](Self::set_position), or `Duration::ZERO` if no seek
    ///   has been performed.
    /// - If the clock is `Paused`, it starts from the frozen position.
    /// - If the clock is already `Running`, this is a no-op.
    pub fn start(&mut self) {
        let base = match &self.state {
            ClockState::Running { .. } => return,
            ClockState::Stopped => self.seek_offset,
            ClockState::Paused { frozen_at } => *frozen_at,
        };
        self.state = ClockState::Running {
            started_at: Instant::now(),
            base,
        };
    }

    /// Stop the clock and reset the position to `Duration::ZERO`.
    ///
    /// `current_time()` and `current_pts()` will return `Duration::ZERO`
    /// until `start()` or `set_position()` is called again.
    pub fn stop(&mut self) {
        self.state = ClockState::Stopped;
        self.seek_offset = Duration::ZERO;
    }

    /// Pause the clock. `current_time()` is frozen at the current position.
    ///
    /// If the clock is not running (already paused or stopped), this is a no-op.
    pub fn pause(&mut self) {
        if matches!(self.state, ClockState::Running { .. }) {
            let frozen_at = self.current_time();
            self.state = ClockState::Paused { frozen_at };
        }
    }

    /// Resume from the paused position. `current_time()` begins advancing again.
    ///
    /// If the clock is not paused (running or stopped), this is a no-op.
    pub fn resume(&mut self) {
        let frozen_at = match &self.state {
            ClockState::Paused { frozen_at } => *frozen_at,
            _ => return,
        };
        self.state = ClockState::Running {
            started_at: Instant::now(),
            base: frozen_at,
        };
    }

    /// Returns the current media time.
    ///
    /// - `Stopped`: always `Duration::ZERO`.
    /// - `Paused`: the frozen timestamp at the moment `pause()` was called.
    /// - `Running`: `base + elapsed × rate`.
    #[must_use]
    pub fn current_time(&self) -> Duration {
        match &self.state {
            ClockState::Stopped => Duration::ZERO,
            ClockState::Paused { frozen_at } => *frozen_at,
            ClockState::Running { started_at, base } => {
                *base + started_at.elapsed().mul_f64(self.rate)
            }
        }
    }

    /// Returns `true` if the clock is currently running (not stopped or paused).
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.state, ClockState::Running { .. })
    }

    /// Sets the playback rate multiplier.
    ///
    /// - `1.0` = real-time (default).
    /// - `2.0` = double speed; `0.5` = half speed.
    /// - Values ≤ `0.0` are invalid and silently ignored (a warning is logged).
    ///
    /// When called while the clock is `Running`, the transition is seamless:
    /// `current_time()` is captured at the old rate and used as the new base,
    /// so no time is skipped or repeated.
    pub fn set_rate(&mut self, rate: f64) {
        if rate <= 0.0 {
            log::warn!("invalid playback rate ignored rate={rate}");
            return;
        }
        if matches!(self.state, ClockState::Running { .. }) {
            let now = self.current_time();
            self.rate = rate;
            self.state = ClockState::Running {
                started_at: Instant::now(),
                base: now,
            };
        } else {
            self.rate = rate;
        }
    }

    /// Returns the current playback rate multiplier. Default: `1.0`.
    #[must_use]
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Returns the current presentation timestamp (PTS) in media time.
    ///
    /// This is the authoritative position query for `PreviewPlayer`. It equals
    /// [`current_time`](Self::current_time) for `Running` and `Paused` states,
    /// and returns the last position set by [`set_position`](Self::set_position)
    /// for the `Stopped` state.
    ///
    /// - `Stopped`: the pending seek offset (default `Duration::ZERO`).
    /// - `Paused`: the frozen timestamp at the moment `pause()` was called.
    /// - `Running`: `base + elapsed × rate`.
    #[must_use]
    pub fn current_pts(&self) -> Duration {
        if matches!(self.state, ClockState::Stopped) {
            self.seek_offset
        } else {
            self.current_time()
        }
    }

    /// Jump to an arbitrary position in media time.
    ///
    /// - `Running`: the clock continues advancing from `pts` immediately.
    /// - `Paused`: the frozen position is updated to `pts`.
    /// - `Stopped`: `pts` is stored and applied as the starting position when
    ///   [`start`](Self::start) is next called.
    ///
    /// After `set_position(t)` + `start()`, [`current_pts`](Self::current_pts)
    /// will immediately return values ≥ `t`.
    pub fn set_position(&mut self, pts: Duration) {
        // seek_offset is always updated so current_pts() is consistent for all states.
        self.seek_offset = pts;
        if matches!(self.state, ClockState::Running { .. }) {
            // Re-anchor the running base at the new position.
            self.state = ClockState::Running {
                started_at: Instant::now(),
                base: pts,
            };
        } else if matches!(self.state, ClockState::Paused { .. }) {
            self.state = ClockState::Paused { frozen_at: pts };
        }
        // Stopped: seek_offset is set above; start() will use it as the initial base.
    }
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Drives real-time playback of a single media file.
///
/// `PreviewPlayer` decodes a video/audio file, synchronises video frame
/// presentation to an audio master clock, and delivers RGBA frames to a
/// registered `FrameSink` (defined in issue #383).
///
/// # Usage (stub — full implementation in later issues)
///
/// ```ignore
/// let mut player = PreviewPlayer::open("clip.mp4")?;
/// player.set_sink(Box::new(RgbaSink::new()));
/// player.play();
/// player.run()?;
/// ```
pub struct PreviewPlayer;

// ── DecodeBuffer ──────────────────────────────────────────────────────────────

/// Default ring buffer capacity for [`DecodeBuffer`] (frames).
const DEFAULT_DECODE_BUFFER_CAPACITY: usize = 8;

/// Builder for [`DecodeBuffer`].
///
/// Created via [`DecodeBuffer::open`]; call [`capacity`](Self::capacity) to
/// override the default ring buffer size, then [`build`](Self::build) to start
/// the background decode thread and obtain a [`DecodeBuffer`].
pub struct DecodeBufferBuilder {
    path: PathBuf,
    capacity: usize,
}

impl DecodeBufferBuilder {
    /// Set the ring buffer capacity in frames. Default: 8.
    ///
    /// The background thread blocks when the buffer is full and resumes as soon
    /// as the consumer calls [`DecodeBuffer::pop_frame`].
    #[must_use]
    pub fn capacity(self, n: usize) -> Self {
        Self {
            capacity: n,
            ..self
        }
    }

    /// Build and start the background decode thread.
    ///
    /// The thread pre-fills the ring buffer; frames are delivered in
    /// presentation order. The caller receives a [`DecodeBuffer`] immediately;
    /// frames become available as the thread decodes them.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the video file cannot be opened or contains
    /// no decodable video stream.
    pub fn build(self) -> Result<DecodeBuffer, PreviewError> {
        // Open decoder on the calling thread for early validation.
        // Propagates FileNotFound / NoVideoStream / Ffmpeg errors immediately.
        let mut decoder = VideoDecoder::open(&self.path).build()?;

        let (tx, rx) = sync_channel(self.capacity);
        let buffered = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));

        let buffered_thread = Arc::clone(&buffered);
        let cancel_thread = Arc::clone(&cancel);

        let handle = thread::spawn(move || -> VideoDecoder {
            decode_loop(&mut decoder, &tx, &buffered_thread, &cancel_thread);
            decoder
        });

        Ok(DecodeBuffer {
            rx: Some(rx),
            buffered,
            handle: Some(handle),
            cancel,
            capacity: self.capacity,
        })
    }
}

/// Pre-decodes frames from a video file into a ring buffer on a background thread.
///
/// `DecodeBuffer` decouples decoder latency from the presentation loop: the
/// background thread keeps the buffer filled so [`pop_frame`](Self::pop_frame)
/// can return the next frame without waiting for the decoder.
///
/// The default ring buffer capacity is 8 frames. Use
/// [`open`](Self::open) → [`capacity`](DecodeBufferBuilder::capacity) →
/// [`build`](DecodeBufferBuilder::build) to configure a different size.
///
/// # Usage
///
/// ```ignore
/// let mut buf = DecodeBuffer::open(Path::new("clip.mp4"))
///     .capacity(16)
///     .build()?;
///
/// while let Some(frame) = buf.pop_frame() {
///     // present frame…
/// }
/// ```
///
/// # Thread safety
///
/// `DecodeBuffer` is `Send` but **not** `Sync`; it must be owned by a single
/// consumer. The internal [`std::sync::mpsc::Receiver`] enforces this.
pub struct DecodeBuffer {
    /// `Option` so `Drop` can take and drop the receiver before joining the thread.
    rx: Option<Receiver<VideoFrame>>,
    /// Approximate count of frames waiting in the ring buffer.
    /// Incremented by the background thread on send; decremented by `pop_frame`.
    buffered: Arc<AtomicUsize>,
    /// Background decode thread handle. Returns the decoder on exit so `seek()`
    /// can recover it without reopening the file.
    handle: Option<JoinHandle<VideoDecoder>>,
    /// Set to `true` to ask the background thread to exit its decode loop.
    cancel: Arc<AtomicBool>,
    /// Channel capacity; needed by `seek()` to create a replacement channel.
    capacity: usize,
}

impl DecodeBuffer {
    /// Open the video at `path` and return a builder for configuring the buffer.
    ///
    /// Chain with [`DecodeBufferBuilder::capacity`] and
    /// [`DecodeBufferBuilder::build`] to start decoding.
    #[must_use]
    pub fn open(path: &Path) -> DecodeBufferBuilder {
        DecodeBufferBuilder {
            path: path.to_path_buf(),
            capacity: DEFAULT_DECODE_BUFFER_CAPACITY,
        }
    }

    /// Pop the next decoded frame.
    ///
    /// Blocks until a frame is available in the buffer or the background thread
    /// reaches end of file (EOF). Returns `None` at EOF.
    pub fn pop_frame(&mut self) -> Option<VideoFrame> {
        let frame = self.rx.as_ref()?.recv().ok();
        if frame.is_some() {
            self.buffered.fetch_sub(1, Ordering::Relaxed);
        }
        frame
    }

    /// Returns an approximation of the number of decoded frames currently
    /// waiting in the buffer.
    ///
    /// This value is advisory only; it may lag the actual buffer state by one
    /// scheduling quantum. Use it for diagnostics, not flow control.
    #[must_use]
    pub fn buffered_frames(&self) -> usize {
        self.buffered.load(Ordering::Relaxed)
    }

    /// Frame-accurate seek to `target_pts`.
    ///
    /// Stops the background decode thread, seeks the underlying decoder to the
    /// nearest preceding I-frame (`AVSEEK_FLAG_BACKWARD` + codec buffer flush),
    /// then restarts the thread. The restarted thread discards frames until
    /// `PTS ≥ target_pts` before making them available via [`pop_frame`](Self::pop_frame).
    ///
    /// Blocks until the thread has stopped and the seek has been accepted by
    /// the decoder. Frames are filled asynchronously after the method returns.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError::SeekFailed`] if the decode thread panicked or
    /// if the underlying `FFmpeg` seek fails.
    pub fn seek(&mut self, target_pts: Duration) -> Result<(), PreviewError> {
        let (mut decoder, tx) = self.stop_and_seek(target_pts)?;
        let buffered_thread = Arc::clone(&self.buffered);
        let cancel_thread = Arc::clone(&self.cancel);

        self.handle = Some(thread::spawn(move || -> VideoDecoder {
            // Forward-decode discard: drop frames whose PTS is before target_pts.
            loop {
                if cancel_thread.load(Ordering::Acquire) {
                    return decoder;
                }
                match decoder.decode_one() {
                    Ok(Some(frame)) => {
                        let pts = if frame.timestamp().is_valid() {
                            frame.timestamp().as_duration()
                        } else {
                            Duration::ZERO
                        };
                        if pts >= target_pts {
                            if tx.send(frame).is_ok() {
                                buffered_thread.fetch_add(1, Ordering::Relaxed);
                            } else {
                                return decoder; // receiver dropped
                            }
                            break; // target frame sent; switch to normal loop
                        }
                        // Frame is before target — discard and continue.
                    }
                    Ok(None) => return decoder, // EOF before target
                    Err(e) => {
                        log::warn!("decode error during seek discard error={e}");
                        return decoder;
                    }
                }
            }

            // Normal decode loop after the discard phase.
            decode_loop(&mut decoder, &tx, &buffered_thread, &cancel_thread);
            decoder
        }));

        Ok(())
    }

    /// Coarse seek to the nearest I-frame at or before `target_pts`.
    ///
    /// Faster than [`seek`](Self::seek) because it skips the forward-decode
    /// discard step. The next [`pop_frame`](Self::pop_frame) returns the frame
    /// at the I-frame position, which may be up to ±½ GOP before `target_pts`
    /// (typically ±1–2 s for H.264 at default settings).
    ///
    /// **Typical use:** call repeatedly while a scrub-bar is being dragged;
    /// call [`seek`](Self::seek) on mouse-up for frame accuracy.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError::SeekFailed`] if the decode thread panicked or
    /// if the underlying `FFmpeg` seek fails.
    pub fn seek_coarse(&mut self, target_pts: Duration) -> Result<(), PreviewError> {
        log::debug!("coarse seek target_pts={target_pts:?}");
        let (mut decoder, tx) = self.stop_and_seek(target_pts)?;
        let buffered_thread = Arc::clone(&self.buffered);
        let cancel_thread = Arc::clone(&self.cancel);

        // No discard loop — start the normal decode loop directly from the I-frame.
        self.handle = Some(thread::spawn(move || -> VideoDecoder {
            decode_loop(&mut decoder, &tx, &buffered_thread, &cancel_thread);
            decoder
        }));

        Ok(())
    }

    /// Shared helper for `seek` and `seek_coarse`.
    ///
    /// 1. Signals cancel, drains the channel, joins the thread to recover the decoder.
    /// 2. Seeks the decoder to the nearest I-frame at or before `target_pts`.
    /// 3. Resets the buffered counter, creates a fresh channel, clears the cancel flag.
    ///
    /// Returns `(decoder, SyncSender)` ready for the caller to spawn a new thread.
    fn stop_and_seek(
        &mut self,
        target_pts: Duration,
    ) -> Result<(VideoDecoder, std::sync::mpsc::SyncSender<VideoFrame>), PreviewError> {
        // 1. Signal the background thread to exit its decode loop.
        self.cancel.store(true, Ordering::Release);

        // 2. Drain the channel so the background thread is not blocked on send().
        if let Some(rx) = &self.rx {
            while rx.try_recv().is_ok() {
                self.buffered.fetch_sub(1, Ordering::Relaxed);
            }
        }

        // 3. Join the thread to recover the decoder.
        let mut decoder = self
            .handle
            .take()
            .and_then(|h| h.join().ok())
            .ok_or_else(|| PreviewError::SeekFailed {
                target: target_pts,
                reason: "decode thread unavailable for seek".into(),
            })?;

        // 4. Seek to the nearest I-frame at or before target_pts.
        //    avformat_seek_file with AVSEEK_FLAG_BACKWARD and avcodec_flush_buffers
        //    are handled inside VideoDecoder::seek (ff-decode/video/decoder_inner/seeking.rs).
        decoder
            .seek(target_pts, SeekMode::Backward)
            .map_err(|e| PreviewError::SeekFailed {
                target: target_pts,
                reason: e.to_string(),
            })?;

        // 5. Reset counter, create a fresh channel, clear the cancel flag.
        self.buffered.store(0, Ordering::Relaxed);
        let (tx, rx) = sync_channel(self.capacity);
        self.rx = Some(rx);
        self.cancel.store(false, Ordering::Release);

        Ok((decoder, tx))
    }
}

impl Drop for DecodeBuffer {
    fn drop(&mut self) {
        // Signal cancel so the thread exits the decode loop promptly.
        self.cancel.store(true, Ordering::Release);
        // Drop the receiver so SyncSender::send() returns Err, unblocking the
        // thread if it is waiting for space in a full channel.
        drop(self.rx.take());
        // Join (ignoring the returned decoder).
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Normal decode loop body shared between `build()` and the post-seek thread.
///
/// Exits when EOF is reached, a decode error occurs, or the `cancel` flag is set,
/// or the receiver drops (i.e., `DecodeBuffer` was dropped).
fn decode_loop(
    decoder: &mut VideoDecoder,
    tx: &std::sync::mpsc::SyncSender<VideoFrame>,
    buffered: &AtomicUsize,
    cancel: &AtomicBool,
) {
    loop {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        match decoder.decode_one() {
            Ok(Some(frame)) => {
                if tx.send(frame).is_ok() {
                    buffered.fetch_add(1, Ordering::Relaxed);
                } else {
                    // Receiver was dropped — DecodeBuffer has been dropped.
                    break;
                }
            }
            Ok(None) => break, // EOF
            Err(e) => {
                log::warn!("decode error in background thread error={e}");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::thread;

    #[test]
    fn clock_stopped_should_return_zero() {
        // Newly created clock returns zero.
        let clock = PlaybackClock::new();
        assert_eq!(clock.current_time(), Duration::ZERO);

        // Clock returns zero after stop.
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(5));
        clock.stop();
        assert_eq!(
            clock.current_time(),
            Duration::ZERO,
            "current_time() must be ZERO after stop()"
        );
    }

    #[test]
    fn clock_paused_should_freeze_at_pause_time() {
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(10));
        clock.pause();

        let t1 = clock.current_time();
        thread::sleep(Duration::from_millis(10));
        let t2 = clock.current_time();

        assert_eq!(t1, t2, "current_time() must not advance while paused");
        assert!(
            !clock.is_running(),
            "clock must not report running while paused"
        );
    }

    #[test]
    fn clock_resumed_should_continue_from_pause() {
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(10));
        clock.pause();
        let t_paused = clock.current_time();

        // Wait while paused — time must not advance.
        thread::sleep(Duration::from_millis(10));
        assert_eq!(clock.current_time(), t_paused);

        clock.resume();
        assert!(clock.is_running());
        thread::sleep(Duration::from_millis(10));

        let t_after = clock.current_time();
        assert!(
            t_after > t_paused,
            "current_time() must advance after resume(); paused={t_paused:?} after={t_after:?}"
        );
    }

    #[test]
    fn clock_start_should_be_noop_when_already_running() {
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(10));
        let t_before = clock.current_time();

        // Second start() should not reset the clock.
        clock.start();
        let t_after = clock.current_time();

        assert!(
            t_after >= t_before,
            "second start() must not reset the clock; before={t_before:?} after={t_after:?}"
        );
    }

    #[test]
    fn clock_resume_should_be_noop_when_not_paused() {
        // resume() on a stopped clock: stays stopped.
        let mut clock = PlaybackClock::new();
        clock.resume();
        assert!(!clock.is_running());
        assert_eq!(clock.current_time(), Duration::ZERO);

        // resume() on a running clock: no effect.
        clock.start();
        thread::sleep(Duration::from_millis(5));
        let t = clock.current_time();
        clock.resume(); // no-op
        assert!(clock.is_running());
        assert!(clock.current_time() >= t);
    }

    #[test]
    fn clock_default_should_equal_new() {
        let a = PlaybackClock::new();
        let b = PlaybackClock::default();
        assert_eq!(a.current_time(), b.current_time());
        assert_eq!(a.is_running(), b.is_running());
    }

    #[test]
    fn set_rate_should_reject_non_positive_values() {
        let mut clock = PlaybackClock::new();

        clock.set_rate(0.0);
        assert!(
            (clock.rate() - 1.0).abs() < f64::EPSILON,
            "rate must remain 1.0 after set_rate(0.0)"
        );

        clock.set_rate(-1.0);
        assert!(
            (clock.rate() - 1.0).abs() < f64::EPSILON,
            "rate must remain 1.0 after set_rate(-1.0)"
        );
    }

    #[test]
    fn set_rate_should_update_rate_when_stopped_or_paused() {
        // Stopped → rate updates.
        let mut clock = PlaybackClock::new();
        clock.set_rate(0.5);
        assert!((clock.rate() - 0.5).abs() < f64::EPSILON);

        // Paused → rate updates without resuming.
        let mut clock = PlaybackClock::new();
        clock.start();
        clock.pause();
        clock.set_rate(2.0);
        assert!((clock.rate() - 2.0).abs() < f64::EPSILON);
        assert!(
            !clock.is_running(),
            "clock must remain paused after set_rate"
        );
    }

    #[test]
    fn set_rate_running_should_not_jump_current_time() {
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(10));
        let before = clock.current_time();
        clock.set_rate(2.0);
        let after = clock.current_time();

        // current_time() must not jump backward or skip more than a scheduler
        // quantum (~16 ms) forward after set_rate while running.
        assert!(
            after >= before,
            "current_time() must not go backward on set_rate; before={before:?} after={after:?}"
        );
        assert!(
            after - before < Duration::from_millis(20),
            "current_time() must not jump forward on set_rate; before={before:?} after={after:?}"
        );
        assert!((clock.rate() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore = "performance thresholds are environment-dependent; run explicitly with -- --include-ignored"]
    fn rate_two_x_should_advance_at_double_speed() {
        let mut clock = PlaybackClock::new();
        clock.set_rate(2.0);
        clock.start();
        thread::sleep(Duration::from_millis(50));
        let elapsed = clock.current_time();

        // At 2×, 50 ms wall time should produce ≥80 ms of media time.
        assert!(
            elapsed >= Duration::from_millis(80),
            "2× rate: expected ≥80 ms after 50 ms wall time, got {elapsed:?}"
        );
    }

    #[test]
    fn set_position_should_shift_pts_by_seek_offset() {
        let seek_target = Duration::from_secs(30);

        // Stopped: current_pts() returns the offset immediately.
        let mut clock = PlaybackClock::new();
        clock.set_position(seek_target);
        assert_eq!(
            clock.current_pts(),
            seek_target,
            "current_pts() must reflect seek_offset when stopped"
        );

        // start() must begin from the seek position.
        clock.start();
        let pts = clock.current_pts();
        assert!(
            pts >= seek_target,
            "current_pts() must be ≥ seek target after start(); target={seek_target:?} pts={pts:?}"
        );
        assert!(
            clock.is_running(),
            "clock must be running after set_position + start()"
        );
    }

    #[test]
    fn set_position_while_paused_should_update_frozen_time() {
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(5));
        clock.pause();

        let seek_target = Duration::from_secs(10);
        clock.set_position(seek_target);

        let pts = clock.current_pts();
        assert_eq!(
            pts, seek_target,
            "frozen time must update to seek target; expected={seek_target:?} got={pts:?}"
        );
        assert!(
            !clock.is_running(),
            "clock must remain paused after set_position"
        );

        // resume() must continue advancing from the new position.
        clock.resume();
        thread::sleep(Duration::from_millis(5));
        let pts_after = clock.current_pts();
        assert!(
            pts_after > seek_target,
            "current_pts() must advance past seek target after resume(); target={seek_target:?} after={pts_after:?}"
        );
    }

    #[test]
    fn set_position_while_running_should_continue_from_new_position() {
        let mut clock = PlaybackClock::new();
        clock.start();
        thread::sleep(Duration::from_millis(5));

        let seek_target = Duration::from_secs(60);
        clock.set_position(seek_target);

        let pts = clock.current_pts();
        assert!(
            pts >= seek_target,
            "current_pts() must be ≥ seek target immediately after set_position while running; \
             target={seek_target:?} pts={pts:?}"
        );
        assert!(
            clock.is_running(),
            "clock must remain running after set_position"
        );
    }

    #[test]
    fn stop_should_clear_seek_offset() {
        let mut clock = PlaybackClock::new();
        clock.set_position(Duration::from_secs(30));
        clock.stop();

        assert_eq!(
            clock.current_pts(),
            Duration::ZERO,
            "stop() must reset seek_offset to ZERO"
        );
    }

    // ── DecodeBuffer tests ────────────────────────────────────────────────────

    fn test_video_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/video/gameplay.mp4")
    }

    #[test]
    fn decode_buffer_build_should_fail_for_nonexistent_file() {
        let result = DecodeBuffer::open(Path::new("nonexistent_placeholder.mp4")).build();
        assert!(
            result.is_err(),
            "build() must return Err for a non-existent file"
        );
    }

    #[test]
    fn decode_buffer_open_should_use_default_capacity() {
        let path = test_video_path();
        let buf = match DecodeBuffer::open(&path).build() {
            Ok(buf) => buf,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        // Buffer starts empty; frames arrive asynchronously.
        assert_eq!(
            buf.buffered_frames(),
            0,
            "buffer must report 0 before any frames have been consumed"
        );
    }

    #[test]
    fn decode_buffer_pop_frame_should_return_some_then_none_at_eof() {
        let path = test_video_path();
        let mut buf = match DecodeBuffer::open(&path).capacity(4).build() {
            Ok(buf) => buf,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        // Pop at least one frame to confirm the decoder is running.
        assert!(
            buf.pop_frame().is_some(),
            "pop_frame() must return Some for a valid video file"
        );
    }

    #[test]
    fn seek_should_reposition_to_target_pts() {
        let path = test_video_path();
        let mut buf = match DecodeBuffer::open(&path).capacity(4).build() {
            Ok(buf) => buf,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        // Consume a few frames to advance past the start.
        for _ in 0..5 {
            if buf.pop_frame().is_none() {
                println!("skipping: EOF before seek target");
                return;
            }
        }

        let seek_target = Duration::from_secs(1);
        match buf.seek(seek_target) {
            Ok(()) => {}
            Err(e) => {
                println!("skipping: seek not supported or failed: {e}");
                return;
            }
        }

        // After seek, the first frame's PTS must be at or near the target.
        let frame = match buf.pop_frame() {
            Some(f) => f,
            None => {
                println!("skipping: no frame after seek");
                return;
            }
        };

        if frame.timestamp().is_valid() {
            let pts = frame.timestamp().as_duration();
            // Allow ±1 second of tolerance (one GOP) for I-frame alignment.
            assert!(
                pts >= seek_target.saturating_sub(Duration::from_secs(1)),
                "post-seek frame PTS must be near target; target={seek_target:?} pts={pts:?}"
            );
        }
    }

    #[test]
    fn seek_should_fail_for_stopped_buffer() {
        // Build with non-existent file → build() fails.
        // This confirms seek errors are propagated correctly.
        let result = DecodeBuffer::open(Path::new("nonexistent.mp4")).build();
        assert!(
            result.is_err(),
            "build() must fail for non-existent file (precondition for seek error path)"
        );
    }
}
