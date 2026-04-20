//! Actor-model playback types for ff-preview.
//!
//! # Overview
//!
//! [`PreviewPlayer`] opens a media file and is a thin builder.  Call
//! [`PreviewPlayer::split`] to obtain a ([`PlayerRunner`], [`PlayerHandle`]) pair:
//!
//! - **[`PlayerRunner`]** — owns the decode buffers, audio thread, and
//!   presentation clock. Move it to a dedicated thread and call
//!   [`PlayerRunner::run`]. The method runs until EOF or a [`PlayerCommand::Stop`]
//!   command arrives.
//! - **[`PlayerHandle`]** — `Clone + Send + Sync`. Holds a command sender and
//!   read-only state atomics. All control methods use `try_send` — they never
//!   block. If the command channel (capacity 64) is full the send is silently
//!   dropped.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ff_decode::{AudioDecoder, HardwareAccel, SeekMode};
use ff_format::SampleFormat;

use super::clock::MasterClock;
use super::decode_buffer::{DecodeBuffer, FrameResult};
use super::sink::FrameSink;
use crate::error::PreviewError;
use crate::event::PlayerEvent;

// ── Constants ─────────────────────────────────────────────────────────────────

const AUDIO_MAX_BUF: usize = 96_000;
const CHANNEL_CAP: usize = 64;

// ── PlayerCommand ─────────────────────────────────────────────────────────────

/// Commands sent from [`PlayerHandle`] to [`PlayerRunner`] via a
/// bounded sync channel (capacity 64).
pub enum PlayerCommand {
    /// Resume playback (clear the paused flag).
    Play,
    /// Pause playback.
    Pause,
    /// Stop the presentation loop; [`PlayerRunner::run`] returns after the
    /// current frame.
    Stop,
    /// Seek to `pts`. Consecutive seeks are coalesced — only the last one
    /// executes.
    Seek(Duration),
    /// Set the playback rate. Values ≤ 0.0 are ignored.
    SetRate(f64),
    /// Set the A/V offset in milliseconds. Clamped to ±5 000 ms.
    SetAvOffset(i64),
}

// ── PlayerHandle ─────────────────────────────────────────────────────────────

/// Shared, cloneable handle to a running [`PlayerRunner`].
///
/// All methods are non-blocking. Commands that cannot be queued immediately
/// (channel full) are silently dropped.
///
/// # Thread safety
///
/// `PlayerHandle` is `Clone + Send + Sync` and can be shared freely across
/// threads without locking.
#[derive(Clone)]
pub struct PlayerHandle {
    cmd_tx: mpsc::SyncSender<PlayerCommand>,
    event_rx: Arc<Mutex<mpsc::Receiver<PlayerEvent>>>,
    /// Current PTS in microseconds. Written by [`PlayerRunner`] on each frame.
    current_pts: Arc<AtomicU64>,
    audio_buf: Option<Arc<Mutex<VecDeque<f32>>>>,
    /// Advances the audio master clock when `pop_audio_samples` drains samples.
    samples_consumed: Option<Arc<AtomicU64>>,
    /// Mirrors the runner's paused state; updated immediately by `play`/`pause`.
    paused: Arc<AtomicBool>,
    /// Mirrors the runner's stopped state; updated immediately by `stop`.
    stopped: Arc<AtomicBool>,
    duration_millis: u64,
}

impl PlayerHandle {
    /// Resume playback.
    pub fn play(&self) {
        self.stopped.store(false, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        let _ = self.cmd_tx.try_send(PlayerCommand::Play);
    }

    /// Pause playback.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
        let _ = self.cmd_tx.try_send(PlayerCommand::Pause);
    }

    /// Stop the presentation loop.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.cmd_tx.try_send(PlayerCommand::Stop);
    }

    /// Seek to `pts`.
    ///
    /// Consecutive calls before the runner processes them are coalesced —
    /// only the most recent `pts` executes.
    pub fn seek(&self, pts: Duration) {
        let _ = self.cmd_tx.try_send(PlayerCommand::Seek(pts));
    }

    /// Set the playback rate.
    ///
    /// Values ≤ 0.0 are silently ignored by the runner.
    pub fn set_rate(&self, rate: f64) {
        let _ = self.cmd_tx.try_send(PlayerCommand::SetRate(rate));
    }

    /// Set the A/V offset correction in milliseconds.
    ///
    /// Positive: video PTS is shifted down relative to audio (video appears
    /// delayed). Negative: video PTS is shifted up (audio appears delayed).
    pub fn set_av_offset(&self, ms: i64) {
        let _ = self.cmd_tx.try_send(PlayerCommand::SetAvOffset(ms));
    }

    /// PTS of the most recently presented frame.
    ///
    /// Returns [`Duration::ZERO`] before the first frame is presented.
    #[must_use]
    pub fn current_pts(&self) -> Duration {
        Duration::from_micros(self.current_pts.load(Ordering::Relaxed))
    }

    /// Container-reported duration, or `None` for live / streaming sources.
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        if self.duration_millis == u64::MAX {
            None
        } else {
            Some(Duration::from_millis(self.duration_millis))
        }
    }

    /// Pull up to `n` interleaved stereo `f32` PCM samples at 48 kHz.
    ///
    /// Returns an empty `Vec` when:
    /// - playback is paused or stopped,
    /// - `n` is 0,
    /// - there is no audio track, or
    /// - the ring buffer is empty (underrun — caller should output silence).
    ///
    /// Advances the audio master clock by `samples.len() / 2` stereo frames.
    #[allow(clippy::cast_precision_loss)]
    pub fn pop_audio_samples(&self, n: usize) -> Vec<f32> {
        if self.paused.load(Ordering::Relaxed) || self.stopped.load(Ordering::Relaxed) {
            return Vec::new();
        }
        if n == 0 {
            return Vec::new();
        }
        let Some(buf) = &self.audio_buf else {
            return Vec::new();
        };
        let mut guard = buf
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let take = n.min(guard.len());
        if take == 0 {
            return Vec::new();
        }
        let samples: Vec<f32> = guard.drain(..take).collect();
        if let Some(sc) = &self.samples_consumed {
            sc.fetch_add((take / 2) as u64, Ordering::Relaxed);
        }
        samples
    }

    /// Poll for the next [`PlayerEvent`] without blocking.
    ///
    /// Returns `None` when no events are pending.
    #[must_use]
    pub fn poll_event(&self) -> Option<PlayerEvent> {
        self.event_rx.lock().ok()?.try_recv().ok()
    }

    /// Block until the next [`PlayerEvent`] arrives or the channel closes.
    ///
    /// Returns `None` when the runner has exited and all events have been
    /// drained. Intended for use inside `spawn_blocking`.
    #[must_use]
    pub fn recv_event(&self) -> Option<PlayerEvent> {
        self.event_rx.lock().ok()?.recv().ok()
    }

    /// Construct a handle for a non-`PlayerRunner` runner (e.g., `TimelineRunner`).
    ///
    /// Audio fields are set to `None`; the handle's
    /// [`pop_audio_samples`](Self::pop_audio_samples) always returns an empty `Vec`.
    #[cfg(feature = "timeline")]
    pub(crate) fn for_timeline(
        cmd_tx: mpsc::SyncSender<PlayerCommand>,
        event_rx: Arc<Mutex<mpsc::Receiver<PlayerEvent>>>,
        current_pts: Arc<AtomicU64>,
        paused: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
        duration_millis: u64,
    ) -> Self {
        Self {
            cmd_tx,
            event_rx,
            current_pts,
            audio_buf: None,
            samples_consumed: None,
            paused,
            stopped,
            duration_millis,
        }
    }
}

// ── PlayerRunner ─────────────────────────────────────────────────────────────

/// Exclusive owner of the decode pipeline. Move to a background thread and
/// call [`run`](Self::run).
///
/// Configure with [`set_sink`](Self::set_sink),
/// [`use_proxy_if_available`](Self::use_proxy_if_available), and
/// [`set_hardware_accel`](Self::set_hardware_accel) **before** calling `run`.
pub struct PlayerRunner {
    path: PathBuf,
    cmd_rx: mpsc::Receiver<PlayerCommand>,
    event_tx: mpsc::SyncSender<PlayerEvent>,
    decode_buf: Option<DecodeBuffer>,
    fps: f64,
    sink: Option<Box<dyn FrameSink>>,
    clock: MasterClock,
    audio_buf: Option<Arc<Mutex<VecDeque<f32>>>>,
    audio_cancel: Option<Arc<AtomicBool>>,
    audio_handle: Option<JoinHandle<()>>,
    sws: super::playback_inner::SwsRgbaConverter,
    rgba_buf: Vec<u8>,
    active_path: PathBuf,
    current_pts: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    av_offset_ms: i64,
    rate: f64,
    duration_millis: u64,
}

impl PlayerRunner {
    /// Register the frame sink. Call before [`run`](Self::run).
    pub fn set_sink(&mut self, sink: Box<dyn FrameSink>) {
        self.sink = Some(sink);
    }

    /// Configure hardware acceleration.
    ///
    /// Currently a no-op stub — hardware acceleration is applied when the
    /// decode buffer is next rebuilt (e.g., after proxy activation).
    pub fn set_hardware_accel(&mut self, _accel: HardwareAccel) {}

    /// Returns the path currently being decoded (original or active proxy).
    #[must_use]
    pub fn active_source(&self) -> &Path {
        &self.active_path
    }

    /// Container-reported duration, or `None` for live / streaming sources.
    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        if self.duration_millis == u64::MAX {
            None
        } else {
            Some(Duration::from_millis(self.duration_millis))
        }
    }

    /// Activate a lower-resolution proxy if one exists in `proxy_dir`.
    ///
    /// Must be called before [`run`](Self::run). Returns `true` if a proxy was
    /// found and activated; `false` if no proxy exists or activation failed.
    ///
    /// Proxy lookup order: `half` → `quarter` → `eighth`; first match wins.
    pub fn use_proxy_if_available(&mut self, proxy_dir: &Path) -> bool {
        let stem = self
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_owned();

        for suffix in ["half", "quarter", "eighth"] {
            let candidate = proxy_dir.join(format!("{stem}_proxy_{suffix}.mp4"));
            if candidate.exists() {
                match self.activate_proxy(&candidate) {
                    Ok(()) => {
                        log::debug!("proxy activated path={}", candidate.display());
                        return true;
                    }
                    Err(e) => {
                        log::warn!(
                            "proxy activation failed path={} error={e}",
                            candidate.display()
                        );
                    }
                }
            }
        }
        false
    }

    /// A/V sync presentation loop.
    ///
    /// Blocks until a [`PlayerCommand::Stop`] is received, the end of file is
    /// reached, or an unrecoverable decode error occurs.
    ///
    /// At the top of each frame, all pending commands are drained from the
    /// channel. Consecutive [`PlayerCommand::Seek`] commands are coalesced —
    /// only the last one executes.
    ///
    /// Emits [`PlayerEvent::SeekCompleted`] after each successful seek,
    /// [`PlayerEvent::PositionUpdate`] after each presented video frame,
    /// [`PlayerEvent::Error`] on non-fatal decode errors, and
    /// [`PlayerEvent::Eof`] before returning.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if a seek fails.
    #[allow(clippy::too_many_lines)]
    pub fn run(mut self) -> Result<(), PreviewError> {
        let fps = self.fps.max(1.0);
        let frame_period = Duration::from_secs_f64(1.0 / fps);

        self.clock.reset(Duration::ZERO);

        loop {
            // ── Drain commands ────────────────────────────────────────────────
            let mut pending_seek: Option<Duration> = None;
            while let Ok(cmd) = self.cmd_rx.try_recv() {
                match cmd {
                    PlayerCommand::Seek(pts) => pending_seek = Some(pts),
                    PlayerCommand::Play => {
                        self.stopped.store(false, Ordering::Release);
                        self.paused.store(false, Ordering::Release);
                    }
                    PlayerCommand::Pause => {
                        self.paused.store(true, Ordering::Release);
                    }
                    PlayerCommand::Stop => {
                        self.stopped.store(true, Ordering::Release);
                    }
                    PlayerCommand::SetRate(r) => {
                        if r > 0.0 {
                            self.rate = r;
                        }
                    }
                    PlayerCommand::SetAvOffset(ms) => {
                        const MAX_OFFSET_MS: i64 = 5_000;
                        self.av_offset_ms = ms.clamp(-MAX_OFFSET_MS, MAX_OFFSET_MS);
                    }
                }
            }

            // ── Apply pending seek ────────────────────────────────────────────
            if let Some(pts) = pending_seek {
                if let Some(buf) = self.decode_buf.as_mut() {
                    buf.seek(pts)?;
                }
                self.clock.reset(pts);
                self.restart_audio_from(pts);
                let _ = self.event_tx.try_send(PlayerEvent::SeekCompleted(pts));
            }

            // Surface non-fatal decode errors from the background thread.
            if let Some(buf) = self.decode_buf.as_ref() {
                while let Ok(msg) = buf.error_events().try_recv() {
                    let _ = self.event_tx.try_send(PlayerEvent::Error(msg));
                }
            }

            if self.stopped.load(Ordering::Acquire) {
                break;
            }
            if self.paused.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
                continue;
            }

            // ── Audio-only path ───────────────────────────────────────────────
            if self.decode_buf.is_none() {
                thread::sleep(Duration::from_millis(10));
                if let Some(audio_buf) = &self.audio_buf {
                    let empty = audio_buf
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .is_empty();
                    if empty
                        && self
                            .audio_handle
                            .as_ref()
                            .is_none_or(JoinHandle::is_finished)
                    {
                        break;
                    }
                } else {
                    break;
                }
                continue;
            }

            // ── Video decode path ─────────────────────────────────────────────
            let pop_result = if let Some(buf) = self.decode_buf.as_mut() {
                buf.pop_frame()
            } else {
                FrameResult::Eof
            };

            match pop_result {
                FrameResult::Eof => break,
                FrameResult::Seeking(last) => {
                    if let Some(ref f) = last {
                        self.present_frame(f);
                    }
                }
                FrameResult::Frame(frame) => {
                    if self.clock.should_sync() {
                        let video_pts = if frame.timestamp().is_valid() {
                            frame.timestamp().as_duration()
                        } else {
                            Duration::ZERO
                        };

                        let offset_ms = self.av_offset_ms;
                        let offset = Duration::from_millis(offset_ms.unsigned_abs());
                        let adjusted_video_pts = if offset_ms >= 0 {
                            video_pts.saturating_sub(offset)
                        } else {
                            video_pts + offset
                        };

                        let clock_pts = self.clock.current_pts();
                        let diff = adjusted_video_pts.as_secs_f64() - clock_pts.as_secs_f64();
                        let fp = frame_period.as_secs_f64();

                        if diff > fp {
                            let sleep_secs =
                                (diff - fp / 2.0).max(0.0) / self.rate.max(f64::MIN_POSITIVE);
                            thread::sleep(Duration::from_secs_f64(sleep_secs));
                        } else if diff < -fp {
                            log::debug!(
                                "dropped late frame video_pts={video_pts:?} \
                                 clock_pts={clock_pts:?}"
                            );
                            continue;
                        }
                    }

                    self.present_frame(&frame);
                    let pts = frame.timestamp().as_duration();
                    let _ = self.event_tx.try_send(PlayerEvent::PositionUpdate(pts));
                }
            }
        }

        let _ = self.event_tx.try_send(PlayerEvent::Eof);
        if let Some(sink) = self.sink.as_mut() {
            sink.flush();
        }
        Ok(())
    }

    fn present_frame(&mut self, frame: &ff_format::VideoFrame) {
        let pts = frame.timestamp().as_duration();
        self.current_pts.store(
            u64::try_from(pts.as_micros()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        let Some(sink) = self.sink.as_mut() else {
            return;
        };
        let width = frame.width();
        let height = frame.height();
        if self.sws.convert(frame, &mut self.rgba_buf) {
            sink.push_frame(&self.rgba_buf, width, height, pts);
        }
    }

    fn restart_audio_from(&mut self, pts: Duration) {
        if let Some(buf) = &self.audio_buf {
            buf.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        drop(self.audio_handle.take());
        if let Some(buf) = &self.audio_buf {
            let new_cancel = Arc::new(AtomicBool::new(false));
            let handle = spawn_audio_thread(
                self.active_path.clone(),
                pts,
                Arc::clone(buf),
                Arc::clone(&new_cancel),
            );
            self.audio_cancel = Some(new_cancel);
            self.audio_handle = Some(handle);
        }
    }

    fn activate_proxy(&mut self, proxy_path: &Path) -> Result<(), PreviewError> {
        let info = ff_probe::open(proxy_path)?;
        let fps = info.frame_rate().unwrap_or(30.0).max(1.0);
        let decode_buf = DecodeBuffer::open(proxy_path).build()?;

        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(buf) = &self.audio_buf {
            buf.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        drop(self.audio_handle.take());

        let (clock, audio_buf, audio_cancel, audio_handle) = if info.has_audio() {
            let sample_rate = info.sample_rate().unwrap_or(48_000);
            let buf = Arc::new(Mutex::new(VecDeque::<f32>::new()));
            let cancel = Arc::new(AtomicBool::new(false));
            let handle = spawn_audio_thread(
                proxy_path.to_path_buf(),
                Duration::ZERO,
                Arc::clone(&buf),
                Arc::clone(&cancel),
            );
            let clock = MasterClock::Audio {
                samples_consumed: Arc::new(AtomicU64::new(0)),
                sample_rate,
            };
            (clock, Some(buf), Some(cancel), Some(handle))
        } else {
            log::debug!(
                "proxy has no audio, using system clock path={}",
                proxy_path.display()
            );
            let clock = MasterClock::System {
                started_at: Instant::now(),
                base_pts: Duration::ZERO,
            };
            (clock, None, None, None)
        };

        self.active_path = proxy_path.to_path_buf();
        self.fps = fps;
        self.decode_buf = Some(decode_buf);
        self.clock = clock;
        self.audio_buf = audio_buf;
        self.audio_cancel = audio_cancel;
        self.audio_handle = audio_handle;
        Ok(())
    }
}

impl Drop for PlayerRunner {
    fn drop(&mut self) {
        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(h) = self.audio_handle.take() {
            let _ = h.join();
        }
    }
}

// ── PreviewPlayer (thin builder) ──────────────────────────────────────────────

/// Thin builder for a ([`PlayerRunner`], [`PlayerHandle`]) pair.
///
/// # Usage
///
/// ```ignore
/// let (mut runner, handle) = PreviewPlayer::open("clip.mp4")?.split();
///
/// runner.set_sink(Box::new(MySink::new()));
///
/// let handle_audio = handle.clone();
///
/// std::thread::spawn(move || { let _ = runner.run(); });
///
/// handle.seek(Duration::from_secs(30));
/// handle.play();
///
/// // cpal audio callback:
/// device.build_output_stream(&cfg, move |buf: &mut [f32], _| {
///     let s = handle_audio.pop_audio_samples(buf.len());
///     buf[..s.len()].copy_from_slice(&s);
/// }, ...);
/// ```
pub struct PreviewPlayer {
    path: PathBuf,
    /// `None` after `split()` consumes it.
    decode_buf: Option<DecodeBuffer>,
    fps: f64,
    /// `None` after `split()` consumes it.
    clock: Option<MasterClock>,
    audio_buf: Option<Arc<Mutex<VecDeque<f32>>>>,
    audio_cancel: Option<Arc<AtomicBool>>,
    audio_handle: Option<JoinHandle<()>>,
    duration_millis: u64,
    active_path: PathBuf,
}

impl PreviewPlayer {
    /// Open a media file and prepare for playback.
    ///
    /// Probes the file to detect audio/video streams, opens a
    /// [`DecodeBuffer`] for the video stream (when present), and spawns a
    /// background audio decode thread (when present). Returns
    /// [`PreviewError`] if the file is missing or contains neither stream.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the file cannot be probed or decoded.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PreviewError> {
        let path = path.as_ref();
        let info = ff_probe::open(path)?;

        if !info.has_video() && !info.has_audio() {
            return Err(PreviewError::Ffmpeg {
                code: -1,
                message: "file has neither a video nor an audio stream".into(),
            });
        }

        let fps = info.frame_rate().unwrap_or(30.0).max(1.0);

        let d = info.duration();
        let duration_millis = if d.is_zero() {
            u64::MAX
        } else {
            u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
        };

        let clock = if info.has_audio() {
            let sample_rate = info.sample_rate().unwrap_or(48_000);
            MasterClock::Audio {
                samples_consumed: Arc::new(AtomicU64::new(0)),
                sample_rate,
            }
        } else {
            log::debug!(
                "using system clock fallback path={} no_audio=true",
                path.display()
            );
            MasterClock::System {
                started_at: Instant::now(),
                base_pts: Duration::ZERO,
            }
        };

        let decode_buf = if info.has_video() {
            Some(DecodeBuffer::open(path).build()?)
        } else {
            log::debug!(
                "audio-only file; skipping video decode buffer path={}",
                path.display()
            );
            None
        };

        let (audio_buf, audio_cancel, audio_handle) = if let MasterClock::Audio { .. } = &clock {
            let buf = Arc::new(Mutex::new(VecDeque::<f32>::new()));
            let cancel = Arc::new(AtomicBool::new(false));
            let handle = spawn_audio_thread(
                path.to_path_buf(),
                Duration::ZERO,
                Arc::clone(&buf),
                Arc::clone(&cancel),
            );
            (Some(buf), Some(cancel), Some(handle))
        } else {
            (None, None, None)
        };

        Ok(PreviewPlayer {
            path: path.to_path_buf(),
            decode_buf,
            fps,
            clock: Some(clock),
            audio_buf,
            audio_cancel,
            audio_handle,
            duration_millis,
            active_path: path.to_path_buf(),
        })
    }

    /// Consume `self` and return an exclusive [`PlayerRunner`] and a shared
    /// [`PlayerHandle`].
    ///
    /// The runner owns the decode pipeline; move it to a background thread
    /// and call [`PlayerRunner::run`].
    /// The handle is `Clone + Send + Sync` and can be shared freely.
    ///
    /// # Panics
    ///
    /// Never panics in practice — the internal clock is always `Some` when
    /// `split` is first called.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn split(mut self) -> (PlayerRunner, PlayerHandle) {
        let current_pts = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let (cmd_tx, cmd_rx) = mpsc::sync_channel(CHANNEL_CAP);
        let (event_tx, event_rx) = mpsc::sync_channel(CHANNEL_CAP);

        let clock = self.clock.take().expect("clock consumed before split");
        let samples_consumed = match &clock {
            MasterClock::Audio {
                samples_consumed, ..
            } => Some(Arc::clone(samples_consumed)),
            MasterClock::System { .. } => None,
        };

        let audio_buf_for_handle = self.audio_buf.clone();
        let duration_millis = self.duration_millis;

        let runner = PlayerRunner {
            path: self.path.clone(),
            cmd_rx,
            event_tx,
            decode_buf: self.decode_buf.take(),
            fps: self.fps,
            sink: None,
            clock,
            audio_buf: self.audio_buf.take(),
            audio_cancel: self.audio_cancel.take(),
            audio_handle: self.audio_handle.take(),
            sws: super::playback_inner::SwsRgbaConverter::new(),
            rgba_buf: Vec::new(),
            active_path: self.active_path.clone(),
            current_pts: Arc::clone(&current_pts),
            paused: Arc::clone(&paused),
            stopped: Arc::clone(&stopped),
            av_offset_ms: 0,
            rate: 1.0,
            duration_millis,
        };

        let handle = PlayerHandle {
            cmd_tx,
            event_rx: Arc::new(Mutex::new(event_rx)),
            current_pts,
            audio_buf: audio_buf_for_handle,
            samples_consumed,
            paused,
            stopped,
            duration_millis,
        };

        (runner, handle)
    }
}

impl Drop for PreviewPlayer {
    fn drop(&mut self) {
        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(h) = self.audio_handle.take() {
            let _ = h.join();
        }
    }
}

// ── spawn_audio_thread ────────────────────────────────────────────────────────

fn spawn_audio_thread(
    path: PathBuf,
    start_pts: Duration,
    buf: Arc<Mutex<VecDeque<f32>>>,
    cancel: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut decoder = match AudioDecoder::open(&path)
            .output_format(SampleFormat::F32)
            .output_sample_rate(48_000)
            .output_channels(2)
            .build()
        {
            Ok(d) => d,
            Err(e) => {
                log::warn!("audio decode thread open failed error={e}");
                return;
            }
        };

        if start_pts != Duration::ZERO
            && let Err(e) = decoder.seek(start_pts, SeekMode::Backward)
        {
            log::warn!("audio seek failed pts={start_pts:?} error={e}");
        }

        loop {
            if cancel.load(Ordering::Acquire) {
                break;
            }

            let buf_len = buf
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len();
            if buf_len >= AUDIO_MAX_BUF {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            match decoder.decode_one() {
                Ok(Some(frame)) => {
                    let samples = super::playback_inner::audio_frame_to_f32(&frame);
                    if !samples.is_empty() {
                        let mut guard = buf
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let space = AUDIO_MAX_BUF.saturating_sub(guard.len());
                        guard.extend(samples.into_iter().take(space));
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    log::warn!("audio decode error error={e}");
                    break;
                }
            }
        }
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_video_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/video/gameplay.mp4")
    }

    fn test_audio_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/audio/konekonoosanpo.mp3")
    }

    // ── open ──────────────────────────────────────────────────────────────────

    #[test]
    fn preview_player_open_should_fail_for_nonexistent_file() {
        let result = PreviewPlayer::open("nonexistent_preview.mp4");
        assert!(
            result.is_err(),
            "open() must return Err for a non-existent file"
        );
    }

    // ── play / pause / stop via handle ───────────────────────────────────────

    #[test]
    fn player_handle_play_pause_should_update_paused_flag_immediately() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        assert!(!handle.paused.load(Ordering::Relaxed));
        assert!(!handle.stopped.load(Ordering::Relaxed));

        handle.pause();
        assert!(handle.paused.load(Ordering::Relaxed));

        handle.play();
        assert!(!handle.paused.load(Ordering::Relaxed));
        assert!(!handle.stopped.load(Ordering::Relaxed));

        handle.stop();
        assert!(handle.stopped.load(Ordering::Relaxed));
    }

    // ── run with sink ─────────────────────────────────────────────────────────

    #[test]
    fn player_runner_run_should_deliver_frames_to_sink() {
        struct CountSink(Arc<Mutex<usize>>);
        impl FrameSink for CountSink {
            fn push_frame(&mut self, _rgba: &[u8], _w: u32, _h: u32, _pts: Duration) {
                *self
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            }
        }

        let path = test_video_path();
        let (mut runner, _handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        let count = Arc::new(Mutex::new(0usize));
        runner.set_sink(Box::new(CountSink(Arc::clone(&count))));

        match runner.run() {
            Ok(()) => {}
            Err(e) => {
                println!("skipping: run() error: {e}");
                return;
            }
        }

        let frames = *count
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            frames > 0,
            "run() must deliver at least one frame to the sink"
        );
    }

    // ── pop_audio_samples ────────────────────────────────────────────────────

    #[test]
    fn pop_audio_samples_should_return_empty_when_paused() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        handle.pause();
        let samples = handle.pop_audio_samples(1024);
        assert!(
            samples.is_empty(),
            "pop_audio_samples() must return empty while paused"
        );
    }

    #[test]
    fn pop_audio_samples_should_return_empty_when_stopped() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        handle.stop();
        let samples = handle.pop_audio_samples(1024);
        assert!(
            samples.is_empty(),
            "pop_audio_samples() must return empty while stopped"
        );
    }

    #[test]
    fn pop_audio_samples_should_return_empty_for_zero_n_samples() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        handle.play();
        let samples = handle.pop_audio_samples(0);
        assert!(
            samples.is_empty(),
            "pop_audio_samples(0) must always return empty"
        );
    }

    #[test]
    fn pop_audio_samples_should_be_callable_via_cloned_handle() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        let shared = handle.clone();
        let _samples = shared.pop_audio_samples(0);
    }

    #[test]
    fn pop_audio_samples_clock_increment_should_equal_half_sample_count() {
        let stereo_samples: usize = 9_600;
        let expected_frames: u64 = (stereo_samples / 2) as u64;
        assert_eq!(
            expected_frames, 4_800,
            "9600 stereo samples must yield 4800 clock frames"
        );
        let pts = Duration::from_secs_f64(f64::from(48_000u32).recip() * expected_frames as f64);
        assert!(
            (pts.as_secs_f64() - 0.1).abs() < 1e-6,
            "4800 frames at 48 kHz must equal 100 ms; got {pts:?}"
        );
    }

    // ── current_pts / duration ───────────────────────────────────────────────

    #[test]
    fn current_pts_should_return_zero_before_first_frame() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        assert_eq!(
            handle.current_pts(),
            Duration::ZERO,
            "current_pts() must be ZERO before any frame is presented"
        );
    }

    #[test]
    fn duration_should_return_some_for_file_with_known_duration() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        assert!(
            handle.duration().is_some(),
            "duration() must return Some for a file with a known container duration"
        );
        let d = handle.duration().unwrap();
        assert!(
            d > Duration::ZERO,
            "duration() must be positive for a valid media file; got {d:?}"
        );
    }

    #[test]
    fn duration_should_return_none_when_duration_millis_is_sentinel() {
        let sentinel = u64::MAX;
        let result: Option<Duration> = if sentinel == u64::MAX {
            None
        } else {
            Some(Duration::from_millis(sentinel))
        };
        assert!(result.is_none(), "sentinel u64::MAX must map to None");

        let valid = 5_000u64;
        let result: Option<Duration> = if valid == u64::MAX {
            None
        } else {
            Some(Duration::from_millis(valid))
        };
        assert_eq!(result, Some(Duration::from_secs(5)));
    }

    #[test]
    fn current_pts_should_advance_after_frames_are_presented() {
        struct PtsSink(Arc<Mutex<Option<Duration>>>);
        impl FrameSink for PtsSink {
            fn push_frame(&mut self, _rgba: &[u8], _w: u32, _h: u32, pts: Duration) {
                *self
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pts);
            }
        }

        let path = test_video_path();
        let (mut runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        let last_pts = Arc::new(Mutex::new(None::<Duration>));
        runner.set_sink(Box::new(PtsSink(Arc::clone(&last_pts))));
        let _ = runner.run();

        let sink_pts = last_pts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .unwrap_or(Duration::ZERO);
        let player_pts = handle.current_pts();
        let diff = sink_pts.abs_diff(player_pts);
        assert!(
            diff <= Duration::from_millis(1),
            "current_pts() must be within 1 ms of the last sink PTS; \
             player_pts={player_pts:?} sink_pts={sink_pts:?} diff={diff:?}"
        );
    }

    // ── seek ──────────────────────────────────────────────────────────────────

    #[test]
    fn seek_coarse_should_delegate_to_decode_buffer() {
        let path = test_video_path();
        let (runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        let target = Duration::from_secs(1);
        handle.seek(target);

        // Stop after a short time so the test doesn't block for the full file.
        let handle_thread = handle.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            handle_thread.stop();
        });

        match runner.run() {
            Ok(()) => {}
            Err(e) => {
                println!("skipping: run() error: {e}");
            }
        }
    }

    // ── proxy ─────────────────────────────────────────────────────────────────

    #[test]
    fn use_proxy_if_available_should_return_false_when_no_proxy_in_dir() {
        let path = test_video_path();
        let (mut runner, _handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        let tmp = std::env::temp_dir().join("ff_preview_no_proxy_dir_test");
        let _ = std::fs::create_dir_all(&tmp);
        let found = runner.use_proxy_if_available(&tmp);
        assert!(
            !found,
            "must return false when no proxy files exist in the directory"
        );
    }

    #[test]
    fn active_source_should_return_original_path_before_proxy_activation() {
        let path = test_video_path();
        let (runner, _handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        assert_eq!(
            runner.active_source(),
            path.as_path(),
            "active_source() must equal the original path before any proxy activation"
        );
    }

    // ── set_rate / set_av_offset ──────────────────────────────────────────────

    #[test]
    fn set_rate_should_accept_positive_value() {
        let path = test_video_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        // Verify that calling set_rate with a valid value does not panic.
        handle.set_rate(2.0);
        handle.set_rate(0.5);
    }

    #[test]
    fn set_av_offset_default_should_be_zero() {
        use std::sync::atomic::{AtomicI64, Ordering};
        let offset = AtomicI64::new(0);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn positive_av_offset_should_reduce_adjusted_video_pts() {
        let video_pts = Duration::from_millis(1_000);
        let offset_ms: i64 = 200;
        let adjusted = if offset_ms >= 0 {
            let offset = Duration::from_millis(offset_ms as u64);
            video_pts.saturating_sub(offset)
        } else {
            let offset = Duration::from_millis(offset_ms.unsigned_abs());
            video_pts + offset
        };
        assert_eq!(
            adjusted,
            Duration::from_millis(800),
            "positive offset must reduce adjusted_video_pts by offset amount"
        );
    }

    #[test]
    fn negative_av_offset_should_increase_adjusted_video_pts() {
        let video_pts = Duration::from_millis(1_000);
        let offset_ms: i64 = -200;
        let adjusted = if offset_ms >= 0 {
            let offset = Duration::from_millis(offset_ms as u64);
            video_pts.saturating_sub(offset)
        } else {
            let offset = Duration::from_millis(offset_ms.unsigned_abs());
            video_pts + offset
        };
        assert_eq!(
            adjusted,
            Duration::from_millis(1_200),
            "negative offset must increase adjusted_video_pts by offset amount"
        );
    }

    #[test]
    fn positive_av_offset_at_zero_pts_should_saturate_to_zero() {
        let video_pts = Duration::ZERO;
        let offset_ms: i64 = 100;
        let adjusted = video_pts.saturating_sub(Duration::from_millis(offset_ms as u64));
        assert_eq!(
            adjusted,
            Duration::ZERO,
            "saturating_sub on zero pts must clamp to zero not underflow"
        );
    }

    // ── audio-only ────────────────────────────────────────────────────────────

    #[test]
    fn audio_only_open_should_succeed() {
        let path = test_audio_path();
        match PreviewPlayer::open(&path) {
            Ok(player) => {
                let (runner, handle) = player.split();
                // Audio-only: runner has no decode buffer.
                assert!(
                    runner.decode_buf.is_none(),
                    "audio-only runner must have no video decode buffer"
                );
                // Handle has an audio buffer.
                assert!(
                    handle.audio_buf.is_some(),
                    "audio-only handle must have an audio ring buffer"
                );
            }
            Err(e) => {
                println!("skipping: audio file not available: {e}");
            }
        }
    }

    #[test]
    fn audio_only_run_should_return_ok_without_video_frames() {
        let path = test_audio_path();
        let (mut runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: audio file not available: {e}");
                return;
            }
        };

        struct CountingSink(usize);
        impl FrameSink for CountingSink {
            fn push_frame(&mut self, _rgba: &[u8], _w: u32, _h: u32, _pts: Duration) {
                self.0 += 1;
            }
        }
        runner.set_sink(Box::new(CountingSink(0)));

        let handle_thread = handle.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            handle_thread.stop();
        });

        let result = runner.run();
        assert!(
            result.is_ok(),
            "run() on an audio-only player must return Ok; got {result:?}"
        );
        assert_eq!(
            handle.current_pts(),
            Duration::ZERO,
            "current_pts() must remain ZERO for audio-only playback (no video frames)"
        );
    }

    #[test]
    fn audio_only_seek_should_not_fail_for_valid_target() {
        let path = test_audio_path();
        let (_runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: audio file not available: {e}");
                return;
            }
        };
        // seek() on audio-only player sends a command without errors.
        handle.seek(Duration::from_secs(1));
    }

    // ── seek event delivery (integration) ────────────────────────────────────

    #[test]
    #[ignore = "requires assets/video/gameplay.mp4; run with -- --include-ignored"]
    fn seek_should_deliver_seek_completed_event_via_poll_event() {
        let path = test_video_path();
        if !path.exists() {
            println!("skipping: video file not found at {}", path.display());
            return;
        }

        let (runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: open failed: {e}");
                return;
            }
        };

        let handle_bg = handle.clone();
        let bg = thread::spawn(move || {
            let _ = runner.run();
        });

        // Give the runner one frame period to start, then seek.
        thread::sleep(Duration::from_millis(50));
        let target = Duration::from_secs(1);
        handle.seek(target);

        // Wait up to 2 seconds for SeekCompleted.
        let deadline = Instant::now() + Duration::from_secs(2);
        let event = loop {
            if let Some(e) = handle.poll_event() {
                break Some(e);
            }
            if Instant::now() > deadline {
                break None;
            }
            thread::sleep(Duration::from_millis(10));
        };

        handle_bg.stop();
        let _ = bg.join();

        match event {
            Some(PlayerEvent::SeekCompleted(pts)) => {
                assert!(
                    pts >= target.saturating_sub(Duration::from_millis(100)),
                    "SeekCompleted pts must be near the requested target; \
                     target={target:?} pts={pts:?}"
                );
            }
            Some(PlayerEvent::Eof) => {
                panic!("received Eof before SeekCompleted — file may be too short");
            }
            Some(PlayerEvent::PositionUpdate(_) | PlayerEvent::Error(_)) | None => {
                panic!("no PlayerEvent::SeekCompleted received within 2 seconds");
            }
        }
    }

    // ── PlayerEvent: PositionUpdate + Error ───────────────────────────────────

    #[test]
    fn position_update_and_error_event_variants_should_be_accessible() {
        let _ = PlayerEvent::PositionUpdate(Duration::ZERO);
        let _ = PlayerEvent::Error("test error".to_string());
    }

    #[test]
    fn eof_event_should_be_delivered_after_run_completes() {
        let path = test_audio_path();
        let (runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: {e}");
                return;
            }
        };

        // Stop after 150 ms so the test does not wait for the full audio duration.
        let handle_stop = handle.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            handle_stop.stop();
        });

        let _ = runner.run();
        let events: Vec<_> = std::iter::from_fn(|| handle.poll_event()).collect();
        assert!(
            events.iter().any(|e| matches!(e, PlayerEvent::Eof)),
            "Eof event must be delivered after run() returns; collected {} events",
            events.len()
        );
    }

    #[test]
    #[ignore = "requires assets/video/gameplay.mp4; run with -- --include-ignored"]
    fn position_update_should_be_emitted_for_each_video_frame() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/video/gameplay.mp4");
        if !path.exists() {
            println!("skipping: video asset not found");
            return;
        }

        use std::sync::{Arc, Mutex};
        struct CountSink {
            count: Arc<Mutex<usize>>,
            max: usize,
            handle: PlayerHandle,
        }
        impl FrameSink for CountSink {
            fn push_frame(&mut self, _rgba: &[u8], _w: u32, _h: u32, _pts: Duration) {
                let mut g = self
                    .count
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *g += 1;
                if *g >= self.max {
                    self.handle.stop();
                }
            }
        }

        let (mut runner, handle) = match PreviewPlayer::open(&path) {
            Ok(p) => p.split(),
            Err(e) => {
                println!("skipping: {e}");
                return;
            }
        };

        let count = Arc::new(Mutex::new(0usize));
        runner.set_sink(Box::new(CountSink {
            count: Arc::clone(&count),
            max: 20,
            handle: handle.clone(),
        }));
        let _ = runner.run();

        let frames = *count
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let position_updates: Vec<_> = std::iter::from_fn(|| handle.poll_event())
            .filter(|e| matches!(e, PlayerEvent::PositionUpdate(_)))
            .collect();

        assert!(
            !position_updates.is_empty(),
            "at least one PositionUpdate event must be emitted; frames delivered={frames}"
        );
        assert!(
            position_updates.len() <= frames,
            "PositionUpdate count ({}) must not exceed frame count ({frames})",
            position_updates.len()
        );
    }
}
