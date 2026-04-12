//! Real-time playback types for ff-preview.
//!
//! This module exposes the primary public API for single-file video/audio
//! playback. All `unsafe` `FFmpeg` calls are isolated in `playback_inner`.

mod playback_inner;

use std::time::{Duration, Instant};

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
/// # Usage (stub — full implementation in #370 / #371)
///
/// ```ignore
/// let mut clock = PlaybackClock::new();
/// clock.start();
/// let pts = clock.current_time();
/// clock.pause();
/// // current_time() is now frozen
/// clock.resume();
/// // current_time() continues advancing from the frozen point
/// ```
pub struct PlaybackClock {
    state: ClockState,
    /// Playback rate multiplier. 1.0 = real-time. Set via `set_rate` (#371).
    rate: f64,
}

impl PlaybackClock {
    /// Create a new clock in the `Stopped` state with a rate of 1.0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: ClockState::Stopped,
            rate: 1.0,
        }
    }

    /// Start the clock from the current position.
    ///
    /// - If the clock is `Stopped`, it starts from `Duration::ZERO`.
    /// - If the clock is `Paused`, it starts from the frozen position.
    /// - If the clock is already `Running`, this is a no-op.
    pub fn start(&mut self) {
        let base = match &self.state {
            ClockState::Running { .. } => return,
            ClockState::Stopped => Duration::ZERO,
            ClockState::Paused { frozen_at } => *frozen_at,
        };
        self.state = ClockState::Running {
            started_at: Instant::now(),
            base,
        };
    }

    /// Stop the clock and reset the position to `Duration::ZERO`.
    ///
    /// `current_time()` will return `Duration::ZERO` until `start()` is called.
    pub fn stop(&mut self) {
        self.state = ClockState::Stopped;
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
