//! Thumbnail extraction pipeline.
//!
//! This module provides [`ThumbnailPipeline`], which extracts a [`VideoFrame`] at each
//! caller-specified timestamp from a media file. Seeking and decoding are delegated to
//! [`ff_decode::VideoDecoder`]; no `unsafe` code is required here.

use std::time::Duration;

use ff_decode::{DecodeError, SeekMode, VideoDecoder};
use ff_format::VideoFrame;

use crate::PipelineError;

/// Extracts still frames from a video file at requested timestamps.
///
/// # Construction
///
/// Use the consuming builder pattern:
///
/// ```ignore
/// use ff_pipeline::ThumbnailPipeline;
///
/// let frames = ThumbnailPipeline::new("video.mp4")
///     .timestamps(vec![0.0, 5.0, 10.0])
///     .run()?;
/// ```
pub struct ThumbnailPipeline {
    path: String,
    timestamps: Vec<f64>,
}

impl ThumbnailPipeline {
    /// Creates a new pipeline for the given file path.
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            timestamps: Vec::new(),
        }
    }

    /// Sets the timestamps (in seconds) at which to extract frames.
    #[must_use]
    pub fn timestamps(mut self, times: Vec<f64>) -> Self {
        self.timestamps = times;
        self
    }

    /// Runs the pipeline and returns one [`VideoFrame`] per requested timestamp.
    ///
    /// Timestamps are processed in ascending order. If `timestamps` is empty,
    /// the file is never opened and `Ok(vec![])` is returned immediately.
    ///
    /// When the `parallel` feature is enabled, each timestamp is decoded in its
    /// own thread via `rayon`. Each thread opens an independent [`VideoDecoder`];
    /// no decoder context is shared. The output order matches the ascending
    /// timestamp order regardless of which thread finishes first.
    ///
    /// # Errors
    ///
    /// Propagates [`PipelineError::Decode`] for any decoding or seek failure.
    pub fn run(mut self) -> Result<Vec<VideoFrame>, PipelineError> {
        if self.timestamps.is_empty() {
            return Ok(vec![]);
        }

        self.timestamps.sort_by(f64::total_cmp);

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;

            log::info!(
                "thumbnail pipeline starting parallel extraction path={} count={}",
                self.path,
                self.timestamps.len()
            );

            // par_iter on a slice is an IndexedParallelIterator: collect() preserves
            // the original index order, so the output is already timestamp-sorted.
            self.timestamps
                .par_iter()
                .map(|ts| {
                    let mut decoder = VideoDecoder::open(&self.path).build()?;
                    decoder.seek(Duration::from_secs_f64(*ts), SeekMode::Keyframe)?;
                    let frame = decoder.decode_one()?.ok_or(DecodeError::EndOfStream)?;
                    Ok(frame)
                })
                .collect()
        }

        #[cfg(not(feature = "parallel"))]
        {
            let mut decoder = VideoDecoder::open(&self.path).build()?;
            log::info!("thumbnail pipeline opened file path={}", self.path);

            let mut frames = Vec::with_capacity(self.timestamps.len());
            for ts in &self.timestamps {
                decoder.seek(Duration::from_secs_f64(*ts), SeekMode::Keyframe)?;
                let frame = decoder.decode_one()?.ok_or(DecodeError::EndOfStream)?;
                frames.push(frame);
            }

            Ok(frames)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_should_store_path() {
        let pipeline = ThumbnailPipeline::new("video.mp4");
        assert_eq!(pipeline.path, "video.mp4");
    }

    #[test]
    fn timestamps_should_store_timestamps() {
        let pipeline = ThumbnailPipeline::new("video.mp4").timestamps(vec![1.0, 2.0, 3.0]);
        assert_eq!(pipeline.timestamps, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn run_with_no_timestamps_should_return_empty_vec() {
        let result = ThumbnailPipeline::new("nonexistent.mp4").run();
        assert!(matches!(result, Ok(ref v) if v.is_empty()));
    }

    #[test]
    fn timestamps_should_sort_ascending_before_run() {
        let mut ts = [3.0_f64, 1.0, 2.0];
        ts.sort_by(f64::total_cmp);
        assert_eq!(ts, [1.0, 2.0, 3.0]);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn timestamps_nan_should_sort_after_finite_values() {
        let mut ts = [2.0_f64, f64::NAN, 1.0];
        ts.sort_by(f64::total_cmp);
        assert_eq!(ts[0], 1.0);
        assert_eq!(ts[1], 2.0);
        assert!(ts[2].is_nan());
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn parallel_run_with_no_timestamps_should_return_empty_vec() {
        let result = ThumbnailPipeline::new("nonexistent.mp4").run();
        assert!(matches!(result, Ok(ref v) if v.is_empty()));
    }
}
