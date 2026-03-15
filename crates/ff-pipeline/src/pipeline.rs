//! Pipeline builder and runner.
//!
//! This module provides:
//!
//! - [`EncoderConfig`] — codec and quality settings for the output file
//! - [`PipelineBuilder`] — consuming builder that validates configuration
//! - [`Pipeline`] — the configured pipeline, executed by calling [`run`](Pipeline::run)

use ff_encode::{AudioCodec, BitrateMode, VideoCodec};
use ff_filter::{FilterGraph, HwAccel};

use crate::error::PipelineError;
use crate::progress::{Progress, ProgressCallback};

/// Codec and quality configuration for the pipeline output.
///
/// Passed to [`PipelineBuilder::output`] alongside the output path.
pub struct EncoderConfig {
    /// Video codec to use for the output stream.
    pub video_codec: VideoCodec,

    /// Audio codec to use for the output stream.
    pub audio_codec: AudioCodec,

    /// Bitrate control mode (CBR, VBR, or CRF).
    pub bitrate_mode: BitrateMode,

    /// Output resolution as `(width, height)` in pixels.
    ///
    /// `None` preserves the source resolution.
    pub resolution: Option<(u32, u32)>,

    /// Output frame rate in frames per second.
    ///
    /// `None` preserves the source frame rate.
    pub framerate: Option<f64>,

    /// Hardware acceleration device to use during encoding.
    ///
    /// `None` uses software (CPU) encoding.
    pub hardware: Option<HwAccel>,
}

/// A configured, ready-to-run transcode pipeline.
///
/// Construct via [`Pipeline::builder`] → [`PipelineBuilder::build`].
/// Execute by calling [`run`](Self::run).
pub struct Pipeline {
    inputs: Vec<String>,
    filter: Option<FilterGraph>,
    output: Option<(String, EncoderConfig)>,
    #[allow(dead_code)]
    callback: Option<ProgressCallback>,
}

impl Pipeline {
    /// Returns a new [`PipelineBuilder`].
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_pipeline::{Pipeline, EncoderConfig};
    /// use ff_encode::{VideoCodec, AudioCodec, BitrateMode};
    ///
    /// let pipeline = Pipeline::builder()
    ///     .input("input.mp4")
    ///     .output("output.mp4", config)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn builder() -> PipelineBuilder {
        PipelineBuilder::new()
    }

    /// Runs the pipeline to completion.
    ///
    /// Executes the decode → (optional) filter → encode loop, calling the
    /// progress callback after each frame.  Returns
    /// [`PipelineError::Cancelled`] if the callback returns `false`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError`] on decode, filter, encode, or cancellation failures.
    pub fn run(self) -> Result<(), PipelineError> {
        log::debug!(
            "pipeline run called inputs={} has_filter={} has_output={}",
            self.inputs.len(),
            self.filter.is_some(),
            self.output.is_some(),
        );
        // TODO(#55): implement decode → filter → encode loop
        Err(PipelineError::Cancelled)
    }
}

/// Consuming builder for [`Pipeline`].
///
/// Validation is performed only in [`build`](Self::build), not in setters.
/// All setter methods take `self` by value and return `Self` for chaining.
pub struct PipelineBuilder {
    inputs: Vec<String>,
    filter: Option<FilterGraph>,
    output: Option<(String, EncoderConfig)>,
    callback: Option<ProgressCallback>,
}

impl PipelineBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inputs: Vec::new(),
            filter: None,
            output: None,
            callback: None,
        }
    }

    /// Adds an input file path.
    ///
    /// Multiple calls append to the input list; clips are concatenated in order.
    #[must_use]
    pub fn input(mut self, path: &str) -> Self {
        self.inputs.push(path.to_owned());
        self
    }

    /// Sets the filter graph to apply between decode and encode.
    ///
    /// If not called, decoded frames are passed directly to the encoder.
    #[must_use]
    pub fn filter(mut self, graph: FilterGraph) -> Self {
        self.filter = Some(graph);
        self
    }

    /// Sets the output file path and encoder configuration.
    #[must_use]
    pub fn output(mut self, path: &str, config: EncoderConfig) -> Self {
        self.output = Some((path.to_owned(), config));
        self
    }

    /// Registers a progress callback.
    ///
    /// The closure is called after each frame is encoded.  Returning `false`
    /// cancels the pipeline and causes [`Pipeline::run`] to return
    /// [`PipelineError::Cancelled`].
    #[must_use]
    pub fn on_progress(mut self, cb: impl Fn(&Progress) -> bool + Send + 'static) -> Self {
        self.callback = Some(Box::new(cb));
        self
    }

    /// Validates the configuration and returns a [`Pipeline`].
    ///
    /// # Errors
    ///
    /// - [`PipelineError::NoInput`] — no input was added via [`input`](Self::input)
    /// - [`PipelineError::NoOutput`] — [`output`](Self::output) was not called
    pub fn build(self) -> Result<Pipeline, PipelineError> {
        if self.inputs.is_empty() {
            return Err(PipelineError::NoInput);
        }
        if self.output.is_none() {
            return Err(PipelineError::NoOutput);
        }
        Ok(Pipeline {
            inputs: self.inputs,
            filter: self.filter,
            output: self.output,
            callback: self.callback,
        })
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff_encode::{AudioCodec, BitrateMode, VideoCodec};

    fn dummy_config() -> EncoderConfig {
        EncoderConfig {
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
            bitrate_mode: BitrateMode::Cbr(4_000_000),
            resolution: None,
            framerate: None,
            hardware: None,
        }
    }

    #[test]
    fn build_should_return_error_when_no_input() {
        let result = Pipeline::builder()
            .output("/tmp/out.mp4", dummy_config())
            .build();
        assert!(matches!(result, Err(PipelineError::NoInput)));
    }

    #[test]
    fn build_should_return_error_when_no_output() {
        let result = Pipeline::builder().input("/tmp/in.mp4").build();
        assert!(matches!(result, Err(PipelineError::NoOutput)));
    }

    #[test]
    fn build_should_succeed_with_valid_input_and_output() {
        let pipeline = Pipeline::builder()
            .input("/tmp/in.mp4")
            .output("/tmp/out.mp4", dummy_config())
            .build();
        assert!(pipeline.is_ok());
    }
}
