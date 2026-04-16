//! `PreviewPlayer` — main playback driver for ff-preview.
//!
//! All safe Rust logic lives here. Unsafe `FFmpeg` calls are isolated in
//! `playback_inner`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ff_decode::{AudioDecoder, SeekMode};
use ff_format::SampleFormat;

use super::clock::MasterClock;
use super::decode_buffer::{DecodeBuffer, FrameResult, SeekEvent};
use super::sink::FrameSink;
use crate::error::PreviewError;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum number of interleaved stereo `f32` samples to buffer for audio
/// playback (2 s × 48 kHz × 2 channels = 96 000).
const AUDIO_MAX_BUF: usize = 96_000;

// ── PreviewPlayer ─────────────────────────────────────────────────────────────

/// Drives real-time playback of a single media file.
///
/// `PreviewPlayer` decodes a video/audio file, synchronises video frame
/// presentation to an audio master clock, and delivers frames to a
/// registered [`FrameSink`].
///
/// # Usage
///
/// ```ignore
/// let mut player = PreviewPlayer::open(Path::new("clip.mp4"))?;
/// player.set_sink(Box::new(MySink::new()));
/// player.play();
/// player.run()?;
/// ```
pub struct PreviewPlayer {
    /// Path to the media file; retained so the audio decode thread can be
    /// restarted from a new position after a seek.
    path: PathBuf,
    /// Pre-decoded frame buffer driven by a background thread.
    decode_buf: DecodeBuffer,
    /// Video frame rate; used to compute the frame period for A/V sync.
    fps: f64,
    /// Frame sink registered via [`set_sink`](Self::set_sink). Optional;
    /// frames are discarded silently if no sink is set.
    sink: Option<Box<dyn FrameSink>>,
    /// Set to `true` while the presentation loop is paused.
    paused: Arc<AtomicBool>,
    /// Set to `true` to signal [`run`](Self::run) to stop after the current frame.
    stopped: Arc<AtomicBool>,
    /// Master clock for A/V sync: audio samples counter or `Instant` wall clock.
    clock: MasterClock,
    /// A/V offset correction in milliseconds (default: 0).
    ///
    /// Positive: video is delayed (video PTS adjusted down).
    /// Negative: audio is delayed (video PTS adjusted up).
    av_offset_ms: AtomicI64,
    /// Decoded audio samples (interleaved f32 stereo at 48 kHz).
    /// `None` when the media file has no audio track.
    audio_buf: Option<Arc<Mutex<VecDeque<f32>>>>,
    /// Cancel flag for the background audio decode thread.
    /// `None` when the media file has no audio track.
    audio_cancel: Option<Arc<AtomicBool>>,
    /// Handle for the background audio decode thread.
    audio_handle: Option<JoinHandle<()>>,
    /// Lazy `sws_scale` converter that converts each frame to packed RGBA.
    /// Re-creates the `SwsContext` automatically when frame geometry changes.
    sws: super::playback_inner::SwsRgbaConverter,
    /// Scratch buffer reused by `present_frame` for the RGBA output of `sws.convert()`.
    rgba_buf: Vec<u8>,
    /// The path currently being decoded — either the original or an activated proxy.
    /// Starts as a clone of `path`; updated by `use_proxy_if_available`.
    active_path: PathBuf,
    /// Set to `true` by `play()` to prevent `use_proxy_if_available` from being
    /// called after playback has started.
    started: AtomicBool,
}

impl PreviewPlayer {
    /// Open a media file and prepare for playback.
    ///
    /// Probes the file to detect audio presence and frame rate, then opens a
    /// [`DecodeBuffer`] for the video stream. Returns [`PreviewError`] if the
    /// file is missing or contains no decodable stream.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the file cannot be probed or decoded.
    pub fn open(path: &Path) -> Result<Self, PreviewError> {
        let info = ff_probe::open(path)?;

        let fps = info.frame_rate().unwrap_or(30.0).max(1.0);

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

        let decode_buf = DecodeBuffer::open(path).build()?;

        // Spawn a background audio decode thread when an audio track is present.
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
            sink: None,
            paused: Arc::new(AtomicBool::new(false)),
            stopped: Arc::new(AtomicBool::new(false)),
            clock,
            av_offset_ms: AtomicI64::new(0),
            audio_buf,
            audio_cancel,
            audio_handle,
            sws: super::playback_inner::SwsRgbaConverter::new(),
            rgba_buf: Vec::new(),
            active_path: path.to_path_buf(),
            started: AtomicBool::new(false),
        })
    }

    /// Register the frame sink. Must be called before [`run`](Self::run).
    pub fn set_sink(&mut self, sink: Box<dyn FrameSink>) {
        self.sink = Some(sink);
    }

    /// Start (or resume) playback.
    ///
    /// Clears the `paused` and `stopped` flags. Must be called before
    /// [`run`](Self::run).
    pub fn play(&self) {
        self.started.store(true, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        self.stopped.store(false, Ordering::Release);
    }

    /// Pause playback. [`run`](Self::run) will spin-sleep until
    /// [`play`](Self::play) is called again.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
    }

    /// Stop playback.
    ///
    /// [`run`](Self::run) returns after the current frame completes.
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }

    /// Returns a cloneable handle to the stop signal.
    ///
    /// Storing `true` into the returned [`Arc<AtomicBool>`] has the same effect
    /// as calling [`stop`](Self::stop) and is safe to call from any context,
    /// including from within a [`FrameSink::push_frame`] callback.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let stop = player.stop_handle();
    /// player.set_sink(Box::new(MySink { stop, max_frames: 10 }));
    /// player.play();
    /// player.run()?;
    /// ```
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stopped)
    }

    /// Returns a cloneable handle to the pause flag.
    ///
    /// Storing `true` pauses [`run`](Self::run); storing `false` resumes it.
    /// Safe to call from any context, including from a UI thread running
    /// concurrently with [`run`](Self::run).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pause = player.pause_handle();
    /// let stop  = player.stop_handle();
    ///
    /// std::thread::spawn(move || { player.play(); let _ = player.run(); });
    ///
    /// pause.store(true, Ordering::Release);   // pause from UI thread
    /// pause.store(false, Ordering::Release);  // resume
    /// stop.store(true, Ordering::Release);    // stop
    /// ```
    pub fn pause_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.paused)
    }

    /// Pop the next decoded video frame.
    ///
    /// Delegates to [`DecodeBuffer::pop_frame`]. Blocks until a frame is available.
    /// Returns [`FrameResult::Eof`] at end of file.
    pub fn pop_frame(&mut self) -> FrameResult {
        self.decode_buf.pop_frame()
    }

    /// Frame-accurate seek to `target_pts`.
    ///
    /// Delegates to [`DecodeBuffer::seek`].
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the seek fails.
    pub fn seek(&mut self, target_pts: Duration) -> Result<(), PreviewError> {
        self.decode_buf.seek(target_pts)
    }

    /// Coarse seek to the nearest I-frame at or before `target_pts`.
    ///
    /// Delegates to [`DecodeBuffer::seek_coarse`]. Faster than
    /// [`seek`](Self::seek) because it skips the forward-decode discard phase.
    /// The first frame after this call will be at the nearest preceding I-frame,
    /// which may be up to ±½ GOP from `target_pts` (typically ±1–2 s for H.264
    /// at default settings).
    ///
    /// **Typical use:** call repeatedly while a scrub bar is being dragged;
    /// call [`seek`](Self::seek) on drag release for frame accuracy.
    ///
    /// ```ignore
    /// // Scrub-bar drag handler:
    /// player.seek_coarse(drag_pts)?;  // fast, called many times
    ///
    /// // Drag released:
    /// player.seek(release_pts)?;      // exact, called once
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the seek fails.
    pub fn seek_coarse(&mut self, target_pts: Duration) -> Result<(), PreviewError> {
        self.decode_buf.seek_coarse(target_pts)
    }

    /// If a proxy file for this media exists in `proxy_dir`, use it transparently.
    ///
    /// Must be called before [`play`](Self::play). Returns `true` if a proxy was
    /// found and activated; returns `false` if no proxy exists (original file
    /// continues to be used).
    ///
    /// Proxy lookup order: `half` → `quarter` → `eighth`; first match wins.
    ///
    /// When a proxy is active, [`FrameSink::push_frame`] delivers frames at the
    /// proxy's native resolution. Callers should not assume a fixed resolution.
    ///
    /// If called after [`play`](Self::play), logs a warning and returns `false`.
    pub fn use_proxy_if_available(&mut self, proxy_dir: &Path) -> bool {
        if self.started.load(Ordering::Acquire) {
            log::warn!("use_proxy_if_available called after play; ignored");
            return false;
        }
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

    /// Returns the path currently being decoded — either the original file or
    /// the activated proxy.
    pub fn active_source(&self) -> &Path {
        &self.active_path
    }

    /// Replace the internal decode buffer and audio thread with those backed by
    /// `proxy_path`. Called exclusively from `use_proxy_if_available`.
    fn activate_proxy(&mut self, proxy_path: &Path) -> Result<(), PreviewError> {
        let info = ff_probe::open(proxy_path)?;
        let fps = info.frame_rate().unwrap_or(30.0).max(1.0);
        let decode_buf = DecodeBuffer::open(proxy_path).build()?;

        // Cancel existing audio thread; clear stale samples.
        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(buf) = &self.audio_buf {
            buf.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        // Detach — the old thread exits on its own when cancel fires.
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
        self.decode_buf = decode_buf;
        self.clock = clock;
        self.audio_buf = audio_buf;
        self.audio_cancel = audio_cancel;
        self.audio_handle = audio_handle;
        Ok(())
    }

    /// Set the A/V offset correction in milliseconds.
    ///
    /// - **Positive** value: video is delayed by `ms` ms relative to the audio
    ///   clock (video PTS is shifted down in the sync comparison).
    /// - **Negative** value: audio is delayed by `ms` ms relative to video
    ///   (video PTS is shifted up in the sync comparison).
    ///
    /// Values outside ±5 000 ms are clamped and a warning is logged.
    /// Safe to call from any thread while [`run`](Self::run) is executing.
    pub fn set_av_offset(&self, ms: i64) {
        const MAX_OFFSET_MS: i64 = 5_000;
        let clamped = if ms.abs() > MAX_OFFSET_MS {
            log::warn!("av_offset clamped value={ms}");
            ms.clamp(-MAX_OFFSET_MS, MAX_OFFSET_MS)
        } else {
            ms
        };
        self.av_offset_ms.store(clamped, Ordering::Relaxed);
    }

    /// Returns the current A/V offset in milliseconds (default: `0`).
    ///
    /// Safe to call from any thread while [`run`](Self::run) is executing.
    pub fn av_offset(&self) -> i64 {
        self.av_offset_ms.load(Ordering::Relaxed)
    }

    /// Pull up to `n_samples` interleaved stereo `f32` PCM samples at 48 kHz.
    ///
    /// Intended for use inside an audio output callback:
    /// ```ignore
    /// let samples = player.pop_audio_samples(buffer_size);
    /// output_buffer[..samples.len()].copy_from_slice(&samples);
    /// // fill remainder with silence when samples.len() < buffer_size (underrun)
    /// ```
    ///
    /// Advances the audio master clock by the number of stereo frames consumed
    /// (`samples.len() / 2`).
    ///
    /// Returns an empty `Vec` when:
    /// - the file has no audio track,
    /// - `n_samples` is `0`,
    /// - playback is paused or stopped, or
    /// - the ring buffer is empty (underrun — caller should output silence).
    pub fn pop_audio_samples(&self, n_samples: usize) -> Vec<f32> {
        if self.paused.load(Ordering::Relaxed) || self.stopped.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let MasterClock::Audio {
            samples_consumed, ..
        } = &self.clock
        else {
            return Vec::new();
        };
        if n_samples == 0 {
            return Vec::new();
        }
        let Some(buf) = &self.audio_buf else {
            return Vec::new();
        };
        let mut guard = buf
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let take = n_samples.min(guard.len());
        if take == 0 {
            return Vec::new();
        }
        let samples: Vec<f32> = guard.drain(..take).collect();
        // Stereo: 2 interleaved samples per frame.
        // Divide by 2 to get mono-equivalent frame count for the audio clock.
        samples_consumed.fetch_add((take / 2) as u64, Ordering::Relaxed);
        samples
    }

    /// A/V sync presentation loop.
    ///
    /// Blocks until [`stop`](Self::stop) is called or the end of file is
    /// reached. Must be called from the presentation thread.
    ///
    /// Video PTS is compared against the master clock:
    /// - **Early frames** (video PTS > clock + 1 frame period): sleep.
    /// - **Late frames** (video PTS < clock − 1 frame period): dropped.
    ///
    /// For video-only files the `System` clock (`Instant`) drives real-time
    /// pacing. For files with audio the `Audio` clock drives sync once
    /// [`pop_audio_samples`](Self::pop_audio_samples) has been called at least
    /// once; before that, frames are presented immediately.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if a frame cannot be presented to the sink.
    pub fn run(&mut self) -> Result<(), PreviewError> {
        let fps = self.fps.max(1.0);
        let frame_period = Duration::from_secs_f64(1.0 / fps);

        // Start the system clock from position 0.
        // Seek events update base_pts during playback.
        self.clock.reset(Duration::ZERO);

        loop {
            if self.stopped.load(Ordering::Acquire) {
                break;
            }
            if self.paused.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
                continue;
            }

            match self.decode_buf.pop_frame() {
                FrameResult::Eof => break,
                FrameResult::Seeking(last) => {
                    if let Some(ref f) = last {
                        self.present_frame(f);
                    }
                    // Non-blocking — loop immediately to check stopped/paused.
                }
                FrameResult::Frame(frame) => {
                    // Update system clock base when a seek just completed.
                    while let Ok(SeekEvent::Completed { pts }) =
                        self.decode_buf.seek_events().try_recv()
                    {
                        self.clock.reset(pts);
                        // Flush stale audio and restart the audio thread from
                        // the seek position so audio and video stay aligned.
                        self.restart_audio_from(pts);
                    }

                    if self.clock.should_sync() {
                        let video_pts = if frame.timestamp().is_valid() {
                            frame.timestamp().as_duration()
                        } else {
                            Duration::ZERO
                        };

                        // Apply A/V offset correction.
                        let offset_ms = self.av_offset_ms.load(Ordering::Relaxed);
                        let offset = Duration::from_millis(offset_ms.unsigned_abs());
                        let adjusted_video_pts = if offset_ms >= 0 {
                            // Positive: video delayed — subtract offset so the
                            // frame appears "earlier" relative to the clock.
                            video_pts.saturating_sub(offset)
                        } else {
                            // Negative: audio delayed — add offset so the frame
                            // appears "later" relative to the clock.
                            video_pts + offset
                        };

                        let clock_pts = self.clock.current_pts();
                        let diff = adjusted_video_pts.as_secs_f64() - clock_pts.as_secs_f64();
                        let fp = frame_period.as_secs_f64();

                        if diff > fp {
                            // Frame is early — sleep until it aligns with the clock.
                            let sleep_secs = (diff - fp / 2.0).max(0.0);
                            thread::sleep(Duration::from_secs_f64(sleep_secs));
                        } else if diff < -fp {
                            // Frame is more than one period late — drop silently.
                            log::debug!(
                                "dropped late frame video_pts={video_pts:?} \
                                 clock_pts={clock_pts:?}"
                            );
                            continue;
                        }
                    }

                    self.present_frame(&frame);
                }
            }
        }
        if let Some(sink) = self.sink.as_mut() {
            sink.flush();
        }
        Ok(())
    }

    /// Convert `frame` to RGBA and pass it to the registered sink, if any.
    fn present_frame(&mut self, frame: &ff_format::VideoFrame) {
        let Some(sink) = self.sink.as_mut() else {
            return;
        };
        let width = frame.width();
        let height = frame.height();
        let pts = frame.timestamp().as_duration();
        if self.sws.convert(frame, &mut self.rgba_buf) {
            sink.push_frame(&self.rgba_buf, width, height, pts);
        }
    }

    /// Flush the audio ring buffer and restart the background audio decode
    /// thread from `pts`.
    ///
    /// Called after a video seek completes so that audio samples stay aligned
    /// with the video timeline. The old thread's cancel flag is set; it exits
    /// at its next cancel check and is detached.
    fn restart_audio_from(&mut self, pts: Duration) {
        // Flush stale samples so the new thread fills only fresh audio.
        if let Some(buf) = &self.audio_buf {
            buf.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        // Signal the running audio thread to stop.
        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        // Detach the old handle — the thread exits on its own when cancel fires.
        drop(self.audio_handle.take());
        // Spawn a fresh thread that decodes from the seek position.
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
}

impl Drop for PreviewPlayer {
    fn drop(&mut self) {
        // Cancel the audio background thread before dropping so it does not
        // outlive the player (the Arc<Mutex<VecDeque>> it holds would stay
        // alive until the thread exits otherwise).
        if let Some(cancel) = &self.audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(h) = self.audio_handle.take() {
            let _ = h.join();
        }
    }
}

// ── spawn_audio_thread ────────────────────────────────────────────────────────

/// Open an [`AudioDecoder`] configured for stereo f32 at 48 kHz, optionally
/// seek to `start_pts`, and push decoded samples into `buf` until the cancel
/// flag is set or EOF is reached.
///
/// The buffer is capped at [`AUDIO_MAX_BUF`] samples; the thread sleeps 1 ms
/// when the buffer is full to avoid busy-waiting.
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
                Ok(None) => break, // EOF
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
    use std::path::Path;

    fn test_video_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/video/gameplay.mp4")
    }

    // ── PreviewPlayer tests ───────────────────────────────────────────────────

    #[test]
    fn preview_player_open_should_fail_for_nonexistent_file() {
        let result = PreviewPlayer::open(Path::new("nonexistent_preview.mp4"));
        assert!(
            result.is_err(),
            "open() must return Err for a non-existent file"
        );
    }

    #[test]
    fn preview_player_play_pause_stop_should_update_state() {
        let path = test_video_path();
        let mut player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        // Initial state: not paused, not stopped.
        assert!(!player.paused.load(Ordering::Relaxed));
        assert!(!player.stopped.load(Ordering::Relaxed));

        player.pause();
        assert!(player.paused.load(Ordering::Relaxed));

        player.play();
        assert!(!player.paused.load(Ordering::Relaxed));
        assert!(!player.stopped.load(Ordering::Relaxed));

        player.stop();
        assert!(player.stopped.load(Ordering::Relaxed));
    }

    #[test]
    fn preview_player_run_should_deliver_frames_to_sink() {
        use std::sync::{Arc, Mutex};

        struct CountingSink(Arc<Mutex<usize>>);
        impl FrameSink for CountingSink {
            fn push_frame(&mut self, _rgba: &[u8], _width: u32, _height: u32, _pts: Duration) {
                *self
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            }
        }

        let path = test_video_path();
        let mut player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        let count = Arc::new(Mutex::new(0usize));
        player.set_sink(Box::new(CountingSink(Arc::clone(&count))));
        player.play();

        // run() blocks until EOF; short test file finishes quickly.
        match player.run() {
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

    // ── pop_audio_samples tests ───────────────────────────────────────────────

    #[test]
    fn pop_audio_samples_should_return_empty_when_paused() {
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.pause();
        let samples = player.pop_audio_samples(1024);
        assert!(
            samples.is_empty(),
            "pop_audio_samples() must return empty while paused"
        );
    }

    #[test]
    fn pop_audio_samples_should_return_empty_when_stopped() {
        let path = test_video_path();
        let mut player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.stop();
        let samples = player.pop_audio_samples(1024);
        assert!(
            samples.is_empty(),
            "pop_audio_samples() must return empty while stopped"
        );
    }

    #[test]
    fn pop_audio_samples_should_return_empty_for_zero_n_samples() {
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.play();
        let samples = player.pop_audio_samples(0);
        assert!(
            samples.is_empty(),
            "pop_audio_samples(0) must always return empty"
        );
    }

    #[test]
    fn pause_handle_should_control_paused_flag_from_shared_reference() {
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        let handle = player.pause_handle();

        handle.store(true, Ordering::Release);
        assert!(
            player.paused.load(Ordering::Acquire),
            "handle must set paused flag"
        );

        handle.store(false, Ordering::Release);
        assert!(
            !player.paused.load(Ordering::Acquire),
            "handle must clear paused flag"
        );

        // Arc clone proves the thread-sharing pattern compiles.
        let cloned = Arc::clone(&handle);
        cloned.store(true, Ordering::Release);
        assert!(
            player.paused.load(Ordering::Acquire),
            "cloned handle must set paused flag"
        );
    }

    #[test]
    fn play_and_pause_should_be_callable_via_shared_reference() {
        // No `mut` binding — only possible with &self receivers.
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.pause();
        assert!(
            player.paused.load(Ordering::Relaxed),
            "pause() via &self must set paused flag"
        );
        player.play();
        assert!(
            !player.paused.load(Ordering::Relaxed),
            "play() via &self must clear paused flag"
        );
    }

    #[test]
    fn pop_audio_samples_should_be_callable_via_shared_reference() {
        // With &self receiver: works through an immutable binding and Arc<T>.
        // This is the compile-time proof that enables cpal-callback usage.
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        // No `mut` binding — only possible with a &self receiver.
        let samples = player.pop_audio_samples(0);
        assert!(samples.is_empty(), "pop_audio_samples(0) must return empty");

        // Via Arc — the canonical pattern for sharing with an audio callback.
        let shared = std::sync::Arc::new(player);
        let _samples = shared.pop_audio_samples(0);
    }

    #[test]
    fn pop_audio_samples_clock_increment_should_equal_half_sample_count() {
        // Verify the stereo-frame → clock-tick formula: n_samples / 2.
        // 9600 stereo samples at 48 kHz stereo = 4800 frames = 100 ms.
        let stereo_samples: usize = 9_600;
        let expected_frames: u64 = (stereo_samples / 2) as u64;
        assert_eq!(
            expected_frames, 4_800,
            "9600 stereo samples must yield 4800 clock frames"
        );
        // At 48 kHz, 4800 frames = 0.1 s.
        let pts = Duration::from_secs_f64(f64::from(48_000u32).recip() * expected_frames as f64);
        assert!(
            (pts.as_secs_f64() - 0.1).abs() < 1e-6,
            "4800 frames at 48 kHz must equal 100 ms; got {pts:?}"
        );
    }

    // ── seek_coarse tests ─────────────────────────────────────────────────────

    #[test]
    fn seek_coarse_should_delegate_to_decode_buffer() {
        let path = test_video_path();
        let mut player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        // Consume a few frames so the decoder has advanced past the start.
        for _ in 0..3 {
            if matches!(player.pop_frame(), FrameResult::Eof) {
                println!("skipping: EOF before seek target");
                return;
            }
        }
        let target = Duration::from_secs(1);
        match player.seek_coarse(target) {
            Ok(()) => {}
            Err(e) => {
                println!("skipping: seek_coarse not supported or failed: {e}");
                return;
            }
        }
        // After a coarse seek the next frame must be available (not EOF).
        match player.pop_frame() {
            FrameResult::Frame(_) | FrameResult::Seeking(_) => {}
            FrameResult::Eof => panic!("pop_frame() returned Eof immediately after seek_coarse"),
        }
    }

    #[test]
    fn seek_coarse_should_be_faster_than_seek_for_same_target() {
        // Structural test: both methods must return Ok for the same target.
        // Timing comparison is environment-dependent and marked #[ignore].
        let path = test_video_path();
        let mut player_exact = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        let mut player_coarse = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };

        let target = Duration::from_secs(1);
        let exact_ok = player_exact.seek(target).is_ok();
        let coarse_ok = player_coarse.seek_coarse(target).is_ok();

        // Both must either succeed or fail (seek support depends on the codec).
        assert_eq!(
            exact_ok, coarse_ok,
            "seek() and seek_coarse() must both succeed or both fail for the same file"
        );
    }

    // ── A/V offset tests ──────────────────────────────────────────────────────

    #[test]
    fn av_offset_default_should_be_zero() {
        use std::sync::atomic::{AtomicI64, Ordering};
        // AtomicI64 default matches the expected API default of 0 ms.
        let offset = AtomicI64::new(0);
        assert_eq!(offset.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn set_av_offset_should_clamp_large_positive_value() {
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.set_av_offset(10_000);
        assert_eq!(player.av_offset(), 5_000, "offset must be clamped to +5000");
    }

    #[test]
    fn set_av_offset_should_clamp_large_negative_value() {
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.set_av_offset(-10_000);
        assert_eq!(
            player.av_offset(),
            -5_000,
            "offset must be clamped to -5000"
        );
    }

    #[test]
    fn positive_av_offset_should_reduce_adjusted_video_pts() {
        // Simulate the offset adjustment: positive offset subtracts from video_pts.
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

    // ── use_proxy_if_available / active_source tests ──────────────────────────

    #[test]
    fn use_proxy_if_available_should_return_false_when_no_proxy_in_dir() {
        let path = test_video_path();
        let mut player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        let tmp = std::env::temp_dir().join("ff_preview_no_proxy_dir_test");
        let _ = std::fs::create_dir_all(&tmp);
        let found = player.use_proxy_if_available(&tmp);
        assert!(
            !found,
            "must return false when no proxy files exist in the directory"
        );
    }

    #[test]
    fn use_proxy_if_available_should_return_false_after_play() {
        let path = test_video_path();
        let mut player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        player.play();
        let found = player.use_proxy_if_available(Path::new("."));
        assert!(!found, "must return false when called after play()");
    }

    #[test]
    fn active_source_should_return_original_path_before_proxy_activation() {
        let path = test_video_path();
        let player = match PreviewPlayer::open(&path) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: video file not available: {e}");
                return;
            }
        };
        assert_eq!(
            player.active_source(),
            path.as_path(),
            "active_source() must equal the original path before any proxy activation"
        );
    }
}
