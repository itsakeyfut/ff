//! Timeline clip data type.
//!
//! This module provides [`Clip`], a plain Rust value type representing a single
//! media clip on a timeline. `Clip` holds no `FFmpeg` context; it is interpreted
//! by `Timeline::render()` at call time to build filter graphs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ff_filter::XfadeTransition;

/// A single media clip on a timeline.
///
/// `Clip` is a plain Rust value type — it holds no `FFmpeg` context. All fields
/// are public so callers can inspect them directly. `Timeline::render()` interprets
/// the clip's fields to build filter graphs at call time.
///
/// # Examples
///
/// ```
/// use ff_pipeline::Clip;
/// use std::time::Duration;
///
/// let clip = Clip::new("intro.mp4")
///     .trim(Duration::from_secs(2), Duration::from_secs(10))
///     .offset(Duration::from_secs(5));
///
/// assert_eq!(clip.duration(), Some(Duration::from_secs(8)));
/// ```
#[derive(Debug, Clone)]
pub struct Clip {
    /// Path to the source media file.
    pub source: PathBuf,
    /// Start point within the source file. `None` = beginning of file.
    pub in_point: Option<Duration>,
    /// End point within the source file. `None` = end of file.
    pub out_point: Option<Duration>,
    /// Start offset on the timeline (`Duration::ZERO` = beginning of composition).
    pub timeline_offset: Duration,
    /// Arbitrary key/value metadata attached to this clip.
    pub metadata: HashMap<String, String>,
    /// Transition applied at the start of this clip (from the previous clip on the same track).
    /// `None` = hard cut. Ignored for the first clip on a track.
    pub transition: Option<XfadeTransition>,
    /// Duration of the transition overlap. Ignored when `transition` is `None`.
    pub transition_duration: Duration,
    /// Per-clip volume adjustment in dB applied during audio mixing (`0.0` = unity gain).
    ///
    /// This value is independent of any track-level volume animation. When non-zero
    /// the clip's own gain overrides the track-level value; set to `0.0` to defer
    /// to the track level.
    ///
    /// Defaults to `0.0`.
    pub volume_db: f64,
}

impl Clip {
    /// Creates a new clip from a source path with no trim points and zero timeline offset.
    pub fn new(source: impl AsRef<Path>) -> Self {
        Self {
            source: source.as_ref().to_path_buf(),
            in_point: None,
            out_point: None,
            timeline_offset: Duration::ZERO,
            metadata: HashMap::new(),
            transition: None,
            transition_duration: Duration::ZERO,
            volume_db: 0.0,
        }
    }

    /// Sets the in/out trim points and returns the updated clip.
    #[must_use]
    pub fn trim(self, in_point: Duration, out_point: Duration) -> Self {
        Self {
            in_point: Some(in_point),
            out_point: Some(out_point),
            ..self
        }
    }

    /// Sets the timeline start offset and returns the updated clip.
    #[must_use]
    pub fn offset(self, timeline_offset: Duration) -> Self {
        Self {
            timeline_offset,
            ..self
        }
    }

    /// Sets the visual transition from the previous clip into this one and returns
    /// the updated clip.
    ///
    /// The transition is applied at the boundary where the preceding clip ends and
    /// this clip begins. For the first clip on a track `transition` is ignored.
    ///
    /// # Example
    ///
    /// ```
    /// use ff_pipeline::Clip;
    /// use ff_filter::XfadeTransition;
    /// use std::time::Duration;
    ///
    /// let clip = Clip::new("b.mp4")
    ///     .with_transition(XfadeTransition::Fade, Duration::from_millis(500));
    ///
    /// assert_eq!(clip.transition, Some(XfadeTransition::Fade));
    /// assert_eq!(clip.transition_duration, Duration::from_millis(500));
    /// ```
    #[must_use]
    pub fn with_transition(self, kind: XfadeTransition, duration: Duration) -> Self {
        Self {
            transition: Some(kind),
            transition_duration: duration,
            ..self
        }
    }

    /// Sets the per-clip volume adjustment in dB and returns the updated clip.
    ///
    /// `0.0` is unity gain (no change). Positive values increase volume; negative
    /// values reduce it. When set to a non-zero value this overrides the track-level
    /// volume animation for this clip during rendering.
    ///
    /// # Example
    ///
    /// ```
    /// use ff_pipeline::Clip;
    ///
    /// let clip = Clip::new("narration.wav").volume(-6.0);
    /// assert_eq!(clip.volume_db, -6.0);
    /// ```
    #[must_use]
    pub fn volume(self, db: f64) -> Self {
        Self {
            volume_db: db,
            ..self
        }
    }

    /// Returns `out_point - in_point` when both are `Some`, otherwise `None`.
    ///
    /// Does not open the source file.
    pub fn duration(&self) -> Option<Duration> {
        match (self.in_point, self.out_point) {
            (Some(in_pt), Some(out_pt)) => out_pt.checked_sub(in_pt),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_new_should_have_zero_offset() {
        let clip = Clip::new("video.mp4");
        assert_eq!(clip.timeline_offset, Duration::ZERO);
        assert!(clip.in_point.is_none());
        assert!(clip.out_point.is_none());
        assert!(clip.metadata.is_empty());
    }

    #[test]
    fn clip_new_should_default_transition_to_none() {
        let clip = Clip::new("video.mp4");
        assert!(clip.transition.is_none());
        assert_eq!(clip.transition_duration, Duration::ZERO);
    }

    #[test]
    fn clip_with_transition_should_set_fields() {
        use ff_filter::XfadeTransition;
        let clip = Clip::new("video.mp4")
            .with_transition(XfadeTransition::Fade, Duration::from_millis(500));
        assert_eq!(clip.transition, Some(XfadeTransition::Fade));
        assert_eq!(clip.transition_duration, Duration::from_millis(500));
    }

    #[test]
    fn clip_trim_should_set_in_out_points() {
        let clip = Clip::new("video.mp4").trim(Duration::from_secs(3), Duration::from_secs(9));
        assert_eq!(clip.in_point, Some(Duration::from_secs(3)));
        assert_eq!(clip.out_point, Some(Duration::from_secs(9)));
    }

    #[test]
    fn clip_duration_should_return_none_when_out_point_unset() {
        let clip = Clip::new("video.mp4");
        assert!(clip.duration().is_none());
    }

    #[test]
    fn clip_duration_should_return_difference_when_both_points_set() {
        let clip = Clip::new("video.mp4").trim(Duration::from_secs(2), Duration::from_secs(10));
        assert_eq!(clip.duration(), Some(Duration::from_secs(8)));
    }

    #[test]
    fn clip_new_should_default_volume_db_to_zero() {
        let clip = Clip::new("audio.wav");
        assert_eq!(clip.volume_db, 0.0);
    }

    #[test]
    fn clip_volume_should_set_volume_db() {
        let clip = Clip::new("audio.wav").volume(-6.0);
        assert_eq!(clip.volume_db, -6.0);
    }

    #[test]
    fn clip_volume_positive_should_set_volume_db() {
        let clip = Clip::new("audio.wav").volume(3.0);
        assert_eq!(clip.volume_db, 3.0);
    }
}
