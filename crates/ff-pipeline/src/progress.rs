//! Pipeline progress tracking.
//!
//! This module provides [`Progress`], which is passed to the
//! [`ProgressCallback`] on every processed frame, and the
//! [`ProgressCallback`] type alias itself.

/// Progress information delivered to the caller on each processed frame.
///
/// An instance of this struct is passed by reference to the
/// [`ProgressCallback`] registered via
/// [`PipelineBuilder::on_progress`](crate::PipelineBuilder::on_progress).
/// The callback returns `true` to continue or `false` to cancel the pipeline.
#[derive(Debug, Clone)]
pub struct Progress {
    /// Number of frames processed so far.
    pub frames_processed: u64,

    /// Total number of frames in the source, or `None` if the container
    /// does not report a frame count.
    pub total_frames: Option<u64>,

    /// Wall-clock time elapsed since [`Pipeline::run`](crate::Pipeline::run) was called.
    pub elapsed: std::time::Duration,
}

impl Progress {
    /// Returns the completion percentage in the range `0.0..=100.0`, or `None`
    /// when [`total_frames`](Self::total_frames) is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_pipeline::Progress;
    /// use std::time::Duration;
    ///
    /// let p = Progress { frames_processed: 25, total_frames: Some(100), elapsed: Duration::ZERO };
    /// assert_eq!(p.percent(), Some(25.0));
    ///
    /// let p = Progress { frames_processed: 5, total_frames: None, elapsed: Duration::ZERO };
    /// assert_eq!(p.percent(), None);
    /// ```
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn percent(&self) -> Option<f64> {
        self.total_frames
            .map(|total| (self.frames_processed as f64 / total as f64) * 100.0)
    }
}

/// Callback invoked on every processed frame to report progress.
///
/// The closure receives a [`Progress`] reference and must return `true` to
/// continue processing or `false` to request cancellation.  When `false` is
/// returned, [`Pipeline::run`](crate::Pipeline::run) stops at the next frame
/// boundary and returns [`PipelineError::Cancelled`](crate::PipelineError::Cancelled).
pub type ProgressCallback = Box<dyn Fn(&Progress) -> bool + Send>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn percent_should_return_none_when_total_frames_unknown() {
        let p = Progress {
            frames_processed: 10,
            total_frames: None,
            elapsed: Duration::from_secs(1),
        };
        assert_eq!(p.percent(), None);
    }

    #[test]
    fn percent_should_return_correct_value_when_total_known() {
        let p = Progress {
            frames_processed: 50,
            total_frames: Some(200),
            elapsed: Duration::from_secs(1),
        };
        assert!((p.percent().unwrap() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percent_should_return_100_when_complete() {
        let p = Progress {
            frames_processed: 100,
            total_frames: Some(100),
            elapsed: Duration::from_secs(5),
        };
        assert!((p.percent().unwrap() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percent_should_return_0_when_no_frames_processed() {
        let p = Progress {
            frames_processed: 0,
            total_frames: Some(120),
            elapsed: Duration::ZERO,
        };
        assert_eq!(p.percent(), Some(0.0));
    }

    #[test]
    fn percent_should_exceed_100_when_processed_exceeds_total() {
        // percent() makes no claim about clamping — callers are responsible.
        let p = Progress {
            frames_processed: 110,
            total_frames: Some(100),
            elapsed: Duration::from_secs(4),
        };
        assert!(p.percent().unwrap() > 100.0);
    }

    #[test]
    fn callback_should_receive_progress_and_return_bool() {
        let continue_cb: ProgressCallback = Box::new(|_p| true);
        let cancel_cb: ProgressCallback = Box::new(|_p| false);

        let p = Progress {
            frames_processed: 1,
            total_frames: Some(10),
            elapsed: Duration::from_millis(33),
        };

        assert!(continue_cb(&p));
        assert!(!cancel_cb(&p));
    }
}
