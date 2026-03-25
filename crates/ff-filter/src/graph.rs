//! Filter graph public API: [`FilterGraph`] and [`FilterGraphBuilder`].

use std::path::Path;
use std::time::Duration;

use ff_format::{AudioFrame, VideoFrame};

use crate::error::FilterError;
use crate::filter_inner::FilterGraphInner;

// ── Supporting enums ──────────────────────────────────────────────────────────

/// Tone-mapping algorithm for HDR-to-SDR conversion.
///
/// Used with [`FilterGraphBuilder::tone_map`].
///
/// # Choosing an algorithm
///
/// | Variant | Characteristic | When to use |
/// |---------|---------------|-------------|
/// | [`Hable`](Self::Hable) | Filmic, rich contrast | Film / cinematic content |
/// | [`Reinhard`](Self::Reinhard) | Simple, fast, neutral | Fast previews, general video |
/// | [`Mobius`](Self::Mobius) | Smooth highlights | Bright outdoor or HDR10 content |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneMap {
    /// Hable (Uncharted 2) filmic tone mapping.
    ///
    /// Produces a warm, cinematic look with compressed shadows and highlights.
    /// The most commonly used algorithm for film and narrative video content.
    Hable,
    /// Reinhard tone mapping.
    ///
    /// A simple, globally uniform operator. Fast and neutral; a safe default
    /// when color-accurate reproduction matters more than filmic aesthetics.
    Reinhard,
    /// Mobius tone mapping.
    ///
    /// A smooth, shoulder-based curve that preserves mid-tones while gently
    /// rolling off bright highlights. Well suited for outdoor and HDR10 content.
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
    /// Apply a 3D LUT from a `.cube` or `.3dl` file.
    Lut3d { path: String },
    /// Brightness/contrast/saturation adjustment via `FFmpeg` `eq` filter.
    Eq {
        brightness: f32,
        contrast: f32,
        saturation: f32,
    },
    /// Per-channel RGB color curves adjustment.
    Curves {
        master: Vec<(f32, f32)>,
        r: Vec<(f32, f32)>,
        g: Vec<(f32, f32)>,
        b: Vec<(f32, f32)>,
    },
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
            Self::Lut3d { .. } => "lut3d",
            Self::Eq { .. } => "eq",
            Self::Curves { .. } => "curves",
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
            Self::Lut3d { path } => format!("file={path}:interp=trilinear"),
            Self::Eq {
                brightness,
                contrast,
                saturation,
            } => format!("brightness={brightness}:contrast={contrast}:saturation={saturation}"),
            Self::Curves { master, r, g, b } => {
                let fmt = |pts: &[(f32, f32)]| -> String {
                    pts.iter()
                        .map(|(x, y)| format!("{x}/{y}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                [("master", master.as_slice()), ("r", r), ("g", g), ("b", b)]
                    .iter()
                    .filter(|(_, pts)| !pts.is_empty())
                    .map(|(name, pts)| format!("{name}='{}'", fmt(pts)))
                    .collect::<Vec<_>>()
                    .join(":")
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

    /// Apply a 3D LUT colour grade from a `.cube` or `.3dl` file.
    ///
    /// Uses `FFmpeg`'s `lut3d` filter with trilinear interpolation.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if:
    /// - the extension is not `.cube` or `.3dl`, or
    /// - the file does not exist at build time.
    #[must_use]
    pub fn lut3d(mut self, path: &str) -> Self {
        self.steps.push(FilterStep::Lut3d {
            path: path.to_owned(),
        });
        self
    }

    /// Adjust brightness, contrast, and saturation using `FFmpeg`'s `eq` filter.
    ///
    /// Valid ranges:
    /// - `brightness`: −1.0 – 1.0 (neutral: 0.0)
    /// - `contrast`: 0.0 – 3.0 (neutral: 1.0)
    /// - `saturation`: 0.0 – 3.0 (neutral: 1.0; 0.0 = grayscale)
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if any
    /// value is outside its valid range.
    #[must_use]
    pub fn eq(mut self, brightness: f32, contrast: f32, saturation: f32) -> Self {
        self.steps.push(FilterStep::Eq {
            brightness,
            contrast,
            saturation,
        });
        self
    }

    /// Apply per-channel RGB color curves using `FFmpeg`'s `curves` filter.
    ///
    /// Each argument is a list of `(input, output)` control points in `[0.0, 1.0]`.
    /// Pass an empty `Vec` for any channel that needs no adjustment.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if any
    /// control point coordinate is outside `[0.0, 1.0]`.
    #[must_use]
    pub fn curves(
        mut self,
        master: Vec<(f32, f32)>,
        r: Vec<(f32, f32)>,
        g: Vec<(f32, f32)>,
        b: Vec<(f32, f32)>,
    ) -> Self {
        self.steps.push(FilterStep::Curves { master, r, g, b });
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

        // Validate overlay coordinates: negative x or y places the overlay
        // entirely off-screen, which is almost always a misconfiguration
        // (e.g. a watermark larger than the video). Catch it early with a
        // descriptive error rather than silently producing invisible output.
        for step in &self.steps {
            if let FilterStep::Overlay { x, y } = step
                && (*x < 0 || *y < 0)
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!(
                        "overlay position ({x}, {y}) is off-screen; \
                         ensure the watermark fits within the video dimensions"
                    ),
                });
            }
            if let FilterStep::Lut3d { path } = step {
                let ext = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if !matches!(ext, "cube" | "3dl") {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("unsupported LUT format: .{ext}; expected .cube or .3dl"),
                    });
                }
                if !Path::new(path).exists() {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("LUT file not found: {path}"),
                    });
                }
            }
            if let FilterStep::Eq {
                brightness,
                contrast,
                saturation,
            } = step
            {
                if !(-1.0..=1.0).contains(brightness) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("eq brightness {brightness} out of range [-1.0, 1.0]"),
                    });
                }
                if !(0.0..=3.0).contains(contrast) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("eq contrast {contrast} out of range [0.0, 3.0]"),
                    });
                }
                if !(0.0..=3.0).contains(saturation) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("eq saturation {saturation} out of range [0.0, 3.0]"),
                    });
                }
            }
            if let FilterStep::Curves { master, r, g, b } = step {
                for (channel, pts) in [
                    ("master", master.as_slice()),
                    ("r", r.as_slice()),
                    ("g", g.as_slice()),
                    ("b", b.as_slice()),
                ] {
                    for &(x, y) in pts {
                        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                            return Err(FilterError::InvalidConfig {
                                reason: format!(
                                    "curves {channel} control point ({x}, {y}) out of range [0.0, 1.0]"
                                ),
                            });
                        }
                    }
                }
            }
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

    #[test]
    fn filter_step_lut3d_should_produce_correct_filter_name() {
        let step = FilterStep::Lut3d {
            path: "grade.cube".to_owned(),
        };
        assert_eq!(step.filter_name(), "lut3d");
    }

    #[test]
    fn filter_step_lut3d_should_produce_correct_args() {
        let step = FilterStep::Lut3d {
            path: "grade.cube".to_owned(),
        };
        assert_eq!(step.args(), "file=grade.cube:interp=trilinear");
    }

    #[test]
    fn builder_lut3d_with_unsupported_extension_should_return_invalid_config() {
        let result = FilterGraph::builder().lut3d("color_grade.txt").build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for unsupported extension, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("unsupported LUT format"),
                "reason should mention unsupported format: {reason}"
            );
        }
    }

    #[test]
    fn builder_lut3d_with_no_extension_should_return_invalid_config() {
        let result = FilterGraph::builder().lut3d("color_grade_no_ext").build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for missing extension, got {result:?}"
        );
    }

    #[test]
    fn builder_lut3d_with_nonexistent_cube_file_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .lut3d("/nonexistent/path/grade_ab12cd.cube")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for nonexistent file, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("LUT file not found"),
                "reason should mention file not found: {reason}"
            );
        }
    }

    #[test]
    fn builder_lut3d_with_nonexistent_3dl_file_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .lut3d("/nonexistent/path/grade_ab12cd.3dl")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for nonexistent .3dl file, got {result:?}"
        );
    }

    #[test]
    fn filter_step_eq_should_produce_correct_filter_name() {
        let step = FilterStep::Eq {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
        };
        assert_eq!(step.filter_name(), "eq");
    }

    #[test]
    fn filter_step_eq_should_produce_correct_args() {
        let step = FilterStep::Eq {
            brightness: 0.1,
            contrast: 1.5,
            saturation: 0.8,
        };
        assert_eq!(step.args(), "brightness=0.1:contrast=1.5:saturation=0.8");
    }

    #[test]
    fn builder_eq_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().eq(0.0, 1.0, 1.0).build();
        assert!(
            result.is_ok(),
            "neutral eq params must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_eq_with_brightness_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().eq(-1.5, 1.0, 1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for brightness < -1.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("brightness"),
                "reason should mention brightness: {reason}"
            );
        }
    }

    #[test]
    fn builder_eq_with_brightness_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().eq(1.5, 1.0, 1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for brightness > 1.0, got {result:?}"
        );
    }

    #[test]
    fn builder_eq_with_contrast_out_of_range_should_return_invalid_config() {
        let result = FilterGraph::builder().eq(0.0, 4.0, 1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for contrast > 3.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("contrast"),
                "reason should mention contrast: {reason}"
            );
        }
    }

    #[test]
    fn builder_eq_with_saturation_out_of_range_should_return_invalid_config() {
        let result = FilterGraph::builder().eq(0.0, 1.0, -0.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for saturation < 0.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("saturation"),
                "reason should mention saturation: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_curves_should_produce_correct_filter_name() {
        let step = FilterStep::Curves {
            master: vec![],
            r: vec![],
            g: vec![],
            b: vec![],
        };
        assert_eq!(step.filter_name(), "curves");
    }

    #[test]
    fn filter_step_curves_should_produce_args_with_all_channels() {
        let step = FilterStep::Curves {
            master: vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)],
            r: vec![(0.0, 0.0), (1.0, 1.0)],
            g: vec![],
            b: vec![(0.0, 0.0), (1.0, 0.8)],
        };
        let args = step.args();
        assert!(args.contains("master='0/0 0.5/0.6 1/1'"), "args={args}");
        assert!(args.contains("r='0/0 1/1'"), "args={args}");
        assert!(
            !args.contains("g="),
            "empty g channel should be omitted: args={args}"
        );
        assert!(args.contains("b='0/0 1/0.8'"), "args={args}");
    }

    #[test]
    fn filter_step_curves_with_empty_channels_should_produce_empty_args() {
        let step = FilterStep::Curves {
            master: vec![],
            r: vec![],
            g: vec![],
            b: vec![],
        };
        assert_eq!(
            step.args(),
            "",
            "all-empty curves should produce empty args string"
        );
    }

    #[test]
    fn builder_curves_with_valid_s_curve_should_succeed() {
        let result = FilterGraph::builder()
            .curves(
                vec![
                    (0.0, 0.0),
                    (0.25, 0.15),
                    (0.5, 0.5),
                    (0.75, 0.85),
                    (1.0, 1.0),
                ],
                vec![],
                vec![],
                vec![],
            )
            .build();
        assert!(
            result.is_ok(),
            "valid S-curve master must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_curves_with_out_of_range_point_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .curves(vec![(0.0, 1.5)], vec![], vec![], vec![])
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for out-of-range point, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("curves") && reason.contains("master"),
                "reason should mention curves master: {reason}"
            );
        }
    }

    #[test]
    fn builder_curves_with_out_of_range_r_channel_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .curves(vec![], vec![(1.2, 0.5)], vec![], vec![])
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for out-of-range r channel point, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("curves") && reason.contains(" r "),
                "reason should mention curves r: {reason}"
            );
        }
    }
}
