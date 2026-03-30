//! Timeline clip data type.
//!
//! This module provides [`Clip`], a plain Rust value type representing a single
//! media clip on a timeline. `Clip` holds no `FFmpeg` context; it is interpreted
//! by `Timeline::render()` at call time to build filter graphs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
}
