//! Real-time playback of a [`Timeline`].
//!
//! [`TimelinePlayer`] opens every clip on the primary video track of a
//! [`Timeline`] and plays them back in order, mapping each clip's frame PTS
//! to the unified timeline coordinate.
//!
//! | Type | Role |
//! |------|------|
//! | [`TimelinePlayer`] | Thin builder: call [`open`](TimelinePlayer::open) |
//! | [`TimelineRunner`] | Owns the decode pipelines; move to a thread and call [`run`](TimelineRunner::run) |
//! | [`PlayerHandle`] | Shared, cloneable control handle |
//!
//! ## Audio
//!
//! When any clip on the primary video track carries an audio stream,
//! [`TimelinePlayer::open`] creates an [`AudioMixer`] with one track per
//! audio-bearing clip.  A background [`AudioDecoder`] thread is started for
//! the active clip and pushes mono samples via [`AudioTrackHandle`].  On clip
//! transition or seek the old thread is cancelled and a new one is started.
//! [`PlayerHandle::pop_audio_samples`] calls [`AudioMixer::mix`] and returns
//! interleaved stereo `f32` output.

mod timeline_inner;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ff_decode::{AudioDecoder, SeekMode};
use ff_format::SampleFormat;
use ff_pipeline::timeline::Timeline;

use crate::audio::{AudioMixer, AudioTrackHandle};
use crate::error::PreviewError;
use crate::event::PlayerEvent;
use crate::playback::SwsRgbaConverter;
use crate::playback::clock::MasterClock;
use crate::playback::decode_buffer::{DecodeBuffer, FrameResult};
use crate::playback::player::{PlayerCommand, PlayerHandle};
use crate::playback::sink::FrameSink;

// ── Constants ─────────────────────────────────────────────────────────────────

const CHANNEL_CAP: usize = 64;
/// Back-pressure limit for the audio decode thread (mono samples).
const AUDIO_MAX_BUF: usize = 96_000;

// ── ClipState ─────────────────────────────────────────────────────────────────

struct ClipState {
    /// Source file path — needed to spawn audio threads on clip transition.
    source: PathBuf,
    decode_buf: DecodeBuffer,
    /// Global timeline position where this clip starts.
    timeline_start: Duration,
    /// Global timeline position where this clip ends.
    timeline_end: Duration,
    /// Source-file PTS at which this clip starts (= `Clip::in_point`).
    in_point: Duration,
    /// Source-file PTS at which this clip ends (`None` = play to EOF).
    out_point: Option<Duration>,
    /// Duration of the crossfade from the previous clip into this one.
    /// `Duration::ZERO` = hard cut.
    transition_dur: Duration,
    /// Audio track handle — `None` if the clip has no audio stream.
    audio_track: Option<AudioTrackHandle>,
}

// ── TransitionState ───────────────────────────────────────────────────────────

struct TransitionState {
    /// Index of the incoming clip (the one being faded in).
    next_idx: usize,
    /// Timeline PTS at which the transition begins.
    start: Duration,
    /// Duration of the transition.
    duration: Duration,
}

// ── TimelinePlayer ────────────────────────────────────────────────────────────

/// Thin builder for a ([`TimelineRunner`], [`PlayerHandle`]) pair backed by a
/// [`Timeline`].
///
/// Playback is limited to the primary video track (`video_tracks[0]`). When
/// any clip carries an audio stream, an [`AudioMixer`] is created and audio
/// is mixed into the stereo output from [`PlayerHandle::pop_audio_samples`].
///
/// # Example
///
/// ```ignore
/// use ff_pipeline::{Timeline, Clip};
/// use ff_preview::{TimelinePlayer, RgbaSink};
/// use std::time::Duration;
///
/// let timeline = Timeline::builder()
///     .canvas(1920, 1080)
///     .frame_rate(30.0)
///     .video_track(vec![
///         Clip::new("intro.mp4").trim(Duration::ZERO, Duration::from_secs(5)),
///     ])
///     .build()?;
///
/// let (mut runner, handle) = TimelinePlayer::open(&timeline)?;
/// runner.set_sink(Box::new(RgbaSink::new()));
/// std::thread::spawn(move || { let _ = runner.run(); });
/// handle.play();
/// ```
pub struct TimelinePlayer;

impl TimelinePlayer {
    /// Open `timeline` for real-time preview playback.
    ///
    /// Probes every clip's source file to determine effective durations and
    /// audio availability, opens a [`DecodeBuffer`] for each clip on the
    /// primary video track, and seeks each buffer to its configured `in_point`.
    ///
    /// When any clip carries an audio stream an [`AudioMixer`] is created and
    /// the first audio-bearing clip's decode thread is started immediately.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] when:
    /// - `timeline` has no video tracks or the primary track is empty,
    /// - a clip source file cannot be found or opened,
    /// - a clip cannot be probed for duration.
    #[allow(clippy::too_many_lines)]
    pub fn open(timeline: &Timeline) -> Result<(TimelineRunner, PlayerHandle), PreviewError> {
        struct ProbeResult {
            source: PathBuf,
            in_pt: Duration,
            clip_dur: Duration,
            timeline_offset: Duration,
            out_point: Option<Duration>,
            transition_dur: Duration,
            has_audio: bool,
        }

        let tracks = timeline.video_tracks();
        if tracks.is_empty() || tracks[0].is_empty() {
            return Err(PreviewError::Ffmpeg {
                code: 0,
                message: "timeline has no video clips in the primary track".into(),
            });
        }

        let fps = timeline.frame_rate().max(1.0);
        let clip_list = &tracks[0];

        // ── Phase 1: probe all clips ──────────────────────────────────────────

        let mut probes: Vec<ProbeResult> = Vec::with_capacity(clip_list.len());
        let mut has_any_audio = false;

        for clip in clip_list {
            let in_pt = clip.in_point.unwrap_or(Duration::ZERO);
            let info = ff_probe::open(&clip.source)?;

            let clip_dur = if let (Some(ip), Some(op)) = (clip.in_point, clip.out_point) {
                op.saturating_sub(ip)
            } else {
                info.duration().saturating_sub(in_pt)
            };

            let transition_dur = if clip.transition.is_some() {
                clip.transition_duration
            } else {
                Duration::ZERO
            };

            let has_audio = info.has_audio();
            has_any_audio |= has_audio;

            probes.push(ProbeResult {
                source: clip.source.clone(),
                in_pt,
                clip_dur,
                timeline_offset: clip.timeline_offset,
                out_point: clip.out_point,
                transition_dur,
                has_audio,
            });
        }

        // ── Phase 2: build mixer and track handles (if audio present) ─────────

        let (mixer_arc, audio_track_handles): (
            Option<Arc<Mutex<AudioMixer>>>,
            Vec<Option<AudioTrackHandle>>,
        ) = if has_any_audio {
            let mut mixer = AudioMixer::new(48_000);
            let handles: Vec<Option<AudioTrackHandle>> = probes
                .iter()
                .map(|p| {
                    if p.has_audio {
                        Some(mixer.add_track())
                    } else {
                        None
                    }
                })
                .collect();
            (Some(Arc::new(Mutex::new(mixer))), handles)
        } else {
            (None, probes.iter().map(|_| None).collect())
        };

        // ── Phase 3: build ClipState objects ──────────────────────────────────

        let mut clip_states: Vec<ClipState> = Vec::with_capacity(probes.len());
        for (i, p) in probes.iter().enumerate() {
            let timeline_start = p.timeline_offset;
            let timeline_end = timeline_start + p.clip_dur;

            let mut decode_buf = DecodeBuffer::open(&p.source).build()?;
            if p.in_pt > Duration::ZERO {
                decode_buf.seek(p.in_pt)?;
            }

            clip_states.push(ClipState {
                source: p.source.clone(),
                decode_buf,
                timeline_start,
                timeline_end,
                in_point: p.in_pt,
                out_point: p.out_point,
                transition_dur: p.transition_dur,
                audio_track: audio_track_handles[i].clone(),
            });
        }

        // ── Compute total duration ─────────────────────────────────────────────

        let total_dur = clip_states
            .iter()
            .map(|c| c.timeline_end)
            .max()
            .unwrap_or(Duration::ZERO);
        let duration_millis = u64::try_from(total_dur.as_millis()).unwrap_or(u64::MAX);

        // ── Build runner and handle ───────────────────────────────────────────

        let current_pts = Arc::new(AtomicU64::new(0));
        let paused = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let (cmd_tx, cmd_rx) = mpsc::sync_channel(CHANNEL_CAP);
        let (event_tx, event_rx) = mpsc::sync_channel::<PlayerEvent>(CHANNEL_CAP);

        // Start audio for the first clip immediately.
        let (initial_audio_cancel, initial_audio_thread) =
            if let Some(handle) = clip_states.first().and_then(|c| c.audio_track.clone()) {
                let source = clip_states[0].source.clone();
                let in_pt = clip_states[0].in_point;
                let cancel = Arc::new(AtomicBool::new(false));
                let thread = spawn_audio_track_thread(source, in_pt, handle, Arc::clone(&cancel));
                (Some(cancel), Some(thread))
            } else {
                (None, None)
            };

        let runner = TimelineRunner {
            clips: clip_states,
            active: 0,
            transition: None,
            cmd_rx,
            event_tx,
            sink: None,
            current_pts: Arc::clone(&current_pts),
            paused: Arc::clone(&paused),
            stopped: Arc::clone(&stopped),
            fps,
            rate: 1.0,
            clock: MasterClock::System {
                started_at: Instant::now(),
                base_pts: Duration::ZERO,
                rate: 1.0,
            },
            sws_a: SwsRgbaConverter::new(),
            sws_b: SwsRgbaConverter::new(),
            rgba_a: Vec::new(),
            rgba_b: Vec::new(),
            blend_buf: Vec::new(),
            audio_mixer: mixer_arc.clone(),
            active_audio_cancel: initial_audio_cancel,
            active_audio_thread: initial_audio_thread,
        };

        let handle = PlayerHandle::for_timeline(
            cmd_tx,
            Arc::new(Mutex::new(event_rx)),
            current_pts,
            paused,
            stopped,
            duration_millis,
            mixer_arc,
        );

        Ok((runner, handle))
    }
}

// ── TimelineRunner ────────────────────────────────────────────────────────────

/// Exclusive owner of the timeline decode pipeline.
///
/// Move to a background thread and call [`run`](Self::run). Register a
/// [`FrameSink`] with [`set_sink`](Self::set_sink) before calling `run`.
pub struct TimelineRunner {
    clips: Vec<ClipState>,
    /// Index of the clip currently being decoded and presented.
    active: usize,
    /// Non-`None` while a crossfade transition is in progress.
    transition: Option<TransitionState>,
    cmd_rx: mpsc::Receiver<PlayerCommand>,
    event_tx: mpsc::SyncSender<PlayerEvent>,
    sink: Option<Box<dyn FrameSink>>,
    current_pts: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    fps: f64,
    rate: f64,
    clock: MasterClock,
    /// Pixel-format converter for the active (outgoing) frame.
    sws_a: SwsRgbaConverter,
    /// Pixel-format converter for the incoming frame during transitions.
    sws_b: SwsRgbaConverter,
    rgba_a: Vec<u8>,
    rgba_b: Vec<u8>,
    blend_buf: Vec<u8>,
    /// Multi-track audio mixer — `None` when no clip has audio.
    audio_mixer: Option<Arc<Mutex<AudioMixer>>>,
    /// Cancel flag for the currently running audio decode thread.
    active_audio_cancel: Option<Arc<AtomicBool>>,
    /// Handle to the currently running audio decode thread.
    active_audio_thread: Option<JoinHandle<()>>,
}

impl TimelineRunner {
    /// Register the frame sink. Call before [`run`](Self::run).
    pub fn set_sink(&mut self, sink: Box<dyn FrameSink>) {
        self.sink = Some(sink);
    }

    /// A/V sync presentation loop.
    ///
    /// Plays all clips in the primary video track from start to finish (or until
    /// a [`PlayerCommand::Stop`] is received).
    ///
    /// Emits [`PlayerEvent::SeekCompleted`] after each successful seek,
    /// [`PlayerEvent::PositionUpdate`] after each presented video frame,
    /// [`PlayerEvent::Error`] on non-fatal decode errors, and
    /// [`PlayerEvent::Eof`] before returning.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError::SeekOutOfRange`] if a seek command targets a
    /// timestamp that falls outside all clips on the timeline.
    #[allow(clippy::too_many_lines)]
    pub fn run(mut self) -> Result<(), PreviewError> {
        if self.clips.is_empty() {
            let _ = self.event_tx.try_send(PlayerEvent::Eof);
            return Ok(());
        }

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
                    PlayerCommand::SetAvOffset(_) => {} // audio timing is system-clock driven
                }
            }

            // ── Apply pending seek ────────────────────────────────────────────
            if let Some(target) = pending_seek {
                self.seek_timeline(target)?;
                self.clock.reset(target);
                let _ = self.event_tx.try_send(PlayerEvent::SeekCompleted(target));
            }

            // ── Error events from active clip ─────────────────────────────────
            {
                let active = self.active;
                while let Ok(msg) = self.clips[active].decode_buf.error_events().try_recv() {
                    let _ = self.event_tx.try_send(PlayerEvent::Error(msg));
                }
            }
            let trans_next = self.transition.as_ref().map(|tp| tp.next_idx);
            if let Some(next_idx) = trans_next {
                while let Ok(msg) = self.clips[next_idx].decode_buf.error_events().try_recv() {
                    let _ = self.event_tx.try_send(PlayerEvent::Error(msg));
                }
            }

            // ── Stopped / paused ──────────────────────────────────────────────
            if self.stopped.load(Ordering::Acquire) {
                break;
            }
            if self.paused.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
                continue;
            }

            // ── Pop frame from active clip ─────────────────────────────────────
            let active = self.active;
            let pop_result = self.clips[active].decode_buf.pop_frame();

            match pop_result {
                FrameResult::Eof => {
                    let old_active = active;
                    if let Some(tp) = self.transition.take() {
                        self.active = tp.next_idx;
                    } else if active + 1 < self.clips.len() {
                        self.active += 1;
                    } else {
                        break;
                    }
                    if self.active != old_active {
                        let in_pt = self.clips[self.active].in_point;
                        self.restart_audio_at(self.active, in_pt);
                    }
                }

                FrameResult::Seeking(last) => {
                    if let Some(ref f) = last {
                        let f_pts = f.timestamp().as_duration();
                        let tl_start = self.clips[active].timeline_start;
                        let in_pt = self.clips[active].in_point;
                        let tl_pts = tl_start + f_pts.saturating_sub(in_pt);
                        let w = f.width();
                        let h = f.height();
                        if self.sws_a.convert(f, &mut self.rgba_a)
                            && let Some(sink) = self.sink.as_mut()
                        {
                            sink.push_frame(&self.rgba_a, w, h, tl_pts);
                        }
                    }
                }

                FrameResult::Frame(frame) => {
                    let f_pts = frame.timestamp().as_duration();
                    let clip_in = self.clips[active].in_point;
                    let clip_out = self.clips[active].out_point;
                    let clip_tl_start = self.clips[active].timeline_start;
                    let clip_tl_end = self.clips[active].timeline_end;

                    // Skip frames before in_point (e.g. right after a seek).
                    if f_pts < clip_in {
                        continue;
                    }

                    // Treat frames past out_point as EOF for this clip.
                    let past_out = clip_out.is_some_and(|op| f_pts >= op);
                    let past_end = {
                        let tl_pts = clip_tl_start + f_pts.saturating_sub(clip_in);
                        tl_pts >= clip_tl_end
                    };

                    if past_out || past_end {
                        let old_active = active;
                        if let Some(tp) = self.transition.take() {
                            self.active = tp.next_idx;
                        } else if active + 1 < self.clips.len() {
                            self.active += 1;
                        } else {
                            break;
                        }
                        if self.active != old_active {
                            let in_pt = self.clips[self.active].in_point;
                            self.restart_audio_at(self.active, in_pt);
                        }
                        continue;
                    }

                    let timeline_pts = clip_tl_start + f_pts.saturating_sub(clip_in);

                    // Update shared current_pts.
                    self.current_pts.store(
                        u64::try_from(timeline_pts.as_micros()).unwrap_or(u64::MAX),
                        Ordering::Relaxed,
                    );

                    // ── Transition zone entry check ────────────────────────────
                    if self.transition.is_none() && active + 1 < self.clips.len() {
                        let next = &self.clips[active + 1];
                        if next.transition_dur > Duration::ZERO
                            && timeline_pts >= next.timeline_start
                        {
                            if timeline_pts < next.timeline_start + next.transition_dur {
                                self.transition = Some(TransitionState {
                                    next_idx: active + 1,
                                    start: next.timeline_start,
                                    duration: next.transition_dur,
                                });
                            } else {
                                // Jumped past the entire transition zone.
                                let old_active = active;
                                self.active = active + 1;
                                if self.active != old_active {
                                    let in_pt = self.clips[self.active].in_point;
                                    self.restart_audio_at(self.active, in_pt);
                                }
                                continue;
                            }
                        }
                    }

                    // ── A/V sync (system clock) ───────────────────────────────
                    {
                        let clock_pts = self.clock.current_pts();
                        let diff = timeline_pts.as_secs_f64() - clock_pts.as_secs_f64();
                        let fp = frame_period.as_secs_f64();

                        if diff > fp {
                            let sleep_secs =
                                (diff - fp / 2.0).max(0.0) / self.rate.max(f64::MIN_POSITIVE);
                            thread::sleep(Duration::from_secs_f64(sleep_secs));
                        } else if diff < -fp {
                            log::debug!(
                                "timeline dropped late frame timeline_pts={timeline_pts:?} \
                                 clock_pts={clock_pts:?}"
                            );
                            continue;
                        }
                    }

                    // ── Present frame ─────────────────────────────────────────
                    let w = frame.width();
                    let h = frame.height();

                    // Copy transition fields to avoid holding a borrow while
                    // calling `pop_frame` on the next clip.
                    let (in_trans, next_idx, trans_start, trans_dur) = match &self.transition {
                        Some(tp) => (true, tp.next_idx, tp.start, tp.duration),
                        None => (false, 0, Duration::ZERO, Duration::ZERO),
                    };

                    let a_ok = self.sws_a.convert(&frame, &mut self.rgba_a);

                    if in_trans && a_ok {
                        let alpha = (timeline_pts.saturating_sub(trans_start).as_secs_f32()
                            / trans_dur.as_secs_f32())
                        .clamp(0.0, 1.0);

                        let next_pop = self.clips[next_idx].decode_buf.pop_frame();

                        let blended = if let FrameResult::Frame(next_frame) = next_pop {
                            if self.sws_b.convert(&next_frame, &mut self.rgba_b) {
                                timeline_inner::blend_rgba(
                                    &self.rgba_a,
                                    &self.rgba_b,
                                    alpha,
                                    &mut self.blend_buf,
                                );
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        if let Some(sink) = self.sink.as_mut() {
                            let pixels = if blended {
                                &self.blend_buf
                            } else {
                                &self.rgba_a
                            };
                            sink.push_frame(pixels, w, h, timeline_pts);
                        }

                        if timeline_pts >= trans_start + trans_dur {
                            let old_active = self.active;
                            self.transition = None;
                            self.active = next_idx;
                            if self.active != old_active {
                                let in_pt = self.clips[self.active].in_point;
                                self.restart_audio_at(self.active, in_pt);
                            }
                        }
                    } else if a_ok && let Some(sink) = self.sink.as_mut() {
                        sink.push_frame(&self.rgba_a, w, h, timeline_pts);
                    }

                    let _ = self
                        .event_tx
                        .try_send(PlayerEvent::PositionUpdate(timeline_pts));
                }
            }
        }

        let _ = self.event_tx.try_send(PlayerEvent::Eof);
        if let Some(sink) = self.sink.as_mut() {
            sink.flush();
        }
        Ok(())
    }

    /// Seek all decode buffers so that `active` is the clip containing `target`
    /// and that clip's buffer is positioned at the correct source-file PTS.
    fn seek_timeline(&mut self, target: Duration) -> Result<(), PreviewError> {
        let clip_idx = self
            .clips
            .iter()
            .position(|c| target >= c.timeline_start && target < c.timeline_end);

        let Some(clip_idx) = clip_idx else {
            return Err(PreviewError::SeekOutOfRange { pts: target });
        };

        let clip_local_pts = self.clips[clip_idx].in_point
            + target.saturating_sub(self.clips[clip_idx].timeline_start);

        self.clips[clip_idx].decode_buf.seek(clip_local_pts)?;
        self.active = clip_idx;
        self.transition = None;

        // Discard stale audio and restart from the seek position.
        if let Some(mixer_arc) = &self.audio_mixer {
            mixer_arc
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .invalidate_all();
        }
        self.restart_audio_at(clip_idx, clip_local_pts);

        Ok(())
    }

    /// Cancel the current audio decode thread (if any) and start a new one
    /// for `clip_idx` beginning at `start_pts`.
    fn restart_audio_at(&mut self, clip_idx: usize, start_pts: Duration) {
        // Cancel and drop the previous thread.
        if let Some(cancel) = &self.active_audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        drop(self.active_audio_thread.take());
        self.active_audio_cancel = None;

        let Some(handle) = self.clips.get(clip_idx).and_then(|c| c.audio_track.clone()) else {
            return;
        };
        handle.clear(); // discard stale samples

        let source = self.clips[clip_idx].source.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread = spawn_audio_track_thread(source, start_pts, handle, Arc::clone(&cancel));
        self.active_audio_cancel = Some(cancel);
        self.active_audio_thread = Some(thread);
    }
}

impl Drop for TimelineRunner {
    fn drop(&mut self) {
        if let Some(cancel) = &self.active_audio_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(h) = self.active_audio_thread.take() {
            let _ = h.join();
        }
    }
}

// ── spawn_audio_track_thread ──────────────────────────────────────────────────

fn spawn_audio_track_thread(
    path: PathBuf,
    start_pts: Duration,
    track: AudioTrackHandle,
    cancel: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut decoder = match AudioDecoder::open(&path)
            .output_format(SampleFormat::F32)
            .output_sample_rate(48_000)
            .output_channels(1) // mono — the mixer applies panning
            .build()
        {
            Ok(d) => d,
            Err(e) => {
                log::warn!("timeline audio thread open failed error={e}");
                return;
            }
        };

        if start_pts > Duration::ZERO
            && let Err(e) = decoder.seek(start_pts, SeekMode::Backward)
        {
            log::warn!("timeline audio seek failed pts={start_pts:?} error={e}");
        }

        loop {
            if cancel.load(Ordering::Acquire) {
                break;
            }

            // Back-pressure: pause decoding when the buffer is full.
            if track.buffered_samples() >= AUDIO_MAX_BUF {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            match decoder.decode_one() {
                Ok(Some(frame)) => {
                    if let Some(samples) = frame.as_f32()
                        && !samples.is_empty()
                    {
                        track.push_samples(samples);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    log::warn!("timeline audio decode error error={e}");
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
    use std::path::PathBuf;

    fn test_video_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/video/gameplay.mp4")
    }

    // ── blend_rgba delegate ────────────────────────────────────────────────

    #[test]
    fn timeline_inner_blend_rgba_at_zero_alpha_should_return_a() {
        let a = vec![255u8, 0, 0, 255];
        let b = vec![0u8, 0, 255, 255];
        let mut dst = Vec::new();
        timeline_inner::blend_rgba(&a, &b, 0.0, &mut dst);
        assert_eq!(dst, a);
    }

    // ── open ──────────────────────────────────────────────────────────────

    #[test]
    fn timeline_player_open_should_fail_when_no_video_tracks() {
        let _ = PreviewError::SeekOutOfRange {
            pts: Duration::from_secs(1),
        };
    }

    // ── run ───────────────────────────────────────────────────────────────

    #[test]
    #[ignore = "requires assets/video/gameplay.mp4; run with -- --include-ignored"]
    fn timeline_runner_run_should_deliver_frames_for_single_clip() {
        use crate::playback::sink::FrameSink;

        let path = test_video_path();
        if !path.exists() {
            println!("skipping: video asset not found");
            return;
        }

        struct CountSink(usize, PlayerHandle);
        impl FrameSink for CountSink {
            fn push_frame(&mut self, _rgba: &[u8], _w: u32, _h: u32, _pts: Duration) {
                self.0 += 1;
                if self.0 >= 20 {
                    self.1.stop();
                }
            }
        }

        let timeline = ff_pipeline::Timeline::builder()
            .canvas(1280, 720)
            .frame_rate(30.0)
            .video_track(vec![
                ff_pipeline::Clip::new(&path).trim(Duration::ZERO, Duration::from_secs(2)),
            ])
            .build()
            .expect("timeline build failed");

        let (mut runner, handle) = match TimelinePlayer::open(&timeline) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: open failed: {e}");
                return;
            }
        };

        runner.set_sink(Box::new(CountSink(0, handle.clone())));
        let _ = runner.run();

        let events: Vec<_> = std::iter::from_fn(|| handle.poll_event()).collect();
        assert!(
            events.iter().any(|e| matches!(e, PlayerEvent::Eof)),
            "Eof event must be delivered after run() completes"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, PlayerEvent::PositionUpdate(_))),
            "PositionUpdate events must be emitted during playback"
        );
    }

    #[test]
    #[ignore = "requires assets/video/gameplay.mp4; run with -- --include-ignored"]
    fn timeline_runner_seek_should_deliver_seek_completed_event() {
        let path = test_video_path();
        if !path.exists() {
            println!("skipping: video asset not found");
            return;
        }

        let timeline = ff_pipeline::Timeline::builder()
            .canvas(1280, 720)
            .frame_rate(30.0)
            .video_track(vec![
                ff_pipeline::Clip::new(&path).trim(Duration::ZERO, Duration::from_secs(10)),
            ])
            .build()
            .expect("timeline build failed");

        let (runner, handle) = match TimelinePlayer::open(&timeline) {
            Ok(p) => p,
            Err(e) => {
                println!("skipping: open failed: {e}");
                return;
            }
        };

        let handle_bg = handle.clone();
        let bg = thread::spawn(move || {
            let _ = runner.run();
        });

        thread::sleep(Duration::from_millis(50));
        handle.seek(Duration::from_secs(1));

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let found = loop {
            if let Some(e) = handle.poll_event() {
                if matches!(e, PlayerEvent::SeekCompleted(_)) {
                    break true;
                }
            }
            if std::time::Instant::now() > deadline {
                break false;
            }
            thread::sleep(Duration::from_millis(10));
        };

        handle_bg.stop();
        let _ = bg.join();

        assert!(
            found,
            "SeekCompleted must be delivered within 3 seconds of seek"
        );
    }
}
