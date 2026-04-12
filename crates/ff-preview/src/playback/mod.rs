//! Real-time playback types for ff-preview.
//!
//! This module exposes the primary public API for single-file video/audio
//! playback. All `unsafe` `FFmpeg` calls are isolated in [`playback_inner`].

mod playback_inner;

/// Drives real-time playback of a single media file.
///
/// `PreviewPlayer` decodes a video/audio file, synchronises video frame
/// presentation to an audio master clock, and delivers RGBA frames to a
/// registered [`FrameSink`].
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

/// A monotonic clock that tracks elapsed playback time.
///
/// The clock supports start, stop, pause, resume, and rate scaling.
/// It is used by `PreviewPlayer` internally; callers may also query it
/// directly via `PreviewPlayer::clock()` (added in a later issue).
///
/// # Usage (stub — full implementation in #370)
///
/// ```ignore
/// let mut clock = PlaybackClock::new();
/// clock.start();
/// let pts = clock.current_time();
/// ```
pub struct PlaybackClock;
