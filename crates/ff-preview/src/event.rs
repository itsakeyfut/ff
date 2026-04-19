//! Player event types emitted by [`PlayerRunner`](crate::playback::player::PlayerRunner).

use std::time::Duration;

/// Events emitted by [`PlayerRunner::run`](crate::playback::player::PlayerRunner::run)
/// and delivered to callers via
/// [`PlayerHandle::poll_event`](crate::playback::player::PlayerHandle::poll_event).
pub enum PlayerEvent {
    /// A seek initiated via [`PlayerHandle::seek`](crate::playback::player::PlayerHandle::seek)
    /// has completed.
    ///
    /// `pts` is the actual presentation timestamp of the first frame available
    /// after the seek, which may differ slightly from the requested target due
    /// to I-frame boundaries.
    SeekCompleted(Duration),

    /// The media file has been fully decoded; `run()` is about to return.
    Eof,
}
