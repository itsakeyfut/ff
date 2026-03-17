//! Filter graph public API: [`FilterGraph`] and [`FilterGraphBuilder`].

use std::time::Duration;

use ff_format::{AudioFrame, VideoFrame};

use crate::error::FilterError;
use crate::filter_inner::FilterGraphInner;

// ── Supporting enums ──────────────────────────────────────────────────────────

/// Tone-mapping algorithm for HDR-to-SDR conversion.
///
/// Used with [`FilterGraphBuilder::tone_map`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneMap {
    /// Hable (Uncharted 2) filmic tone mapping.
    Hable,
    /// Reinhard tone mapping.
    Reinhard,
    /// Mobius tone mapping.
    Mobius,
}

impl ToneMap {
    /// Returns the libavfilter `tonemap` algorithm name for this variant.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Hable => "hable",
            Self::Reinhard => "reinhard",
            Self::Mobius => "mobius",
        }
    }
}

/// Hardware acceleration backend for filter graph operations.
///
/// When set on the builder, upload/download filters are inserted automatically
/// around the filter chain. This is independent of `ff_decode::HardwareAccel`
/// and is defined here to avoid a hard dependency on `ff-decode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwAccel {
    /// NVIDIA CUDA.
    Cuda,
    /// Apple `VideoToolbox`.
    VideoToolbox,
    /// VA-API (Video Acceleration API, Linux).
    Vaapi,
}

// ── FilterStep ────────────────────────────────────────────────────────────────

/// A single step in a filter chain, constructed by the builder methods.
///
/// This is an internal representation; users interact with it only via the
/// [`FilterGraphBuilder`] API.
#[derive(Debug, Clone)]
pub(crate) enum FilterStep {
    /// Trim: keep only frames in `[start, end)` seconds.
    Trim { start: f64, end: f64 },
    /// Scale to a new resolution.
    Scale { width: u32, height: u32 },
    /// Crop a rectangular region.
    Crop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// Overlay a second stream at position `(x, y)`.
    Overlay { x: i32, y: i32 },
    /// Fade-in from black over `duration`.
    FadeIn(Duration),
    /// Fade-out to black over `duration`.
    FadeOut(Duration),
    /// Rotate clockwise by `degrees`.
    Rotate(f64),
    /// HDR-to-SDR tone mapping.
    ToneMap(ToneMap),
    /// Adjust audio volume (in dB; negative = quieter).
    Volume(f64),
    /// Mix `n` audio inputs together.
    Amix(usize),
    /// Parametric equalizer band.
    Equalizer { band_hz: f64, gain_db: f64 },
}

impl FilterStep {
    /// Returns the libavfilter filter name for this step.
    pub(crate) fn filter_name(&self) -> &'static str {
        match self {
            Self::Trim { .. } => "trim",
            Self::Scale { .. } => "scale",
            Self::Crop { .. } => "crop",
            Self::Overlay { .. } => "overlay",
            Self::FadeIn(_) | Self::FadeOut(_) => "fade",
            Self::Rotate(_) => "rotate",
            Self::ToneMap(_) => "tonemap",
            Self::Volume(_) => "volume",
            Self::Amix(_) => "amix",
            Self::Equalizer { .. } => "equalizer",
        }
    }

    /// Returns the `args` string passed to `avfilter_graph_create_filter`.
    pub(crate) fn args(&self) -> String {
        match self {
            Self::Trim { start, end } => format!("start={start}:end={end}"),
            Self::Scale { width, height } => format!("w={width}:h={height}"),
            Self::Crop {
                x,
                y,
                width,
                height,
            } => {
                format!("x={x}:y={y}:w={width}:h={height}")
            }
            Self::Overlay { x, y } => format!("x={x}:y={y}"),
            Self::FadeIn(d) => format!("type=in:duration={}", d.as_secs_f64()),
            Self::FadeOut(d) => format!("type=out:duration={}", d.as_secs_f64()),
            Self::Rotate(degrees) => {
                format!("angle={}", degrees.to_radians())
            }
            Self::ToneMap(algorithm) => format!("tonemap={}", algorithm.as_str()),
            Self::Volume(db) => format!("volume={db}dB"),
            Self::Amix(inputs) => format!("inputs={inputs}"),
            Self::Equalizer { band_hz, gain_db } => {
                format!("f={band_hz}:width_type=o:width=2:g={gain_db}")
            }
        }
    }
}

// ── FilterGraphBuilder ────────────────────────────────────────────────────────

/// Builder for constructing a [`FilterGraph`].
///
/// Create one with [`FilterGraph::builder()`], chain the desired filter
/// methods, then call [`build`](Self::build) to obtain the graph.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::{FilterGraph, ToneMap};
///
/// let graph = FilterGraph::builder()
///     .scale(1280, 720)
///     .tone_map(ToneMap::Hable)
///     .build()?;
/// ```
#[derive(Debug, Default)]
pub struct FilterGraphBuilder {
    steps: Vec<FilterStep>,
    hw: Option<HwAccel>,
}

impl FilterGraphBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ── Video filters ─────────────────────────────────────────────────────────

    /// Trim the stream to the half-open interval `[start, end)` in seconds.
    #[must_use]
    pub fn trim(mut self, start: f64, end: f64) -> Self {
        self.steps.push(FilterStep::Trim { start, end });
        self
    }

    /// Scale the video to `width × height` pixels.
    #[must_use]
    pub fn scale(mut self, width: u32, height: u32) -> Self {
        self.steps.push(FilterStep::Scale { width, height });
        self
    }

    /// Crop a rectangle starting at `(x, y)` with the given dimensions.
    #[must_use]
    pub fn crop(mut self, x: u32, y: u32, width: u32, height: u32) -> Self {
        self.steps.push(FilterStep::Crop {
            x,
            y,
            width,
            height,
        });
        self
    }

    /// Overlay a second input stream at position `(x, y)`.
    #[must_use]
    pub fn overlay(mut self, x: i32, y: i32) -> Self {
        self.steps.push(FilterStep::Overlay { x, y });
        self
    }

    /// Fade in from black over the given `duration`.
    #[must_use]
    pub fn fade_in(mut self, duration: Duration) -> Self {
        self.steps.push(FilterStep::FadeIn(duration));
        self
    }

    /// Fade out to black over the given `duration`.
    #[must_use]
    pub fn fade_out(mut self, duration: Duration) -> Self {
        self.steps.push(FilterStep::FadeOut(duration));
        self
    }

    /// Rotate the video clockwise by `degrees`.
    #[must_use]
    pub fn rotate(mut self, degrees: f64) -> Self {
        self.steps.push(FilterStep::Rotate(degrees));
        self
    }

    /// Apply HDR-to-SDR tone mapping using the given `algorithm`.
    #[must_use]
    pub fn tone_map(mut self, algorithm: ToneMap) -> Self {
        self.steps.push(FilterStep::ToneMap(algorithm));
        self
    }

    // ── Audio filters ─────────────────────────────────────────────────────────

    /// Adjust audio volume by `gain_db` decibels (negative = quieter).
    #[must_use]
    pub fn volume(mut self, gain_db: f64) -> Self {
        self.steps.push(FilterStep::Volume(gain_db));
        self
    }

    /// Mix `inputs` audio streams together.
    #[must_use]
    pub fn amix(mut self, inputs: usize) -> Self {
        self.steps.push(FilterStep::Amix(inputs));
        self
    }

    /// Apply a parametric equalizer band at `band_hz` Hz with `gain_db` dB.
    #[must_use]
    pub fn equalizer(mut self, band_hz: f64, gain_db: f64) -> Self {
        self.steps.push(FilterStep::Equalizer { band_hz, gain_db });
        self
    }

    // ── Hardware ──────────────────────────────────────────────────────────────

    /// Enable hardware-accelerated filtering.
    ///
    /// When set, `hwupload` and `hwdownload` filters are inserted around the
    /// filter chain automatically.
    #[must_use]
    pub fn hardware(mut self, hw: HwAccel) -> Self {
        self.hw = Some(hw);
        self
    }

    // ── Build ─────────────────────────────────────────────────────────────────

    /// Build the [`FilterGraph`].
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::BuildFailed`] if `steps` is empty (there is
    /// nothing to filter). The actual `FFmpeg` graph is constructed lazily on the
    /// first [`push_video`](FilterGraph::push_video) or
    /// [`push_audio`](FilterGraph::push_audio) call.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.steps.is_empty() {
            return Err(FilterError::BuildFailed);
        }
        crate::filter_inner::validate_filter_steps(&self.steps)?;
        let output_resolution = self.steps.iter().rev().find_map(|s| {
            if let FilterStep::Scale { width, height } = s {
                Some((*width, *height))
            } else {
                None
            }
        });
        Ok(FilterGraph {
            inner: FilterGraphInner::new(self.steps, self.hw),
            output_resolution,
        })
    }
}

// ── FilterGraph ───────────────────────────────────────────────────────────────

/// An `FFmpeg` libavfilter filter graph.
///
/// Constructed via [`FilterGraph::builder()`].  The underlying `AVFilterGraph` is
/// initialised lazily on the first push call, deriving format information from
/// the first frame.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::FilterGraph;
///
/// let mut graph = FilterGraph::builder()
///     .scale(1280, 720)
///     .build()?;
///
/// // Push decoded frames in …
/// graph.push_video(0, &video_frame)?;
///
/// // … and pull filtered frames out.
/// while let Some(frame) = graph.pull_video()? {
///     // use frame
/// }
/// ```
pub struct FilterGraph {
    inner: FilterGraphInner,
    output_resolution: Option<(u32, u32)>,
}

impl std::fmt::Debug for FilterGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterGraph").finish_non_exhaustive()
    }
}

impl FilterGraph {
    /// Create a new builder.
    #[must_use]
    pub fn builder() -> FilterGraphBuilder {
        FilterGraphBuilder::new()
    }

    /// Returns the output resolution produced by this graph's `scale` filter step,
    /// if one was configured.
    ///
    /// When multiple `scale` steps are chained, the **last** one's dimensions are
    /// returned. Returns `None` when no `scale` step was added.
    #[must_use]
    pub fn output_resolution(&self) -> Option<(u32, u32)> {
        self.output_resolution
    }

    /// Push a video frame into input slot `slot`.
    ///
    /// On the first call the filter graph is initialised using this frame's
    /// format, resolution, and time base.
    ///
    /// # Errors
    ///
    /// - [`FilterError::InvalidInput`] if `slot` is out of range.
    /// - [`FilterError::BuildFailed`] if the graph cannot be initialised.
    /// - [`FilterError::ProcessFailed`] if the `FFmpeg` push fails.
    pub fn push_video(&mut self, slot: usize, frame: &VideoFrame) -> Result<(), FilterError> {
        self.inner.push_video(slot, frame)
    }

    /// Pull the next filtered video frame, if one is available.
    ///
    /// Returns `None` when the internal `FFmpeg` buffer is empty (EAGAIN) or
    /// at end-of-stream.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::ProcessFailed`] on an unexpected `FFmpeg` error.
    pub fn pull_video(&mut self) -> Result<Option<VideoFrame>, FilterError> {
        self.inner.pull_video()
    }

    /// Push an audio frame into input slot `slot`.
    ///
    /// On the first call the audio filter graph is initialised using this
    /// frame's format, sample rate, and channel count.
    ///
    /// # Errors
    ///
    /// - [`FilterError::InvalidInput`] if `slot` is out of range.
    /// - [`FilterError::BuildFailed`] if the graph cannot be initialised.
    /// - [`FilterError::ProcessFailed`] if the `FFmpeg` push fails.
    pub fn push_audio(&mut self, slot: usize, frame: &AudioFrame) -> Result<(), FilterError> {
        self.inner.push_audio(slot, frame)
    }

    /// Pull the next filtered audio frame, if one is available.
    ///
    /// Returns `None` when the internal `FFmpeg` buffer is empty (EAGAIN) or
    /// at end-of-stream.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::ProcessFailed`] on an unexpected `FFmpeg` error.
    pub fn pull_audio(&mut self) -> Result<Option<AudioFrame>, FilterError> {
        self.inner.pull_audio()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_step_scale_should_produce_correct_args() {
        let step = FilterStep::Scale {
            width: 1280,
            height: 720,
        };
        assert_eq!(step.filter_name(), "scale");
        assert_eq!(step.args(), "w=1280:h=720");
    }

    #[test]
    fn filter_step_trim_should_produce_correct_args() {
        let step = FilterStep::Trim {
            start: 10.0,
            end: 30.0,
        };
        assert_eq!(step.filter_name(), "trim");
        assert_eq!(step.args(), "start=10:end=30");
    }

    #[test]
    fn filter_step_volume_should_produce_correct_args() {
        let step = FilterStep::Volume(-6.0);
        assert_eq!(step.filter_name(), "volume");
        assert_eq!(step.args(), "volume=-6dB");
    }

    #[test]
    fn tone_map_variants_should_have_correct_names() {
        assert_eq!(ToneMap::Hable.as_str(), "hable");
        assert_eq!(ToneMap::Reinhard.as_str(), "reinhard");
        assert_eq!(ToneMap::Mobius.as_str(), "mobius");
    }

    #[test]
    fn builder_empty_steps_should_return_error() {
        let result = FilterGraph::builder().build();
        assert!(
            matches!(result, Err(FilterError::BuildFailed)),
            "expected BuildFailed, got {result:?}"
        );
    }

    #[test]
    fn filter_step_overlay_should_produce_correct_args() {
        let step = FilterStep::Overlay { x: 10, y: 20 };
        assert_eq!(step.filter_name(), "overlay");
        assert_eq!(step.args(), "x=10:y=20");
    }

    #[test]
    fn filter_step_crop_should_produce_correct_args() {
        let step = FilterStep::Crop {
            x: 0,
            y: 0,
            width: 640,
            height: 360,
        };
        assert_eq!(step.filter_name(), "crop");
        assert_eq!(step.args(), "x=0:y=0:w=640:h=360");
    }

    #[test]
    fn filter_step_fade_in_should_produce_correct_args() {
        let step = FilterStep::FadeIn(Duration::from_secs(1));
        assert_eq!(step.filter_name(), "fade");
        assert_eq!(step.args(), "type=in:duration=1");
    }

    #[test]
    fn filter_step_fade_out_should_produce_correct_args() {
        let step = FilterStep::FadeOut(Duration::from_secs(2));
        assert_eq!(step.filter_name(), "fade");
        assert_eq!(step.args(), "type=out:duration=2");
    }

    #[test]
    fn filter_step_rotate_should_produce_correct_args() {
        let step = FilterStep::Rotate(90.0);
        assert_eq!(step.filter_name(), "rotate");
        assert_eq!(step.args(), format!("angle={}", 90_f64.to_radians()));
    }

    #[test]
    fn filter_step_tone_map_should_produce_correct_args() {
        let step = FilterStep::ToneMap(ToneMap::Hable);
        assert_eq!(step.filter_name(), "tonemap");
        assert_eq!(step.args(), "tonemap=hable");
    }

    #[test]
    fn filter_step_amix_should_produce_correct_args() {
        let step = FilterStep::Amix(3);
        assert_eq!(step.filter_name(), "amix");
        assert_eq!(step.args(), "inputs=3");
    }

    #[test]
    fn filter_step_equalizer_should_produce_correct_args() {
        let step = FilterStep::Equalizer {
            band_hz: 1000.0,
            gain_db: 3.0,
        };
        assert_eq!(step.filter_name(), "equalizer");
        assert_eq!(step.args(), "f=1000:width_type=o:width=2:g=3");
    }

    #[test]
    fn builder_steps_should_accumulate_in_order() {
        let result = FilterGraph::builder()
            .trim(0.0, 5.0)
            .scale(1280, 720)
            .volume(-3.0)
            .build();
        assert!(
            result.is_ok(),
            "builder with multiple valid steps must succeed, got {result:?}"
        );
    }

    #[test]
    fn builder_with_valid_steps_should_succeed() {
        let result = FilterGraph::builder().scale(1280, 720).build();
        assert!(
            result.is_ok(),
            "builder with a known filter step must succeed, got {result:?}"
        );
    }

    #[test]
    fn output_resolution_should_return_scale_dimensions() {
        let fg = FilterGraph::builder().scale(1280, 720).build().unwrap();
        assert_eq!(fg.output_resolution(), Some((1280, 720)));
    }

    #[test]
    fn output_resolution_should_return_last_scale_when_chained() {
        let fg = FilterGraph::builder()
            .scale(1920, 1080)
            .scale(1280, 720)
            .build()
            .unwrap();
        assert_eq!(fg.output_resolution(), Some((1280, 720)));
    }

    #[test]
    fn output_resolution_should_return_none_when_no_scale() {
        let fg = FilterGraph::builder().trim(0.0, 5.0).build().unwrap();
        assert_eq!(fg.output_resolution(), None);
    }
}
