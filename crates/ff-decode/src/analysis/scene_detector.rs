//! Scene-change detection.

#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::DecodeError;

/// Detects scene changes in a video file and returns their timestamps.
///
/// Uses `FFmpeg`'s `select=gt(scene\,threshold)` filter to identify frames
/// where the scene changes.  The `threshold` controls detection sensitivity:
/// lower values detect more cuts (including subtle ones); higher values detect
/// only hard cuts.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::SceneDetector;
///
/// let cuts = SceneDetector::new("video.mp4")
///     .threshold(0.3)
///     .run()?;
///
/// for ts in &cuts {
///     println!("Scene change at {:?}", ts);
/// }
/// ```
pub struct SceneDetector {
    input: PathBuf,
    threshold: f64,
}

impl SceneDetector {
    /// Creates a new detector for the given video file.
    ///
    /// The default detection threshold is `0.4`.  Call
    /// [`threshold`](Self::threshold) to override it.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            threshold: 0.4,
        }
    }

    /// Sets the scene-change detection threshold.
    ///
    /// Must be in the range `[0.0, 1.0]`.  Lower values make the detector more
    /// sensitive (more cuts reported); higher values require a larger visual
    /// difference.  Passing a value outside this range causes
    /// [`run`](Self::run) to return [`DecodeError::AnalysisFailed`].
    ///
    /// Default: `0.4`.
    #[must_use]
    pub fn threshold(self, t: f64) -> Self {
        Self {
            threshold: t,
            ..self
        }
    }

    /// Runs scene-change detection and returns one [`Duration`] per detected cut.
    ///
    /// Timestamps are sorted in ascending order and represent the PTS of the
    /// first frame of each new scene.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — threshold outside `[0.0, 1.0]`,
    ///   input file not found, or an internal filter-graph error.
    pub fn run(self) -> Result<Vec<Duration>, DecodeError> {
        if !(0.0..=1.0).contains(&self.threshold) {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("threshold must be in [0.0, 1.0], got {}", self.threshold),
            });
        }
        if !self.input.exists() {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("file not found: {}", self.input.display()),
            });
        }
        // SAFETY: detect_scenes_unsafe manages all raw pointer lifetimes according
        // to the avfilter ownership rules documented in analysis_inner. The path is
        // valid for the duration of the call.
        unsafe { super::analysis_inner::detect_scenes_unsafe(&self.input, self.threshold) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_detector_invalid_threshold_below_zero_should_return_analysis_failed() {
        let result = SceneDetector::new("irrelevant.mp4").threshold(-0.1).run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for threshold=-0.1, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_invalid_threshold_above_one_should_return_analysis_failed() {
        let result = SceneDetector::new("irrelevant.mp4").threshold(1.1).run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for threshold=1.1, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_missing_file_should_return_analysis_failed() {
        let result = SceneDetector::new("does_not_exist_99999.mp4").run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing file, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_boundary_thresholds_should_be_valid() {
        // 0.0 and 1.0 are valid thresholds (boundary-inclusive check).
        // They return errors only for missing file, not for bad threshold.
        let r0 = SceneDetector::new("irrelevant.mp4").threshold(0.0).run();
        let r1 = SceneDetector::new("irrelevant.mp4").threshold(1.0).run();
        // Both should fail with AnalysisFailed (file not found), NOT threshold error.
        assert!(
            matches!(r0, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed (file), got {r0:?}"
        );
        assert!(
            matches!(r1, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed (file), got {r1:?}"
        );
    }
}
