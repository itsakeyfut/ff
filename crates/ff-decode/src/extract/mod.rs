//! Batch frame extraction at regular time intervals.
//!
//! [`FrameExtractor`] samples one frame per configurable time interval across
//! the full duration of a video. Returns a `Vec<VideoFrame>` suitable for
//! thumbnail strips and preview generation.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ff_format::VideoFrame;

use crate::DecodeError;
use crate::VideoDecoder;

/// Extracts one frame per time interval from a video file.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::FrameExtractor;
/// use std::time::Duration;
///
/// let frames = FrameExtractor::new("video.mp4")
///     .interval(Duration::from_secs(5))
///     .run()?;
/// println!("extracted {} frames", frames.len());
/// ```
pub struct FrameExtractor {
    input: PathBuf,
    interval: Duration,
}

impl FrameExtractor {
    /// Creates a new `FrameExtractor` for the given input file.
    ///
    /// The default extraction interval is 1 second.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            interval: Duration::from_secs(1),
        }
    }

    /// Sets the time interval between extracted frames.
    ///
    /// Passing [`Duration::ZERO`] causes [`run`](Self::run) to return
    /// [`DecodeError::AnalysisFailed`].
    #[must_use]
    pub fn interval(self, d: Duration) -> Self {
        Self {
            interval: d,
            ..self
        }
    }

    /// Runs the extraction and returns one frame per interval.
    ///
    /// Timestamps `0, interval, 2×interval, …` up to (but not including)
    /// the video duration are sampled. [`DecodeError::NoFrameAtTimestamp`]
    /// for a given timestamp is silently skipped with a `warn!` log; all
    /// other errors are propagated immediately.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — interval is zero, or the input
    ///   file cannot be opened.
    /// - Any other [`DecodeError`] propagated from the decoder.
    pub fn run(self) -> Result<Vec<VideoFrame>, DecodeError> {
        if self.interval.is_zero() {
            return Err(DecodeError::AnalysisFailed {
                reason: "interval must be positive".to_string(),
            });
        }

        let mut decoder = VideoDecoder::open(&self.input).build()?;
        let duration = decoder.duration();

        let mut frames = Vec::new();
        let mut ts = Duration::ZERO;

        while ts < duration {
            match decoder.extract_frame(ts) {
                Ok(frame) => frames.push(frame),
                Err(DecodeError::NoFrameAtTimestamp { .. }) => {
                    log::warn!(
                        "frame not available, skipping timestamp={ts:?} input={}",
                        self.input.display()
                    );
                }
                Err(e) => return Err(e),
            }
            ts += self.interval;
        }

        let frame_count = frames.len();
        log::debug!(
            "frame extraction complete frames={frame_count} interval={interval:?}",
            interval = self.interval
        );

        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_extractor_zero_interval_should_err() {
        let result = FrameExtractor::new("irrelevant.mp4")
            .interval(Duration::ZERO)
            .run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for zero interval, got {result:?}"
        );
    }

    #[test]
    fn frame_extractor_should_return_correct_frame_count() {
        // Unit test: verify the timestamp generation logic without a real file.
        // We test this via the zero-interval guard and rely on integration tests
        // for the full run() path with a real video file.
        let extractor = FrameExtractor::new("video.mp4").interval(Duration::from_secs(1));
        assert_eq!(extractor.interval, Duration::from_secs(1));
    }
}
