//! HLS segmented output builder.
//!
//! This module exposes [`HlsOutput`], a consuming builder that configures and
//! writes an HLS segmented stream. Validation is deferred to [`HlsOutput::build`]
//! so setter calls are infallible.

use std::time::Duration;

use crate::error::StreamError;

/// Builds and writes an HLS segmented output.
///
/// `HlsOutput` follows the consuming-builder pattern: each setter takes `self`
/// and returns a new `Self`, and the final [`build`](Self::build) call validates
/// the configuration before returning a ready-to-write instance.
///
/// # Examples
///
/// ```ignore
/// use ff_stream::HlsOutput;
/// use std::time::Duration;
///
/// HlsOutput::new("/var/www/hls")
///     .input("source.mp4")
///     .segment_duration(Duration::from_secs(6))
///     .keyframe_interval(48)
///     .build()?
///     .write()?;
/// ```
pub struct HlsOutput {
    output_dir: String,
    input_path: Option<String>,
    segment_duration: Duration,
    keyframe_interval: u32,
}

impl HlsOutput {
    /// Create a new builder targeting `output_dir`.
    ///
    /// The directory does not need to exist at construction time; it will be
    /// created (if absent) by the `FFmpeg` HLS muxer when [`write`](Self::write)
    /// is called.
    ///
    /// Defaults: segment duration = 6 s, keyframe interval = 48 frames.
    #[must_use]
    pub fn new(output_dir: &str) -> Self {
        Self {
            output_dir: output_dir.to_owned(),
            input_path: None,
            segment_duration: Duration::from_secs(6),
            keyframe_interval: 48,
        }
    }

    /// Set the input media file path.
    ///
    /// This is required; [`build`](Self::build) will return
    /// [`StreamError::InvalidConfig`] if no input is supplied.
    #[must_use]
    pub fn input(mut self, path: &str) -> Self {
        self.input_path = Some(path.to_owned());
        self
    }

    /// Override the HLS segment duration (default: 6 s).
    ///
    /// Apple's HLS recommendation is 6 s for live streams and up to 10 s for
    /// VOD. Smaller values reduce latency but increase the number of segment
    /// files and playlist entries.
    #[must_use]
    pub fn segment_duration(mut self, d: Duration) -> Self {
        self.segment_duration = d;
        self
    }

    /// Override the keyframe interval in frames (default: 48).
    ///
    /// HLS requires segment boundaries to align with keyframes. Setting this to
    /// `fps × segment_duration` (e.g. 24 fps × 2 s = 48) ensures clean cuts
    /// without decoding artefacts at the start of each segment.
    #[must_use]
    pub fn keyframe_interval(mut self, frames: u32) -> Self {
        self.keyframe_interval = frames;
        self
    }

    /// Validate the configuration and return a ready-to-write `HlsOutput`.
    ///
    /// # Errors
    ///
    /// - [`StreamError::InvalidConfig`] when `output_dir` is empty.
    /// - [`StreamError::InvalidConfig`] when no input path has been set via
    ///   [`input`](Self::input).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_stream::HlsOutput;
    ///
    /// // Missing input → error
    /// assert!(HlsOutput::new("/tmp/hls").build().is_err());
    ///
    /// // Valid configuration → ok
    /// assert!(HlsOutput::new("/tmp/hls").input("src.mp4").build().is_ok());
    /// ```
    pub fn build(self) -> Result<Self, StreamError> {
        if self.output_dir.is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "output_dir must not be empty".into(),
            });
        }
        if self.input_path.is_none() {
            return Err(StreamError::InvalidConfig {
                reason: "input path is required".into(),
            });
        }
        log::info!(
            "hls output configured output_dir={} segment_duration={:.1}s keyframe_interval={}",
            self.output_dir,
            self.segment_duration.as_secs_f64(),
            self.keyframe_interval,
        );
        Ok(self)
    }

    /// Write HLS segments to the output directory.
    ///
    /// On success the output directory will contain a `playlist.m3u8` file and
    /// numbered segment files (`segment000.ts`, `segment001.ts`, …).
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::InvalidConfig`] with `"not yet implemented"` until
    /// `FFmpeg` HLS muxing integration is complete.
    pub fn write(self) -> Result<(), StreamError> {
        Err(StreamError::InvalidConfig {
            reason: "not yet implemented".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_should_store_output_dir() {
        let h = HlsOutput::new("/tmp/hls");
        assert_eq!(h.output_dir, "/tmp/hls");
    }

    #[test]
    fn input_should_store_input_path() {
        let h = HlsOutput::new("/tmp/hls").input("/src/video.mp4");
        assert_eq!(h.input_path.as_deref(), Some("/src/video.mp4"));
    }

    #[test]
    fn segment_duration_should_store_duration() {
        let d = Duration::from_secs(10);
        let h = HlsOutput::new("/tmp/hls").segment_duration(d);
        assert_eq!(h.segment_duration, d);
    }

    #[test]
    fn keyframe_interval_should_store_interval() {
        let h = HlsOutput::new("/tmp/hls").keyframe_interval(24);
        assert_eq!(h.keyframe_interval, 24);
    }

    #[test]
    fn build_without_input_should_return_invalid_config() {
        let result = HlsOutput::new("/tmp/hls").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_empty_output_dir_should_return_invalid_config() {
        let result = HlsOutput::new("").input("/src/video.mp4").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_valid_config_should_succeed() {
        let result = HlsOutput::new("/tmp/hls").input("/src/video.mp4").build();
        assert!(result.is_ok());
    }
}
