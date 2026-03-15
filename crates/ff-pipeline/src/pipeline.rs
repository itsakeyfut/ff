//! Pipeline builder and runner.
//!
//! This module provides:
//!
//! - [`EncoderConfig`] — codec and quality settings for the output file
//! - [`PipelineBuilder`] — consuming builder that validates configuration
//! - [`Pipeline`] — the configured pipeline, executed by calling [`run`](Pipeline::run)

use std::time::Instant;

use ff_decode::{AudioDecoder, VideoDecoder};
use ff_encode::{AudioCodec, BitrateMode, HardwareEncoder, VideoCodec, VideoEncoder};
use ff_filter::{FilterGraph, HwAccel};
use ff_format::Timestamp;

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
        // Invariants guaranteed by build(): inputs is non-empty, output is Some.
        let first_input = &self.inputs[0];
        let (out_path, enc_config) = self.output.ok_or(PipelineError::NoOutput)?;
        let mut filter = self.filter;
        let num_inputs = self.inputs.len();

        // Open the first input to determine output dimensions.
        let first_vdec = VideoDecoder::open(first_input).build()?;
        let (out_width, out_height) = enc_config
            .resolution
            .unwrap_or_else(|| (first_vdec.width(), first_vdec.height()));
        let fps = enc_config
            .framerate
            .unwrap_or_else(|| first_vdec.frame_rate());

        // total_frames is only meaningful for single-input pipelines.
        let total_frames = if num_inputs == 1 {
            first_vdec.stream_info().frame_count()
        } else {
            None
        };

        log::info!(
            "pipeline starting inputs={num_inputs} output={out_path} \
             width={out_width} height={out_height} fps={fps} total_frames={total_frames:?}"
        );

        // Probe audio from the first input to configure the encoder audio track.
        let audio_config: Option<(u32, u32)> = match AudioDecoder::open(first_input).build() {
            Ok(adec) => Some((
                adec.stream_info().sample_rate(),
                adec.stream_info().channels(),
            )),
            Err(e) => {
                log::warn!(
                    "audio stream unavailable, encoding video only \
                     path={first_input} reason={e}"
                );
                None
            }
        };

        // Build encoder, adding audio track only when the first input has audio.
        let hw = hwaccel_to_hardware_encoder(enc_config.hardware);
        let mut enc_builder = VideoEncoder::create(&out_path)
            .video(out_width, out_height, fps)
            .video_codec(enc_config.video_codec)
            .bitrate_mode(enc_config.bitrate_mode)
            .hardware_encoder(hw);

        if let Some((sample_rate, channels)) = audio_config {
            enc_builder = enc_builder
                .audio(sample_rate, channels)
                .audio_codec(enc_config.audio_codec);
        }

        let mut encoder = enc_builder.build()?;
        log::debug!(
            "encoder opened codec={} hardware={hw:?}",
            encoder.actual_video_codec()
        );

        let start = Instant::now();
        let mut frames_processed: u64 = 0;
        let mut cancelled = false;
        let frame_period_secs = if fps > 0.0 { 1.0 / fps } else { 0.0 };

        // PTS offset in seconds: accumulates the duration of all processed inputs.
        let mut pts_offset_secs: f64 = 0.0;

        // Reuse the already-opened first decoder; open fresh decoders for subsequent inputs.
        let mut maybe_first_vdec = Some(first_vdec);

        'inputs: for input in &self.inputs {
            let mut vdec = if let Some(vd) = maybe_first_vdec.take() {
                vd
            } else {
                VideoDecoder::open(input).build()?
            };

            let mut last_frame_end_secs: f64 = pts_offset_secs;

            loop {
                let Some(mut raw_frame) = vdec.decode_one()? else {
                    break;
                };

                // Rebase timestamp so this clip follows the previous one.
                let ts = raw_frame.timestamp();
                let new_pts_secs = pts_offset_secs + ts.as_secs_f64();
                last_frame_end_secs = new_pts_secs + frame_period_secs;
                raw_frame.set_timestamp(Timestamp::from_secs_f64(new_pts_secs, ts.time_base()));

                let frame = if let Some(ref mut fg) = filter {
                    fg.push_video(0, &raw_frame)?;
                    match fg.pull_video()? {
                        Some(f) => f,
                        None => continue, // filter is buffering; feed more input
                    }
                } else {
                    raw_frame
                };

                encoder.push_video(&frame)?;
                frames_processed += 1;

                if let Some(ref cb) = self.callback {
                    let progress = Progress {
                        frames_processed,
                        total_frames,
                        elapsed: start.elapsed(),
                    };
                    if !cb(&progress) {
                        log::info!(
                            "pipeline cancelled by callback \
                             frames_processed={frames_processed}"
                        );
                        cancelled = true;
                        break 'inputs;
                    }
                }
            }

            // Advance PTS offset to the end of the last frame of this input.
            pts_offset_secs = last_frame_end_secs;
            log::debug!("input complete path={input} pts_offset_secs={pts_offset_secs:.3}");
        }

        // Audio pass: process each input sequentially, rebasing timestamps.
        if !cancelled && audio_config.is_some() {
            let mut audio_offset_secs: f64 = 0.0;
            for input in &self.inputs {
                match AudioDecoder::open(input).build() {
                    Ok(mut adec) => {
                        let mut last_audio_end_secs: f64 = audio_offset_secs;
                        while let Some(mut aframe) = adec.decode_one()? {
                            let ts = aframe.timestamp();
                            let new_pts_secs = audio_offset_secs + ts.as_secs_f64();
                            #[allow(clippy::cast_precision_loss)]
                            let frame_dur_secs = if aframe.sample_rate() > 0 {
                                aframe.samples() as f64 / f64::from(aframe.sample_rate())
                            } else {
                                0.0
                            };
                            last_audio_end_secs = new_pts_secs + frame_dur_secs;
                            aframe.set_timestamp(Timestamp::from_secs_f64(
                                new_pts_secs,
                                ts.time_base(),
                            ));
                            encoder.push_audio(&aframe)?;
                        }
                        audio_offset_secs = last_audio_end_secs;
                    }
                    Err(e) => {
                        log::warn!("audio stream unavailable path={input} reason={e}");
                    }
                }
            }
        }

        // Flush encoder and write trailer regardless of cancellation.
        encoder.finish()?;

        let elapsed = start.elapsed();
        log::info!("pipeline finished frames_processed={frames_processed} elapsed={elapsed:?}");

        if cancelled {
            return Err(PipelineError::Cancelled);
        }
        Ok(())
    }
}

/// Converts a filter-graph hardware backend into an encoder hardware backend.
///
/// `HwAccel` (ff-filter) and `HardwareEncoder` (ff-encode) are separate types
/// to avoid a cross-crate dependency.  This function maps between them.
fn hwaccel_to_hardware_encoder(hw: Option<HwAccel>) -> HardwareEncoder {
    match hw {
        None => HardwareEncoder::None,
        Some(HwAccel::Cuda) => HardwareEncoder::Nvenc,
        Some(HwAccel::VideoToolbox) => HardwareEncoder::VideoToolbox,
        Some(HwAccel::Vaapi) => HardwareEncoder::Vaapi,
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

    #[test]
    fn input_should_accept_multiple_paths() {
        // Three successive .input() calls must all succeed and build must not
        // return NoInput.
        let result = Pipeline::builder()
            .input("/tmp/a.mp4")
            .input("/tmp/b.mp4")
            .input("/tmp/c.mp4")
            .output("/tmp/out.mp4", dummy_config())
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn on_progress_should_not_prevent_successful_build() {
        let result = Pipeline::builder()
            .input("/tmp/in.mp4")
            .output("/tmp/out.mp4", dummy_config())
            .on_progress(|_p| true)
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn default_should_produce_empty_builder() {
        // PipelineBuilder::default() must behave identically to ::new():
        // an empty builder has no inputs and therefore returns NoInput.
        let result = PipelineBuilder::default()
            .output("/tmp/out.mp4", dummy_config())
            .build();
        assert!(matches!(result, Err(PipelineError::NoInput)));
    }

    #[test]
    fn build_should_require_both_input_and_output() {
        // Neither input alone nor output alone is sufficient.
        assert!(matches!(
            Pipeline::builder().build(),
            Err(PipelineError::NoInput)
        ));
        assert!(matches!(
            Pipeline::builder().input("/tmp/in.mp4").build(),
            Err(PipelineError::NoOutput)
        ));
    }
}
