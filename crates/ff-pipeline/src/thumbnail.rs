//! Thumbnail extraction pipeline.
//!
//! This module provides [`ThumbnailPipeline`], which extracts a [`VideoFrame`] at each
//! caller-specified timestamp from a media file. Seeking and decoding are delegated to
//! [`ff_decode::VideoDecoder`]; no `unsafe` code is required here.

use std::path::PathBuf;
use std::time::Duration;

use ff_decode::{SeekMode, VideoDecoder};
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
    output_dir: Option<PathBuf>,
    width: Option<u32>,
    quality: Option<u32>,
}

impl ThumbnailPipeline {
    /// Creates a new pipeline for the given file path.
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            timestamps: Vec::new(),
            output_dir: None,
            width: None,
            quality: None,
        }
    }

    /// Sets the timestamps (in seconds) at which to extract frames.
    #[must_use]
    pub fn timestamps(mut self, times: Vec<f64>) -> Self {
        self.timestamps = times;
        self
    }

    /// Set output directory for [`run_to_files()`](Self::run_to_files).
    #[must_use]
    pub fn output_dir(mut self, dir: impl AsRef<std::path::Path>) -> Self {
        self.output_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Limit thumbnail width; height is scaled proportionally.
    ///
    /// Only used by [`run_to_files()`](Self::run_to_files).
    #[must_use]
    pub fn width(mut self, w: u32) -> Self {
        self.width = Some(w);
        self
    }

    /// JPEG quality 0–100.
    ///
    /// Only used by [`run_to_files()`](Self::run_to_files).
    #[must_use]
    pub fn quality(mut self, q: u32) -> Self {
        self.quality = Some(q);
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
        decode_frames(&self.path, &mut self.timestamps)
    }

    /// Runs the pipeline, writes each frame as a JPEG to `output_dir`,
    /// and returns the written paths in timestamp order.
    ///
    /// File names: `thumb_0000.jpg`, `thumb_0001.jpg`, … (zero-padded index).
    /// When `.width()` is set, height is scaled proportionally.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::NoOutput`]  — `output_dir` not configured
    /// - [`PipelineError::Io`]        — directory creation failed
    /// - [`PipelineError::Decode`]    — seek/decode failed
    /// - [`PipelineError::Encode`]    — image write failed
    pub fn run_to_files(mut self) -> Result<Vec<PathBuf>, PipelineError> {
        let dir = self.output_dir.take().ok_or(PipelineError::NoOutput)?;

        if self.timestamps.is_empty() {
            return Ok(vec![]);
        }

        std::fs::create_dir_all(&dir)?;

        let frames = decode_frames(&self.path, &mut self.timestamps)?;

        let mut paths = Vec::with_capacity(frames.len());
        for (i, frame) in frames.into_iter().enumerate() {
            let fw = frame.width();
            let fh = frame.height();

            let (enc_w, enc_h) = match self.width {
                Some(w) if fw > w => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let enc_h =
                        ((f64::from(fh) * f64::from(w) / f64::from(fw)).round() as u32).max(1);
                    (w, enc_h)
                }
                _ => (fw, fh),
            };

            let out_path = dir.join(format!("thumb_{i:04}.jpg"));

            let mut builder = ff_encode::ImageEncoder::create(&out_path)
                .width(enc_w)
                .height(enc_h);
            if let Some(q) = self.quality {
                builder = builder.quality(q);
            }
            builder.build()?.encode(&frame)?;

            log::info!(
                "thumbnail written path={} width={enc_w} height={enc_h}",
                out_path.display()
            );
            paths.push(out_path);
        }

        Ok(paths)
    }
}

fn decode_frames(path: &str, timestamps: &mut [f64]) -> Result<Vec<VideoFrame>, PipelineError> {
    timestamps.sort_by(f64::total_cmp);

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;

        log::info!(
            "thumbnail pipeline starting parallel extraction path={} count={}",
            path,
            timestamps.len()
        );

        // par_iter on a slice is an IndexedParallelIterator: collect() preserves
        // the original index order, so the output is already timestamp-sorted.
        timestamps
            .par_iter()
            .map(|ts| {
                let mut decoder = VideoDecoder::open(path).build()?;
                decoder.seek(Duration::from_secs_f64(*ts), SeekMode::Keyframe)?;
                let frame = decoder
                    .decode_one()?
                    .ok_or(PipelineError::FrameNotAvailable)?;
                Ok(frame)
            })
            .collect()
    }

    #[cfg(not(feature = "parallel"))]
    {
        let mut decoder = VideoDecoder::open(path).build()?;
        log::info!("thumbnail pipeline opened file path={path}");

        let mut frames = Vec::with_capacity(timestamps.len());
        for ts in timestamps.iter() {
            decoder.seek(Duration::from_secs_f64(*ts), SeekMode::Keyframe)?;
            let frame = decoder
                .decode_one()?
                .ok_or(PipelineError::FrameNotAvailable)?;
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

    #[test]
    fn output_dir_should_store_path() {
        let pipeline = ThumbnailPipeline::new("video.mp4").output_dir("/tmp/thumbs");
        assert_eq!(pipeline.output_dir, Some(PathBuf::from("/tmp/thumbs")));
    }

    #[test]
    fn width_setter_should_store_value() {
        let pipeline = ThumbnailPipeline::new("video.mp4").width(320);
        assert_eq!(pipeline.width, Some(320));
    }

    #[test]
    fn quality_setter_should_store_value() {
        let pipeline = ThumbnailPipeline::new("video.mp4").quality(85);
        assert_eq!(pipeline.quality, Some(85));
    }

    #[test]
    fn run_to_files_without_output_dir_should_return_no_output_error() {
        let result = ThumbnailPipeline::new("nonexistent.mp4")
            .timestamps(vec![0.0])
            .run_to_files();
        assert!(matches!(result, Err(PipelineError::NoOutput)));
    }

    #[test]
    fn run_to_files_with_empty_timestamps_and_no_dir_should_return_no_output_error() {
        let result = ThumbnailPipeline::new("nonexistent.mp4").run_to_files();
        assert!(matches!(result, Err(PipelineError::NoOutput)));
    }
}
