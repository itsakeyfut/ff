//! DASH segmented output builder.
//!
//! This module exposes [`DashOutput`], a consuming builder that configures and
//! writes a DASH segmented stream. Validation is deferred to
//! [`DashOutput::build`] so setter calls are infallible.

use std::time::Duration;

use crate::error::StreamError;

/// Builds and writes a DASH segmented output.
///
/// `DashOutput` follows the consuming-builder pattern: each setter takes `self`
/// and returns a new `Self`, and the final [`build`](Self::build) call validates
/// the configuration before returning a ready-to-write instance.
///
/// # Examples
///
/// ```ignore
/// use ff_stream::DashOutput;
/// use std::time::Duration;
///
/// DashOutput::new("/var/www/dash")
///     .input("source.mp4")
///     .segment_duration(Duration::from_secs(4))
///     .build()?
///     .write()?;
/// ```
pub struct DashOutput {
    output_dir: String,
    input_path: Option<String>,
    segment_duration: Duration,
}

impl DashOutput {
    /// Create a new builder targeting `output_dir`.
    ///
    /// The directory does not need to exist at construction time; it will be
    /// created (if absent) by the `FFmpeg` DASH muxer when [`write`](Self::write)
    /// is called.
    ///
    /// Default: segment duration = 4 s.
    #[must_use]
    pub fn new(output_dir: &str) -> Self {
        Self {
            output_dir: output_dir.to_owned(),
            input_path: None,
            segment_duration: Duration::from_secs(4),
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

    /// Override the DASH segment duration (default: 4 s).
    ///
    /// MPEG-DASH recommends 2–10 s segments; 4 s is a common default that
    /// balances latency against the overhead of many small files.
    #[must_use]
    pub fn segment_duration(mut self, d: Duration) -> Self {
        self.segment_duration = d;
        self
    }

    /// Validate the configuration and return a ready-to-write `DashOutput`.
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
    /// use ff_stream::DashOutput;
    ///
    /// // Missing input → error
    /// assert!(DashOutput::new("/tmp/dash").build().is_err());
    ///
    /// // Valid configuration → ok
    /// assert!(DashOutput::new("/tmp/dash").input("src.mp4").build().is_ok());
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
            "dash output configured output_dir={} segment_duration={:.1}s",
            self.output_dir,
            self.segment_duration.as_secs_f64(),
        );
        Ok(self)
    }

    /// Write DASH segments to the output directory.
    ///
    /// On success the output directory will contain a `manifest.mpd` file and
    /// the corresponding initialization and media segments.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::InvalidConfig`] when the builder is not fully
    /// configured, or [`StreamError::Ffmpeg`] when an `FFmpeg` operation fails.
    pub fn write(self) -> Result<(), StreamError> {
        let input_path = self.input_path.ok_or_else(|| StreamError::InvalidConfig {
            reason: "input path missing after build (internal error)".into(),
        })?;
        let seg_secs = self.segment_duration.as_secs_f64();
        log::info!(
            "dash write starting input={input_path} output_dir={} segment_duration={seg_secs:.1}s",
            self.output_dir
        );
        crate::dash_inner::write_dash(&input_path, &self.output_dir, seg_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_should_store_output_dir() {
        let d = DashOutput::new("/tmp/dash");
        assert_eq!(d.output_dir, "/tmp/dash");
    }

    #[test]
    fn build_without_input_should_return_invalid_config() {
        let result = DashOutput::new("/tmp/dash").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_valid_config_should_succeed() {
        let result = DashOutput::new("/tmp/dash").input("/src/video.mp4").build();
        assert!(result.is_ok());
    }

    #[test]
    fn write_without_build_should_return_invalid_config() {
        let result = DashOutput::new("/tmp/dash").write();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }
}
