//! Playback clock types for ff-preview.
//!
//! [`PlaybackClock`] is the public wall-clock API for callers that need
//! independent timing queries. [`MasterClock`] is the crate-internal A/V sync
//! reference driven by either consumed audio samples or an `Instant`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// ── ClockState ────────────────────────────────────────────────────────────────

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

// ── PlaybackClock ─────────────────────────────────────────────────────────────

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

    /// Pause the clock at the current position.
    ///
    /// `current_time()` and `current_pts()` are frozen until
    /// [`resume`](Self::resume) is called. If already `Paused` or `Stopped`, no-op.
    pub fn pause(&mut self) {
        if let ClockState::Running { started_at, base } = &self.state {
            let elapsed = started_at.elapsed().mul_f64(self.rate);
            self.state = ClockState::Paused {
                frozen_at: *base + elapsed,
            };
        }
    }

    /// Resume from a paused position. No-op if not paused.
    pub fn resume(&mut self) {
        if let ClockState::Paused { frozen_at } = self.state {
            self.state = ClockState::Running {
                started_at: Instant::now(),
                base: frozen_at,
            };
        }
    }

    /// Current wall-clock elapsed time since start (affected by rate).
    ///
    /// Equivalent to [`current_pts`](Self::current_pts) for clocks that
    /// start at zero; use `current_pts()` when a seek offset has been set.
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

    /// Current presentation timestamp (elapsed time since position zero).
    ///
    /// Identical to `current_time()` when the clock was started from zero.
    /// When a `set_position(t)` was called before `start()`, the clock
    /// advances from `t` and this method returns values ≥ `t`.
    #[must_use]
    pub fn current_pts(&self) -> Duration {
        match &self.state {
            ClockState::Stopped => self.seek_offset,
            _ => self.current_time(),
        }
    }

    /// Returns `true` if the clock is actively advancing.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.state, ClockState::Running { .. })
    }

    /// Set the playback rate. Values ≤ 0.0 are ignored (rate stays unchanged).
    ///
    /// When the clock is `Running`, the current position is re-anchored at
    /// `Instant::now()` so that `current_time()` does not jump.
    pub fn set_rate(&mut self, rate: f64) {
        if rate <= 0.0 {
            return;
        }
        if let ClockState::Running { started_at, base } = &mut self.state {
            // Re-anchor to now so the position does not jump on rate change.
            let elapsed = started_at.elapsed().mul_f64(self.rate);
            *base += elapsed;
            *started_at = Instant::now();
        }
        self.rate = rate;
    }

    /// Current playback rate (default: 1.0).
    #[must_use]
    pub fn rate(&self) -> f64 {
        self.rate
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

// ── MasterClock ───────────────────────────────────────────────────────────────

/// Reference clock for the A/V sync loop in [`PreviewPlayer::run`].
///
/// - `Audio`: driven by consumed audio samples ÷ `sample_rate`.
/// - `System`: driven by [`std::time::Instant`] (video-only files).
pub(crate) enum MasterClock {
    Audio {
        samples_consumed: Arc<AtomicU64>,
        sample_rate: u32,
        /// Wall-clock fallback activated after the first presented frame when no
        /// audio consumer has called `pop_audio_samples()`. Tuple: `(wall start, base PTS)`.
        ///
        /// When `Some`, `current_pts()` returns `base_pts + elapsed` instead of
        /// `Duration::ZERO`, so video pacing runs at real time even without a cpal
        /// consumer. If `samples_consumed` becomes non-zero later (a consumer
        /// connects mid-playback), `current_pts()` automatically switches to the
        /// audio-clock path with no additional coordination.
        fallback: Option<(Instant, Duration)>,
    },
    System {
        started_at: Instant,
        base_pts: Duration,
    },
}

impl MasterClock {
    /// Current master clock position.
    ///
    /// For `Audio`: returns the maximum of the sample-based clock and the
    /// wall-clock fallback (when set). Taking the maximum ensures that the
    /// clock continues advancing at wall-clock rate after the audio ring
    /// buffer drains (audio track ends before video), while also allowing
    /// a late-connecting cpal consumer to drive the clock forward once it
    /// overtakes the initial fallback.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn current_pts(&self) -> Duration {
        match self {
            Self::Audio {
                samples_consumed,
                sample_rate,
                fallback,
            } => {
                let s = samples_consumed.load(Ordering::Relaxed);
                let sample_pts = if s > 0 {
                    Some(Duration::from_secs_f64(s as f64 / f64::from(*sample_rate)))
                } else {
                    None
                };
                let fallback_pts = fallback
                    .as_ref()
                    .map(|(started_at, base_pts)| *base_pts + started_at.elapsed());
                match (sample_pts, fallback_pts) {
                    // Both present: use whichever is further ahead.
                    // - During normal playback the sample clock is ahead → sample wins.
                    // - After audio EOF (samples frozen) the wall-clock fallback
                    //   overtakes → fallback wins.
                    (Some(sp), Some(fp)) => sp.max(fp),
                    (Some(sp), None) => sp,
                    (None, Some(fp)) => fp,
                    (None, None) => Duration::ZERO,
                }
            }
            Self::System {
                started_at,
                base_pts,
            } => *base_pts + started_at.elapsed(),
        }
    }

    /// Whether A/V sync should be applied for the current frame.
    ///
    /// - `System`: always `true` — wall clock drives FPS pacing.
    /// - `Audio`: `true` once any of the following holds:
    ///   - `samples_consumed > 0` (a cpal consumer has called `pop_audio_samples`), or
    ///   - `fallback.is_some()` (the wall-clock fallback was armed after the first frame).
    ///
    ///   Returns `false` only in the brief window between `run()` starting and the
    ///   first frame being presented — this prevents an indefinite sleep before any
    ///   clock reference is available.
    pub(crate) fn should_sync(&self) -> bool {
        match self {
            Self::System { .. } => true,
            Self::Audio {
                samples_consumed,
                fallback,
                ..
            } => samples_consumed.load(Ordering::Relaxed) > 0 || fallback.is_some(),
        }
    }

    /// Activate the wall-clock fallback at `base_pts` if no audio samples have
    /// been consumed yet and the fallback has not already been armed.
    ///
    /// Called by [`PlayerRunner::run`] immediately after the first
    /// `present_frame()` call. Once armed, `should_sync()` returns `true` and
    /// `current_pts()` advances in real time even when no cpal consumer is
    /// connected.
    ///
    /// Idempotent: subsequent calls are no-ops.  If `samples_consumed` becomes
    /// non-zero (a consumer connects mid-playback), `current_pts()` automatically
    /// switches to the audio-clock path without any additional coordination.
    ///
    /// No-op for [`MasterClock::System`].
    pub(crate) fn activate_fallback_if_no_audio(&mut self, base_pts: Duration) {
        if let Self::Audio {
            samples_consumed,
            fallback,
            ..
        } = self
            && samples_consumed.load(Ordering::Relaxed) == 0
            && fallback.is_none()
        {
            *fallback = Some((Instant::now(), base_pts));
        }
    }

    /// Re-arm the wall-clock fallback at `base_pts`, even when
    /// `samples_consumed > 0`.
    ///
    /// Unlike [`activate_fallback_if_no_audio`](Self::activate_fallback_if_no_audio),
    /// this method activates unconditionally and is intended to be called by
    /// the pacing loop when it detects that audio has gone silent (audio track
    /// ended before video). After re-arming, [`current_pts`](Self::current_pts)
    /// returns the `max` of the frozen sample position and the advancing
    /// wall-clock, so video continues at its native frame rate.
    ///
    /// No-op for [`MasterClock::System`].
    pub(crate) fn rearm_fallback_at(&mut self, base_pts: Duration) {
        if let Self::Audio { fallback, .. } = self {
            *fallback = Some((Instant::now(), base_pts));
        }
    }

    /// Current value of the audio sample counter, or `0` for a `System` clock.
    ///
    /// Used by the pacing loop to detect stalls: if this value stops
    /// advancing for several consecutive frames while `> 0`, the audio track
    /// has ended and `rearm_fallback_at` should be called.
    pub(crate) fn audio_samples_snapshot(&self) -> u64 {
        if let Self::Audio {
            samples_consumed, ..
        } = self
        {
            samples_consumed.load(Ordering::Relaxed)
        } else {
            0
        }
    }

    /// Reset the clock to start ticking from `base` right now.
    ///
    /// For [`MasterClock::System`]: re-anchors `started_at` and sets `base_pts`.
    ///
    /// For [`MasterClock::Audio`]: if the wall-clock fallback is active (i.e. no
    /// audio consumer is present), re-anchors the fallback at `(Instant::now(), base)`
    /// so that post-seek pacing starts from the correct position. If the fallback
    /// is not yet armed (pre-first-frame) or if `samples_consumed > 0` (audio
    /// consumer active), this is a no-op — the seek position is reflected in the
    /// audio buffer restart performed by `restart_audio_from`.
    pub(crate) fn reset(&mut self, base: Duration) {
        match self {
            Self::System {
                started_at,
                base_pts,
            } => {
                *started_at = Instant::now();
                *base_pts = base;
            }
            Self::Audio { fallback, .. } => {
                if fallback.is_some() {
                    *fallback = Some((Instant::now(), base));
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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

    // ── MasterClock tests ─────────────────────────────────────────────────────

    #[test]
    fn master_clock_system_should_advance_from_base_pts() {
        let clock = MasterClock::System {
            started_at: Instant::now(),
            base_pts: Duration::from_secs(5),
        };
        let pts = clock.current_pts();
        assert!(
            pts >= Duration::from_secs(5),
            "pts must be >= base_pts; got {pts:?}"
        );
        assert!(
            pts < Duration::from_secs(6),
            "pts must not advance 1 s in a unit test; got {pts:?}"
        );
        assert!(clock.should_sync(), "System clock must always sync");
    }

    #[test]
    fn master_clock_system_reset_should_update_base_and_time_reference() {
        let mut clock = MasterClock::System {
            started_at: Instant::now() - Duration::from_secs(10),
            base_pts: Duration::ZERO,
        };
        assert!(
            clock.current_pts() >= Duration::from_secs(9),
            "clock should show ~10 s before reset"
        );
        clock.reset(Duration::from_secs(5));
        let pts = clock.current_pts();
        assert!(
            pts >= Duration::from_secs(5),
            "pts must be >= new base after reset; got {pts:?}"
        );
        assert!(
            pts < Duration::from_secs(6),
            "pts must not advance 1 s in a unit test after reset; got {pts:?}"
        );
    }

    #[test]
    fn master_clock_audio_should_not_sync_before_first_sample() {
        let clock = MasterClock::Audio {
            samples_consumed: Arc::new(AtomicU64::new(0)),
            sample_rate: 48_000,
            fallback: None,
        };
        assert!(
            !clock.should_sync(),
            "audio clock must not sync before any samples are consumed and before fallback is armed"
        );
        assert_eq!(
            clock.current_pts(),
            Duration::ZERO,
            "audio clock PTS must be zero before any samples and before fallback is armed"
        );
    }

    #[test]
    fn master_clock_audio_should_sync_and_report_pts_after_samples_consumed() {
        let consumed = Arc::new(AtomicU64::new(48_000));
        let clock = MasterClock::Audio {
            samples_consumed: Arc::clone(&consumed),
            sample_rate: 48_000,
            fallback: None,
        };
        assert!(
            clock.should_sync(),
            "audio clock must sync when samples > 0"
        );
        assert_eq!(
            clock.current_pts(),
            Duration::from_secs(1),
            "48000 samples at 48000 Hz must equal 1 second"
        );
    }

    #[test]
    fn master_clock_audio_should_sync_after_fallback_activated() {
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::new(AtomicU64::new(0)),
            sample_rate: 48_000,
            fallback: None,
        };
        assert!(
            !clock.should_sync(),
            "must not sync before fallback is armed"
        );
        clock.activate_fallback_if_no_audio(Duration::from_secs(1));
        assert!(
            clock.should_sync(),
            "must sync after fallback is activated even when samples_consumed == 0"
        );
    }

    #[test]
    fn master_clock_audio_fallback_current_pts_should_advance_from_base_pts() {
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::new(AtomicU64::new(0)),
            sample_rate: 48_000,
            fallback: None,
        };
        let base = Duration::from_secs(5);
        clock.activate_fallback_if_no_audio(base);
        let pts = clock.current_pts();
        assert!(
            pts >= base,
            "fallback current_pts must be >= base_pts; got {pts:?}"
        );
        assert!(
            pts < base + Duration::from_secs(1),
            "fallback must not advance 1 s in a unit test; got {pts:?}"
        );
    }

    #[test]
    fn master_clock_audio_max_of_sample_and_fallback_should_prefer_further_ahead() {
        // current_pts() returns max(sample_pts, fallback_pts) when both are set.
        // Scenario: initial fallback armed at 2 s (first frame PTS=2s, no cpal
        // consumer). Then 1 s of audio is consumed. sample_pts=1 s < fallback≈2 s,
        // so the fallback wins and the clock reports ≈2 s.
        let consumed = Arc::new(AtomicU64::new(0));
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::clone(&consumed),
            sample_rate: 48_000,
            fallback: None,
        };
        clock.activate_fallback_if_no_audio(Duration::from_secs(2));
        assert!(clock.should_sync(), "fallback must enable sync");
        // Audio consumer processes 1 s of audio.
        consumed.store(48_000, Ordering::Relaxed);
        // sample_pts=1 s, fallback_pts≈2 s → max returns ≈2 s.
        let pts = clock.current_pts();
        assert!(
            pts >= Duration::from_secs(2),
            "max() must return fallback when fallback is further ahead; got {pts:?}"
        );
        assert!(
            pts < Duration::from_secs(3),
            "fallback must not be wildly ahead of 2 s; got {pts:?}"
        );
    }

    #[test]
    fn master_clock_audio_activate_fallback_should_be_idempotent() {
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::new(AtomicU64::new(0)),
            sample_rate: 48_000,
            fallback: None,
        };
        clock.activate_fallback_if_no_audio(Duration::from_secs(1));
        let pts1 = clock.current_pts();
        thread::sleep(Duration::from_millis(5));
        // Second call with a different base must be ignored.
        clock.activate_fallback_if_no_audio(Duration::from_secs(100));
        let pts2 = clock.current_pts();
        assert!(
            pts2 > pts1,
            "clock must keep advancing from the first base after second activate; \
             pts1={pts1:?} pts2={pts2:?}"
        );
        assert!(
            pts2 < Duration::from_secs(5),
            "second activate must not reset clock to base=100 s; pts2={pts2:?}"
        );
    }

    #[test]
    fn master_clock_audio_reset_should_update_fallback_base_pts() {
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::new(AtomicU64::new(0)),
            sample_rate: 48_000,
            fallback: None,
        };
        clock.activate_fallback_if_no_audio(Duration::from_secs(5));
        // Simulate a seek to 10 s.
        clock.reset(Duration::from_secs(10));
        let pts = clock.current_pts();
        assert!(
            pts >= Duration::from_secs(10),
            "after reset, fallback must advance from the new base_pts; got {pts:?}"
        );
        assert!(
            pts < Duration::from_secs(11),
            "fallback must not advance 1 s in a unit test after reset; got {pts:?}"
        );
    }

    #[test]
    fn master_clock_audio_reset_should_not_arm_fallback_if_not_yet_active() {
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::new(AtomicU64::new(0)),
            sample_rate: 48_000,
            fallback: None,
        };
        // reset() before the first frame must not arm the fallback.
        clock.reset(Duration::ZERO);
        assert!(
            !clock.should_sync(),
            "reset() before activate_fallback_if_no_audio must not arm the fallback"
        );
        assert_eq!(
            clock.current_pts(),
            Duration::ZERO,
            "PTS must remain ZERO when fallback is not yet armed"
        );
    }

    #[test]
    fn master_clock_audio_rearm_should_advance_past_frozen_sample_pts() {
        // Simulates audio-track-ended-before-video: samples_consumed is frozen
        // at 45 222 ms worth of frames. After rearm_fallback_at(45.222s), the
        // clock must advance beyond 45.222 s even though samples_consumed does
        // not change.
        let frozen_frames: u64 = (45_222 * 48_000) / 1_000; // frames for 45.222 s
        let consumed = Arc::new(AtomicU64::new(frozen_frames));
        let mut clock = MasterClock::Audio {
            samples_consumed: Arc::clone(&consumed),
            sample_rate: 48_000,
            fallback: None,
        };
        let frozen_pts = Duration::from_secs_f64(frozen_frames as f64 / 48_000.0);
        // Before rearm: clock is frozen at the audio EOF position.
        assert_eq!(
            clock.current_pts(),
            frozen_pts,
            "clock must be frozen at audio EOF position before rearm"
        );
        // Re-arm at the frozen position.
        clock.rearm_fallback_at(frozen_pts);
        thread::sleep(Duration::from_millis(10));
        // After rearm: clock must have advanced past the frozen value.
        let pts_after = clock.current_pts();
        assert!(
            pts_after > frozen_pts,
            "clock must advance past frozen sample_pts after rearm; \
             frozen={frozen_pts:?} after={pts_after:?}"
        );
        assert!(
            pts_after < frozen_pts + Duration::from_secs(1),
            "clock must not advance 1 s in a unit test after rearm; got {pts_after:?}"
        );
    }

    #[test]
    fn master_clock_audio_rearm_should_be_noop_for_system_clock() {
        let mut clock = MasterClock::System {
            started_at: Instant::now(),
            base_pts: Duration::ZERO,
        };
        // Must not panic and System behaviour must be unchanged.
        clock.rearm_fallback_at(Duration::from_secs(99));
        assert!(
            clock.should_sync(),
            "System clock must always sync after rearm_fallback_at"
        );
    }

    #[test]
    fn audio_samples_snapshot_should_return_current_counter_for_audio_clock() {
        let consumed = Arc::new(AtomicU64::new(12_345));
        let clock = MasterClock::Audio {
            samples_consumed: Arc::clone(&consumed),
            sample_rate: 48_000,
            fallback: None,
        };
        assert_eq!(
            clock.audio_samples_snapshot(),
            12_345,
            "audio_samples_snapshot must reflect the current AtomicU64 value"
        );
    }

    #[test]
    fn audio_samples_snapshot_should_return_zero_for_system_clock() {
        let clock = MasterClock::System {
            started_at: Instant::now(),
            base_pts: Duration::ZERO,
        };
        assert_eq!(
            clock.audio_samples_snapshot(),
            0,
            "audio_samples_snapshot must return 0 for System clock"
        );
    }

    #[test]
    fn master_clock_audio_current_pts_should_advance_one_second_after_48k_frames() {
        // After the fix, MasterClock::Audio is always constructed with
        // sample_rate = DECODED_SAMPLE_RATE = 48_000 (the decoder output rate).
        // 48 000 stereo frames consumed at 48 000 Hz must equal exactly 1 second.
        let consumed = Arc::new(AtomicU64::new(48_000));
        let clock = MasterClock::Audio {
            samples_consumed: Arc::clone(&consumed),
            sample_rate: 48_000,
            fallback: None,
        };
        assert_eq!(
            clock.current_pts(),
            Duration::from_secs(1),
            "48 000 consumed frames / 48 000 Hz must equal exactly 1.0 s"
        );
    }

    #[test]
    fn master_clock_audio_native_rate_mismatch_demonstrates_bug() {
        // Documents the pre-fix behaviour: if the source file's native rate
        // (e.g. 44 100 Hz) were used instead of the decoder's output rate,
        // 48 000 consumed frames would yield 1.088 s — 8.8 % too fast.
        // This test is deliberately left in to show what the wrong answer looks like.
        let consumed = Arc::new(AtomicU64::new(48_000));
        let clock_wrong = MasterClock::Audio {
            samples_consumed: Arc::clone(&consumed),
            sample_rate: 44_100, // wrong: source native rate, not decoder output rate
            fallback: None,
        };
        let pts_wrong = clock_wrong.current_pts();
        // 48 000 / 44 100 ≈ 1.0884 s — NOT 1.0 s
        assert!(
            pts_wrong > Duration::from_secs(1),
            "using native rate produces a clock that runs too fast; got {pts_wrong:?}"
        );
        assert!(
            pts_wrong < Duration::from_millis(1_100),
            "drift must be bounded to ~8.8 %; got {pts_wrong:?}"
        );
    }

    #[test]
    fn master_clock_system_activate_fallback_should_be_noop() {
        let mut clock = MasterClock::System {
            started_at: Instant::now(),
            base_pts: Duration::ZERO,
        };
        // Must not panic and must not change System behaviour.
        clock.activate_fallback_if_no_audio(Duration::from_secs(99));
        assert!(
            clock.should_sync(),
            "System clock must always sync regardless of activate_fallback_if_no_audio"
        );
    }
}
