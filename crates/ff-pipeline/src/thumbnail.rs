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
    /// # Errors
    ///
    /// Propagates [`PipelineError::Decode`] for any decoding or seek failure.
    pub fn run(mut self) -> Result<Vec<VideoFrame>, PipelineError> {
        if self.timestamps.is_empty() {
            return Ok(vec![]);
        }

        self.timestamps.sort_by(f64::total_cmp);

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
}
