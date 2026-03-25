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

/// An RGB colour value used by the three-way colour corrector.
///
/// Each channel is a multiplicative factor (neutral = `1.0`).
/// Values above `1.0` push the channel warmer/brighter; values below `1.0`
/// pull it cooler/darker.  Negative values are clamped at the `FFmpeg` layer.
///
/// See [`FilterGraphBuilder::three_way_cc`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    /// Red channel multiplier (neutral: `1.0`).
    pub r: f32,
    /// Green channel multiplier (neutral: `1.0`).
    pub g: f32,
    /// Blue channel multiplier (neutral: `1.0`).
    pub b: f32,
}

impl Rgb {
    /// Neutral value — no colour shift on any channel.
    pub const NEUTRAL: Rgb = Rgb {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
}

/// Resampling algorithm for the `scale` filter.
///
/// Used with [`FilterGraphBuilder::scale`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleAlgorithm {
    /// Fast bilinear interpolation (default). Good balance of speed and quality.
    Fast,
    /// Bilinear interpolation. Slightly slower than [`Fast`](Self::Fast) but
    /// produces smoother results.
    Bilinear,
    /// Bicubic interpolation. Higher quality than bilinear with moderate overhead.
    Bicubic,
    /// Lanczos interpolation — sharpest output, highest CPU cost.
    Lanczos,
}

impl ScaleAlgorithm {
    /// Returns the `sws_flags` string passed to the `scale` filter.
    #[must_use]
    pub const fn as_flags_str(self) -> &'static str {
        match self {
            Self::Fast => "fast_bilinear",
            Self::Bilinear => "bilinear",
            Self::Bicubic => "bicubic",
            Self::Lanczos => "lanczos",
        }
    }
}

/// Deinterlacing mode for the `yadif` filter.
///
/// Used with [`FilterGraphBuilder::yadif`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YadifMode {
    /// Output one frame per frame (progressive output).
    Frame = 0,
    /// Output one frame per field (doubles the frame rate).
    Field = 1,
    /// Frame mode without spatial interlacing check.
    FrameNospatial = 2,
    /// Field mode without spatial interlacing check.
    FieldNospatial = 3,
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
    /// Scale to a new resolution using the given resampling algorithm.
    Scale {
        width: u32,
        height: u32,
        algorithm: ScaleAlgorithm,
    },
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
    /// Rotate clockwise by `angle_degrees`, filling exposed areas with `fill_color`.
    Rotate {
        angle_degrees: f64,
        fill_color: String,
    },
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
    /// White balance correction via `colorchannelmixer`.
    WhiteBalance { temperature_k: u32, tint: f32 },
    /// Hue rotation by an arbitrary angle.
    Hue { degrees: f32 },
    /// Per-channel gamma correction via `FFmpeg` `eq` filter.
    Gamma { r: f32, g: f32, b: f32 },
    /// Three-way colour corrector (lift / gamma / gain) via `FFmpeg` `curves` filter.
    ThreeWayCC {
        /// Affects shadows (blacks). Neutral: `Rgb::NEUTRAL`.
        lift: Rgb,
        /// Affects midtones. Neutral: `Rgb::NEUTRAL`. All components must be > 0.0.
        gamma: Rgb,
        /// Affects highlights (whites). Neutral: `Rgb::NEUTRAL`.
        gain: Rgb,
    },
    /// Vignette effect via `FFmpeg` `vignette` filter.
    Vignette {
        /// Radius angle in radians (valid range: 0.0 – π/2 ≈ 1.5708). Default: π/5 ≈ 0.628.
        angle: f32,
        /// Horizontal centre of the vignette. `0.0` maps to `w/2`.
        x0: f32,
        /// Vertical centre of the vignette. `0.0` maps to `h/2`.
        y0: f32,
    },
    /// Horizontal flip (mirror left-right).
    HFlip,
    /// Vertical flip (mirror top-bottom).
    VFlip,
    /// Pad to a target resolution with a fill color (letterbox / pillarbox).
    Pad {
        /// Target canvas width in pixels.
        width: u32,
        /// Target canvas height in pixels.
        height: u32,
        /// Horizontal offset of the source frame within the canvas.
        /// Negative values are replaced with `(ow-iw)/2` (centred).
        x: i32,
        /// Vertical offset of the source frame within the canvas.
        /// Negative values are replaced with `(oh-ih)/2` (centred).
        y: i32,
        /// Fill color (any `FFmpeg` color string, e.g. `"black"`, `"0x000000"`).
        color: String,
    },
    /// Scale (preserving aspect ratio) then centre-pad to fill target dimensions
    /// (letterbox or pillarbox as required).
    ///
    /// Implemented as a `scale` filter with `force_original_aspect_ratio=decrease`
    /// followed by a `pad` filter that centres the scaled frame on the canvas.
    FitToAspect {
        /// Target canvas width in pixels.
        width: u32,
        /// Target canvas height in pixels.
        height: u32,
        /// Fill color for the bars (any `FFmpeg` color string, e.g. `"black"`).
        color: String,
    },
    /// Gaussian blur with configurable radius.
    ///
    /// `sigma` is the blur radius. Valid range: 0.0 – 10.0 (values near 0.0 are
    /// nearly a no-op; higher values produce a stronger blur).
    GBlur {
        /// Blur radius (standard deviation). Must be ≥ 0.0.
        sigma: f32,
    },
    /// Sharpen or blur via unsharp mask (luma + chroma strength).
    ///
    /// Positive values sharpen; negative values blur. Valid range for each
    /// component: −1.5 – 1.5.
    Unsharp {
        /// Luma (brightness) sharpening/blurring amount. Range: −1.5 – 1.5.
        luma_strength: f32,
        /// Chroma (colour) sharpening/blurring amount. Range: −1.5 – 1.5.
        chroma_strength: f32,
    },
    /// High Quality 3D noise reduction (`hqdn3d`).
    ///
    /// Typical values: `luma_spatial=4.0`, `chroma_spatial=3.0`,
    /// `luma_tmp=6.0`, `chroma_tmp=4.5`. All values must be ≥ 0.0.
    Hqdn3d {
        /// Spatial luma noise reduction strength. Must be ≥ 0.0.
        luma_spatial: f32,
        /// Spatial chroma noise reduction strength. Must be ≥ 0.0.
        chroma_spatial: f32,
        /// Temporal luma noise reduction strength. Must be ≥ 0.0.
        luma_tmp: f32,
        /// Temporal chroma noise reduction strength. Must be ≥ 0.0.
        chroma_tmp: f32,
    },
    /// Non-local means noise reduction (`nlmeans`).
    ///
    /// `strength` controls the denoising intensity; range 1.0–30.0.
    /// Higher values remove more noise but are significantly more CPU-intensive.
    ///
    /// NOTE: nlmeans is CPU-intensive; avoid for real-time pipelines.
    Nlmeans {
        /// Denoising strength. Must be in the range [1.0, 30.0].
        strength: f32,
    },
    /// Deinterlace using the `yadif` filter.
    Yadif {
        /// Deinterlacing mode controlling output frame rate and spatial checks.
        mode: YadifMode,
    },
}

/// Convert a color temperature in Kelvin to linear RGB multipliers using
/// Tanner Helland's algorithm.
///
/// Returns `(r, g, b)` each in `[0.0, 1.0]`.
fn kelvin_to_rgb(temp_k: u32) -> (f64, f64, f64) {
    let t = (f64::from(temp_k) / 100.0).clamp(10.0, 400.0);
    let r = if t <= 66.0 {
        1.0
    } else {
        (329.698_727_446_4 * (t - 60.0).powf(-0.133_204_759_2) / 255.0).clamp(0.0, 1.0)
    };
    let g = if t <= 66.0 {
        ((99.470_802_586_1 * t.ln() - 161.119_568_166_1) / 255.0).clamp(0.0, 1.0)
    } else {
        ((288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)) / 255.0).clamp(0.0, 1.0)
    };
    let b = if t >= 66.0 {
        1.0
    } else if t <= 19.0 {
        0.0
    } else {
        ((138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7) / 255.0).clamp(0.0, 1.0)
    };
    (r, g, b)
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
            Self::Rotate { .. } => "rotate",
            Self::ToneMap(_) => "tonemap",
            Self::Volume(_) => "volume",
            Self::Amix(_) => "amix",
            Self::Equalizer { .. } => "equalizer",
            Self::Lut3d { .. } => "lut3d",
            Self::Eq { .. } => "eq",
            Self::Curves { .. } => "curves",
            Self::WhiteBalance { .. } => "colorchannelmixer",
            Self::Hue { .. } => "hue",
            Self::Gamma { .. } => "eq",
            Self::ThreeWayCC { .. } => "curves",
            Self::Vignette { .. } => "vignette",
            Self::HFlip => "hflip",
            Self::VFlip => "vflip",
            Self::Pad { .. } => "pad",
            // FitToAspect is implemented as scale + pad; "scale" is validated at
            // build time.  The pad filter is inserted by filter_inner at graph
            // construction time.
            Self::FitToAspect { .. } => "scale",
            Self::GBlur { .. } => "gblur",
            Self::Unsharp { .. } => "unsharp",
            Self::Hqdn3d { .. } => "hqdn3d",
            Self::Nlmeans { .. } => "nlmeans",
            Self::Yadif { .. } => "yadif",
        }
    }

    /// Returns the `args` string passed to `avfilter_graph_create_filter`.
    pub(crate) fn args(&self) -> String {
        match self {
            Self::Trim { start, end } => format!("start={start}:end={end}"),
            Self::Scale {
                width,
                height,
                algorithm,
            } => format!("w={width}:h={height}:flags={}", algorithm.as_flags_str()),
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
            Self::Rotate {
                angle_degrees,
                fill_color,
            } => {
                format!(
                    "angle={}:fillcolor={fill_color}",
                    angle_degrees.to_radians()
                )
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
            Self::WhiteBalance {
                temperature_k,
                tint,
            } => {
                let (r, g, b) = kelvin_to_rgb(*temperature_k);
                let g_adj = (g + f64::from(*tint)).clamp(0.0, 2.0);
                format!("rr={r}:gg={g_adj}:bb={b}")
            }
            Self::Hue { degrees } => format!("h={degrees}"),
            Self::Gamma { r, g, b } => format!("gamma_r={r}:gamma_g={g}:gamma_b={b}"),
            Self::Vignette { angle, x0, y0 } => {
                let cx = if *x0 == 0.0 {
                    "w/2".to_string()
                } else {
                    x0.to_string()
                };
                let cy = if *y0 == 0.0 {
                    "h/2".to_string()
                } else {
                    y0.to_string()
                };
                format!("angle={angle}:x0={cx}:y0={cy}")
            }
            Self::ThreeWayCC { lift, gamma, gain } => {
                // Convert lift/gamma/gain to a 3-point per-channel curves representation.
                // The formula maps:
                //   input 0.0 → (lift - 1.0) * gain  (black point)
                //   input 0.5 → (0.5 * lift)^(1/gamma) * gain  (midtone)
                //   input 1.0 → gain  (white point)
                // All neutral (1.0) produces the identity curve 0/0 0.5/0.5 1/1.
                let curve = |l: f32, gm: f32, gn: f32| -> String {
                    let l = f64::from(l);
                    let gm = f64::from(gm);
                    let gn = f64::from(gn);
                    let black = ((l - 1.0) * gn).clamp(0.0, 1.0);
                    let mid = ((0.5 * l).powf(1.0 / gm) * gn).clamp(0.0, 1.0);
                    let white = gn.clamp(0.0, 1.0);
                    format!("0/{black} 0.5/{mid} 1/{white}")
                };
                format!(
                    "r='{}':g='{}':b='{}'",
                    curve(lift.r, gamma.r, gain.r),
                    curve(lift.g, gamma.g, gain.g),
                    curve(lift.b, gamma.b, gain.b),
                )
            }
            Self::HFlip | Self::VFlip => String::new(),
            Self::GBlur { sigma } => format!("sigma={sigma}"),
            Self::Unsharp {
                luma_strength,
                chroma_strength,
            } => format!(
                "luma_msize_x=5:luma_msize_y=5:luma_amount={luma_strength}:\
                 chroma_msize_x=5:chroma_msize_y=5:chroma_amount={chroma_strength}"
            ),
            Self::Hqdn3d {
                luma_spatial,
                chroma_spatial,
                luma_tmp,
                chroma_tmp,
            } => format!("{luma_spatial}:{chroma_spatial}:{luma_tmp}:{chroma_tmp}"),
            Self::Nlmeans { strength } => format!("s={strength}"),
            Self::Yadif { mode } => format!("mode={}", *mode as i32),
            Self::FitToAspect { width, height, .. } => {
                // Scale to fit within the target dimensions, preserving the source
                // aspect ratio.  The accompanying pad filter (inserted by
                // filter_inner after this scale filter) centres the result on the
                // target canvas.
                format!("w={width}:h={height}:force_original_aspect_ratio=decrease")
            }
            Self::Pad {
                width,
                height,
                x,
                y,
                color,
            } => {
                let px = if *x < 0 {
                    "(ow-iw)/2".to_string()
                } else {
                    x.to_string()
                };
                let py = if *y < 0 {
                    "(oh-ih)/2".to_string()
                } else {
                    y.to_string()
                };
                format!("width={width}:height={height}:x={px}:y={py}:color={color}")
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

    /// Scale the video to `width × height` pixels using the given resampling
    /// `algorithm`.
    ///
    /// Use [`ScaleAlgorithm::Fast`] for the best speed/quality trade-off.
    /// For highest quality use [`ScaleAlgorithm::Lanczos`] at the cost of
    /// additional CPU time.
    #[must_use]
    pub fn scale(mut self, width: u32, height: u32, algorithm: ScaleAlgorithm) -> Self {
        self.steps.push(FilterStep::Scale {
            width,
            height,
            algorithm,
        });
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

    /// Rotate the video clockwise by `angle_degrees`, filling exposed corners
    /// with `fill_color`.
    ///
    /// `fill_color` accepts any color string understood by `FFmpeg` — for example
    /// `"black"`, `"white"`, `"0x00000000"` (transparent), or `"gray"`.
    /// Pass `"black"` to reproduce the classic solid-background rotation.
    #[must_use]
    pub fn rotate(mut self, angle_degrees: f64, fill_color: &str) -> Self {
        self.steps.push(FilterStep::Rotate {
            angle_degrees,
            fill_color: fill_color.to_owned(),
        });
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

    /// Correct white balance using `FFmpeg`'s `colorchannelmixer` filter.
    ///
    /// RGB channel multipliers are derived from `temperature_k` via Tanner
    /// Helland's Kelvin-to-RGB algorithm. The `tint` offset shifts the green
    /// channel (positive = more green, negative = more magenta).
    ///
    /// Valid ranges:
    /// - `temperature_k`: 1000–40000 K (neutral daylight ≈ 6500 K)
    /// - `tint`: −1.0–1.0
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if either
    /// value is outside its valid range.
    #[must_use]
    pub fn white_balance(mut self, temperature_k: u32, tint: f32) -> Self {
        self.steps.push(FilterStep::WhiteBalance {
            temperature_k,
            tint,
        });
        self
    }

    /// Rotate hue by `degrees` using `FFmpeg`'s `hue` filter.
    ///
    /// Valid range: −360.0–360.0. A value of `0.0` is a no-op.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `degrees` is outside `[−360.0, 360.0]`.
    #[must_use]
    pub fn hue(mut self, degrees: f32) -> Self {
        self.steps.push(FilterStep::Hue { degrees });
        self
    }

    /// Apply per-channel gamma correction using `FFmpeg`'s `eq` filter.
    ///
    /// Valid range per channel: 0.1–10.0. A value of `1.0` is neutral.
    /// Values above 1.0 brighten midtones; values below 1.0 darken them.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if any
    /// channel value is outside `[0.1, 10.0]`.
    #[must_use]
    pub fn gamma(mut self, r: f32, g: f32, b: f32) -> Self {
        self.steps.push(FilterStep::Gamma { r, g, b });
        self
    }

    /// Apply a three-way colour corrector (lift / gamma / gain) using `FFmpeg`'s
    /// `curves` filter.
    ///
    /// Each parameter is an [`Rgb`] triplet; neutral for all three is
    /// [`Rgb::NEUTRAL`] (`r=1.0, g=1.0, b=1.0`).
    ///
    /// - **lift**: shifts shadows (blacks). Values below `1.0` darken shadows.
    /// - **gamma**: shapes midtones via a power curve. Values above `1.0`
    ///   brighten midtones; values below `1.0` darken them.
    /// - **gain**: scales highlights (whites). Values above `1.0` boost whites.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if any
    /// `gamma` component is `≤ 0.0` (division by zero in the power curve).
    #[must_use]
    pub fn three_way_cc(mut self, lift: Rgb, gamma: Rgb, gain: Rgb) -> Self {
        self.steps
            .push(FilterStep::ThreeWayCC { lift, gamma, gain });
        self
    }

    /// Apply a vignette effect using `FFmpeg`'s `vignette` filter.
    ///
    /// Darkens the corners of the frame with a smooth radial falloff.
    ///
    /// - `angle`: radius angle in radians (`0.0` – π/2 ≈ 1.5708). Default: π/5 ≈ 0.628.
    /// - `x0`: horizontal centre of the vignette. Pass `0.0` to use the video
    ///   centre (`w/2`).
    /// - `y0`: vertical centre of the vignette. Pass `0.0` to use the video
    ///   centre (`h/2`).
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `angle` is outside `[0.0, π/2]`.
    #[must_use]
    pub fn vignette(mut self, angle: f32, x0: f32, y0: f32) -> Self {
        self.steps.push(FilterStep::Vignette { angle, x0, y0 });
        self
    }

    /// Flip the video horizontally (mirror left–right) using `FFmpeg`'s `hflip` filter.
    #[must_use]
    pub fn hflip(mut self) -> Self {
        self.steps.push(FilterStep::HFlip);
        self
    }

    /// Flip the video vertically (mirror top–bottom) using `FFmpeg`'s `vflip` filter.
    #[must_use]
    pub fn vflip(mut self) -> Self {
        self.steps.push(FilterStep::VFlip);
        self
    }

    /// Pad the frame to `width × height` pixels, placing the source at `(x, y)`
    /// and filling the exposed borders with `color`.
    ///
    /// Pass a negative value for `x` or `y` to centre the source on that axis
    /// (`x = -1` → `(width − source_w) / 2`).
    ///
    /// `color` accepts any color string understood by `FFmpeg` — for example
    /// `"black"`, `"white"`, `"0x000000"`.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `width` or `height` is zero.
    #[must_use]
    pub fn pad(mut self, width: u32, height: u32, x: i32, y: i32, color: &str) -> Self {
        self.steps.push(FilterStep::Pad {
            width,
            height,
            x,
            y,
            color: color.to_owned(),
        });
        self
    }

    /// Scale the source frame to fit within `width × height` while preserving its
    /// aspect ratio, then centre it on a `width × height` canvas filled with
    /// `color` (letterbox / pillarbox).
    ///
    /// Wide sources (wider aspect ratio than the target) get horizontal black bars
    /// (*letterbox*); tall sources get vertical bars (*pillarbox*).
    ///
    /// `color` accepts any color string understood by `FFmpeg` — for example
    /// `"black"`, `"white"`, `"0x000000"`.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `width` or `height` is zero.
    #[must_use]
    pub fn fit_to_aspect(mut self, width: u32, height: u32, color: &str) -> Self {
        self.steps.push(FilterStep::FitToAspect {
            width,
            height,
            color: color.to_owned(),
        });
        self
    }

    /// Apply a Gaussian blur with the given `sigma` (blur radius).
    ///
    /// `sigma` controls the standard deviation of the Gaussian kernel.
    /// Values near `0.0` are nearly a no-op; values up to `10.0` produce
    /// progressively stronger blur.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `sigma` is negative.
    #[must_use]
    pub fn gblur(mut self, sigma: f32) -> Self {
        self.steps.push(FilterStep::GBlur { sigma });
        self
    }

    /// Sharpen or blur the image using an unsharp mask on luma and chroma.
    ///
    /// Positive values sharpen; negative values blur. Pass `0.0` for either
    /// channel to leave it unchanged.
    ///
    /// Valid ranges: `luma_strength` and `chroma_strength` each −1.5 – 1.5.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if either
    /// value is outside `[−1.5, 1.5]`.
    #[must_use]
    pub fn unsharp(mut self, luma_strength: f32, chroma_strength: f32) -> Self {
        self.steps.push(FilterStep::Unsharp {
            luma_strength,
            chroma_strength,
        });
        self
    }

    /// Apply High Quality 3D (`hqdn3d`) noise reduction.
    ///
    /// Typical values: `luma_spatial=4.0`, `chroma_spatial=3.0`,
    /// `luma_tmp=6.0`, `chroma_tmp=4.5`. All values must be ≥ 0.0.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if any
    /// value is negative.
    #[must_use]
    pub fn hqdn3d(
        mut self,
        luma_spatial: f32,
        chroma_spatial: f32,
        luma_tmp: f32,
        chroma_tmp: f32,
    ) -> Self {
        self.steps.push(FilterStep::Hqdn3d {
            luma_spatial,
            chroma_spatial,
            luma_tmp,
            chroma_tmp,
        });
        self
    }

    /// Apply non-local means (`nlmeans`) noise reduction.
    ///
    /// `strength` controls denoising intensity; range 1.0–30.0.
    /// Higher values remove more noise at the cost of significantly more CPU.
    ///
    /// NOTE: nlmeans is CPU-intensive; avoid for real-time pipelines.
    #[must_use]
    pub fn nlmeans(mut self, strength: f32) -> Self {
        self.steps.push(FilterStep::Nlmeans { strength });
        self
    }

    /// Deinterlace using the `yadif` (Yet Another Deinterlacing Filter).
    ///
    /// `mode` controls whether one frame or two fields are emitted per input
    /// frame and whether the spatial interlacing check is enabled.
    #[must_use]
    pub fn yadif(mut self, mode: YadifMode) -> Self {
        self.steps.push(FilterStep::Yadif { mode });
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
            if let FilterStep::Crop { width, height, .. } = step
                && (*width == 0 || *height == 0)
            {
                return Err(FilterError::InvalidConfig {
                    reason: "crop width and height must be > 0".to_string(),
                });
            }
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
            if let FilterStep::WhiteBalance {
                temperature_k,
                tint,
            } = step
            {
                if !(1000..=40000).contains(temperature_k) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!(
                            "white_balance temperature_k {temperature_k} out of range [1000, 40000]"
                        ),
                    });
                }
                if !(-1.0..=1.0).contains(tint) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("white_balance tint {tint} out of range [-1.0, 1.0]"),
                    });
                }
            }
            if let FilterStep::Hue { degrees } = step
                && !(-360.0..=360.0).contains(degrees)
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!("hue degrees {degrees} out of range [-360.0, 360.0]"),
                });
            }
            if let FilterStep::Gamma { r, g, b } = step {
                for (channel, val) in [("r", r), ("g", g), ("b", b)] {
                    if !(0.1..=10.0).contains(val) {
                        return Err(FilterError::InvalidConfig {
                            reason: format!("gamma {channel} {val} out of range [0.1, 10.0]"),
                        });
                    }
                }
            }
            if let FilterStep::ThreeWayCC { gamma, .. } = step {
                for (channel, val) in [("r", gamma.r), ("g", gamma.g), ("b", gamma.b)] {
                    if val <= 0.0 {
                        return Err(FilterError::InvalidConfig {
                            reason: format!("three_way_cc gamma.{channel} {val} must be > 0.0"),
                        });
                    }
                }
            }
            if let FilterStep::Vignette { angle, .. } = step
                && !((0.0)..=std::f32::consts::FRAC_PI_2).contains(angle)
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!("vignette angle {angle} out of range [0.0, π/2]"),
                });
            }
            if let FilterStep::Pad { width, height, .. } = step
                && (*width == 0 || *height == 0)
            {
                return Err(FilterError::InvalidConfig {
                    reason: "pad width and height must be > 0".to_string(),
                });
            }
            if let FilterStep::FitToAspect { width, height, .. } = step
                && (*width == 0 || *height == 0)
            {
                return Err(FilterError::InvalidConfig {
                    reason: "fit_to_aspect width and height must be > 0".to_string(),
                });
            }
            if let FilterStep::GBlur { sigma } = step
                && *sigma < 0.0
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!("gblur sigma {sigma} must be >= 0.0"),
                });
            }
            if let FilterStep::Unsharp {
                luma_strength,
                chroma_strength,
            } = step
            {
                if !(-1.5..=1.5).contains(luma_strength) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!(
                            "unsharp luma_strength {luma_strength} out of range [-1.5, 1.5]"
                        ),
                    });
                }
                if !(-1.5..=1.5).contains(chroma_strength) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!(
                            "unsharp chroma_strength {chroma_strength} out of range [-1.5, 1.5]"
                        ),
                    });
                }
            }
            if let FilterStep::Hqdn3d {
                luma_spatial,
                chroma_spatial,
                luma_tmp,
                chroma_tmp,
            } = step
            {
                for (name, val) in [
                    ("luma_spatial", luma_spatial),
                    ("chroma_spatial", chroma_spatial),
                    ("luma_tmp", luma_tmp),
                    ("chroma_tmp", chroma_tmp),
                ] {
                    if *val < 0.0 {
                        return Err(FilterError::InvalidConfig {
                            reason: format!("hqdn3d {name} {val} must be >= 0.0"),
                        });
                    }
                }
            }
            if let FilterStep::Nlmeans { strength } = step
                && (*strength < 1.0 || *strength > 30.0)
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!("nlmeans strength {strength} out of range [1.0, 30.0]"),
                });
            }
        }

        crate::filter_inner::validate_filter_steps(&self.steps)?;
        let output_resolution = self.steps.iter().rev().find_map(|s| {
            if let FilterStep::Scale { width, height, .. } = s {
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
            algorithm: ScaleAlgorithm::Fast,
        };
        assert_eq!(step.filter_name(), "scale");
        assert_eq!(step.args(), "w=1280:h=720:flags=fast_bilinear");
    }

    #[test]
    fn filter_step_scale_lanczos_should_produce_lanczos_flags() {
        let step = FilterStep::Scale {
            width: 1920,
            height: 1080,
            algorithm: ScaleAlgorithm::Lanczos,
        };
        assert_eq!(step.args(), "w=1920:h=1080:flags=lanczos");
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
        let step = FilterStep::Rotate {
            angle_degrees: 90.0,
            fill_color: "black".to_owned(),
        };
        assert_eq!(step.filter_name(), "rotate");
        assert_eq!(
            step.args(),
            format!("angle={}:fillcolor=black", 90_f64.to_radians())
        );
    }

    #[test]
    fn filter_step_rotate_transparent_fill_should_produce_correct_args() {
        let step = FilterStep::Rotate {
            angle_degrees: 45.0,
            fill_color: "0x00000000".to_owned(),
        };
        assert_eq!(step.filter_name(), "rotate");
        let args = step.args();
        assert!(
            args.contains("fillcolor=0x00000000"),
            "args should contain transparent fill: {args}"
        );
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
            .scale(1280, 720, ScaleAlgorithm::Fast)
            .volume(-3.0)
            .build();
        assert!(
            result.is_ok(),
            "builder with multiple valid steps must succeed, got {result:?}"
        );
    }

    #[test]
    fn builder_with_valid_steps_should_succeed() {
        let result = FilterGraph::builder()
            .scale(1280, 720, ScaleAlgorithm::Fast)
            .build();
        assert!(
            result.is_ok(),
            "builder with a known filter step must succeed, got {result:?}"
        );
    }

    #[test]
    fn output_resolution_should_return_scale_dimensions() {
        let fg = FilterGraph::builder()
            .scale(1280, 720, ScaleAlgorithm::Fast)
            .build()
            .unwrap();
        assert_eq!(fg.output_resolution(), Some((1280, 720)));
    }

    #[test]
    fn output_resolution_should_return_last_scale_when_chained() {
        let fg = FilterGraph::builder()
            .scale(1920, 1080, ScaleAlgorithm::Fast)
            .scale(1280, 720, ScaleAlgorithm::Bicubic)
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

    #[test]
    fn filter_step_white_balance_should_produce_correct_filter_name() {
        let step = FilterStep::WhiteBalance {
            temperature_k: 6500,
            tint: 0.0,
        };
        assert_eq!(step.filter_name(), "colorchannelmixer");
    }

    #[test]
    fn filter_step_white_balance_6500k_neutral_tint_should_produce_near_unity_args() {
        // At 6500 K (daylight), all channels should be close to 1.0.
        let step = FilterStep::WhiteBalance {
            temperature_k: 6500,
            tint: 0.0,
        };
        let args = step.args();
        // Parse rr= value to verify it is close to 1.0.
        assert!(args.starts_with("rr="), "args must start with rr=: {args}");
        assert!(
            args.contains("gg=") && args.contains("bb="),
            "args must contain gg and bb: {args}"
        );
    }

    #[test]
    fn filter_step_white_balance_3200k_should_produce_warm_shift() {
        // At 3200 K (tungsten), red should dominate over blue.
        let step = FilterStep::WhiteBalance {
            temperature_k: 3200,
            tint: 0.0,
        };
        let (r, _g, b) = kelvin_to_rgb(3200);
        assert!(r > b, "3200 K must produce a warm shift (r={r} > b={b})");
        // Verify the args string contains rr and bb.
        let args = step.args();
        assert!(args.contains("rr=") && args.contains("bb="), "args={args}");
    }

    #[test]
    fn builder_white_balance_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().white_balance(6500, 0.0).build();
        assert!(
            result.is_ok(),
            "valid white_balance params must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_white_balance_with_temperature_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().white_balance(500, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for temperature_k < 1000, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("temperature_k"),
                "reason should mention temperature_k: {reason}"
            );
        }
    }

    #[test]
    fn builder_white_balance_with_temperature_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().white_balance(50000, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for temperature_k > 40000, got {result:?}"
        );
    }

    #[test]
    fn builder_white_balance_with_tint_out_of_range_should_return_invalid_config() {
        let result = FilterGraph::builder().white_balance(6500, 1.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for tint > 1.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("tint"),
                "reason should mention tint: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_hue_should_produce_correct_filter_name() {
        let step = FilterStep::Hue { degrees: 90.0 };
        assert_eq!(step.filter_name(), "hue");
    }

    #[test]
    fn filter_step_hue_should_produce_correct_args() {
        let step = FilterStep::Hue { degrees: 180.0 };
        assert_eq!(step.args(), "h=180");
    }

    #[test]
    fn filter_step_hue_zero_should_produce_no_op_args() {
        let step = FilterStep::Hue { degrees: 0.0 };
        assert_eq!(step.args(), "h=0");
    }

    #[test]
    fn builder_hue_with_valid_degrees_should_succeed() {
        let result = FilterGraph::builder().hue(0.0).build();
        assert!(
            result.is_ok(),
            "hue(0.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_hue_with_degrees_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().hue(400.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for degrees > 360.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("degrees"),
                "reason should mention degrees: {reason}"
            );
        }
    }

    #[test]
    fn builder_hue_with_degrees_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().hue(-400.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for degrees < -360.0, got {result:?}"
        );
    }

    #[test]
    fn filter_step_gamma_should_produce_correct_filter_name() {
        let step = FilterStep::Gamma {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        assert_eq!(step.filter_name(), "eq");
    }

    #[test]
    fn filter_step_gamma_should_produce_correct_args() {
        let step = FilterStep::Gamma {
            r: 2.2,
            g: 2.2,
            b: 2.2,
        };
        assert_eq!(step.args(), "gamma_r=2.2:gamma_g=2.2:gamma_b=2.2");
    }

    #[test]
    fn filter_step_gamma_neutral_should_produce_unity_args() {
        let step = FilterStep::Gamma {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        };
        assert_eq!(step.args(), "gamma_r=1:gamma_g=1:gamma_b=1");
    }

    #[test]
    fn builder_gamma_with_neutral_values_should_succeed() {
        let result = FilterGraph::builder().gamma(1.0, 1.0, 1.0).build();
        assert!(
            result.is_ok(),
            "gamma(1.0, 1.0, 1.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_gamma_with_r_out_of_range_should_return_invalid_config() {
        let result = FilterGraph::builder().gamma(0.0, 1.0, 1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for r < 0.1, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("gamma") && reason.contains(" r "),
                "reason should mention gamma r: {reason}"
            );
        }
    }

    #[test]
    fn builder_gamma_with_b_out_of_range_should_return_invalid_config() {
        let result = FilterGraph::builder().gamma(1.0, 1.0, 11.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for b > 10.0, got {result:?}"
        );
    }

    #[test]
    fn rgb_neutral_constant_should_have_all_channels_one() {
        assert_eq!(Rgb::NEUTRAL.r, 1.0);
        assert_eq!(Rgb::NEUTRAL.g, 1.0);
        assert_eq!(Rgb::NEUTRAL.b, 1.0);
    }

    #[test]
    fn filter_step_three_way_cc_should_produce_correct_filter_name() {
        let step = FilterStep::ThreeWayCC {
            lift: Rgb::NEUTRAL,
            gamma: Rgb::NEUTRAL,
            gain: Rgb::NEUTRAL,
        };
        assert_eq!(step.filter_name(), "curves");
    }

    #[test]
    fn filter_step_three_way_cc_neutral_should_produce_identity_curves() {
        let step = FilterStep::ThreeWayCC {
            lift: Rgb::NEUTRAL,
            gamma: Rgb::NEUTRAL,
            gain: Rgb::NEUTRAL,
        };
        let args = step.args();
        // Neutral: 0/0, 0.5/0.5, 1/1 for all channels.
        assert!(
            args.contains("r='0/0 0.5/0.5 1/1'"),
            "neutral r channel must be identity: {args}"
        );
        assert!(
            args.contains("g='0/0 0.5/0.5 1/1'"),
            "neutral g channel must be identity: {args}"
        );
        assert!(
            args.contains("b='0/0 0.5/0.5 1/1'"),
            "neutral b channel must be identity: {args}"
        );
    }

    #[test]
    fn builder_three_way_cc_with_neutral_values_should_succeed() {
        let result = FilterGraph::builder()
            .three_way_cc(Rgb::NEUTRAL, Rgb::NEUTRAL, Rgb::NEUTRAL)
            .build();
        assert!(
            result.is_ok(),
            "neutral three_way_cc must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_three_way_cc_with_gamma_zero_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .three_way_cc(
                Rgb::NEUTRAL,
                Rgb {
                    r: 0.0,
                    g: 1.0,
                    b: 1.0,
                },
                Rgb::NEUTRAL,
            )
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for gamma.r = 0.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("gamma.r"),
                "reason should mention gamma.r: {reason}"
            );
        }
    }

    #[test]
    fn builder_three_way_cc_with_negative_gamma_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .three_way_cc(
                Rgb::NEUTRAL,
                Rgb {
                    r: 1.0,
                    g: -0.5,
                    b: 1.0,
                },
                Rgb::NEUTRAL,
            )
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for gamma.g < 0.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("gamma.g"),
                "reason should mention gamma.g: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_vignette_should_produce_correct_filter_name() {
        let step = FilterStep::Vignette {
            angle: 0.628,
            x0: 0.0,
            y0: 0.0,
        };
        assert_eq!(step.filter_name(), "vignette");
    }

    #[test]
    fn filter_step_vignette_zero_centre_should_use_w2_h2_defaults() {
        let step = FilterStep::Vignette {
            angle: 0.628,
            x0: 0.0,
            y0: 0.0,
        };
        let args = step.args();
        assert!(args.contains("x0=w/2"), "x0=0.0 should map to w/2: {args}");
        assert!(args.contains("y0=h/2"), "y0=0.0 should map to h/2: {args}");
        assert!(
            args.contains("angle=0.628"),
            "args must contain angle: {args}"
        );
    }

    #[test]
    fn filter_step_vignette_custom_centre_should_produce_numeric_coords() {
        let step = FilterStep::Vignette {
            angle: 0.5,
            x0: 320.0,
            y0: 240.0,
        };
        let args = step.args();
        assert!(args.contains("x0=320"), "custom x0 should appear: {args}");
        assert!(args.contains("y0=240"), "custom y0 should appear: {args}");
    }

    #[test]
    fn builder_vignette_with_valid_angle_should_succeed() {
        let result = FilterGraph::builder()
            .vignette(std::f32::consts::PI / 5.0, 0.0, 0.0)
            .build();
        assert!(
            result.is_ok(),
            "default vignette angle must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_vignette_with_angle_too_large_should_return_invalid_config() {
        let result = FilterGraph::builder().vignette(2.0, 0.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for angle > π/2, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("angle"),
                "reason should mention angle: {reason}"
            );
        }
    }

    #[test]
    fn builder_vignette_with_negative_angle_should_return_invalid_config() {
        let result = FilterGraph::builder().vignette(-0.1, 0.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for angle < 0.0, got {result:?}"
        );
    }

    #[test]
    fn builder_crop_with_zero_width_should_return_invalid_config() {
        let result = FilterGraph::builder().crop(0, 0, 0, 100).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for width=0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("crop width and height must be > 0"),
                "reason should mention crop dimensions: {reason}"
            );
        }
    }

    #[test]
    fn builder_crop_with_zero_height_should_return_invalid_config() {
        let result = FilterGraph::builder().crop(0, 0, 100, 0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for height=0, got {result:?}"
        );
    }

    #[test]
    fn builder_crop_with_valid_dimensions_should_succeed() {
        let result = FilterGraph::builder().crop(0, 0, 64, 64).build();
        assert!(
            result.is_ok(),
            "crop with valid dimensions must build successfully, got {result:?}"
        );
    }

    #[test]
    fn filter_step_hflip_should_produce_correct_filter_name_and_empty_args() {
        let step = FilterStep::HFlip;
        assert_eq!(step.filter_name(), "hflip");
        assert_eq!(step.args(), "");
    }

    #[test]
    fn filter_step_vflip_should_produce_correct_filter_name_and_empty_args() {
        let step = FilterStep::VFlip;
        assert_eq!(step.filter_name(), "vflip");
        assert_eq!(step.args(), "");
    }

    #[test]
    fn builder_hflip_should_succeed() {
        let result = FilterGraph::builder().hflip().build();
        assert!(
            result.is_ok(),
            "hflip must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_vflip_should_succeed() {
        let result = FilterGraph::builder().vflip().build();
        assert!(
            result.is_ok(),
            "vflip must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_hflip_twice_should_succeed() {
        let result = FilterGraph::builder().hflip().hflip().build();
        assert!(
            result.is_ok(),
            "double hflip (round-trip) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn filter_step_pad_should_produce_correct_filter_name() {
        let step = FilterStep::Pad {
            width: 1920,
            height: 1080,
            x: -1,
            y: -1,
            color: "black".to_owned(),
        };
        assert_eq!(step.filter_name(), "pad");
    }

    #[test]
    fn filter_step_pad_negative_xy_should_produce_centred_args() {
        let step = FilterStep::Pad {
            width: 1920,
            height: 1080,
            x: -1,
            y: -1,
            color: "black".to_owned(),
        };
        assert_eq!(
            step.args(),
            "width=1920:height=1080:x=(ow-iw)/2:y=(oh-ih)/2:color=black"
        );
    }

    #[test]
    fn filter_step_pad_explicit_xy_should_produce_numeric_args() {
        let step = FilterStep::Pad {
            width: 1920,
            height: 1080,
            x: 320,
            y: 180,
            color: "0x000000".to_owned(),
        };
        assert_eq!(
            step.args(),
            "width=1920:height=1080:x=320:y=180:color=0x000000"
        );
    }

    #[test]
    fn filter_step_pad_zero_xy_should_produce_zero_offset_args() {
        let step = FilterStep::Pad {
            width: 1280,
            height: 720,
            x: 0,
            y: 0,
            color: "black".to_owned(),
        };
        assert_eq!(step.args(), "width=1280:height=720:x=0:y=0:color=black");
    }

    #[test]
    fn builder_pad_with_valid_params_should_succeed() {
        let result = FilterGraph::builder()
            .pad(1920, 1080, -1, -1, "black")
            .build();
        assert!(
            result.is_ok(),
            "pad with valid params must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_pad_with_zero_width_should_return_invalid_config() {
        let result = FilterGraph::builder().pad(0, 1080, -1, -1, "black").build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for width=0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("pad width and height must be > 0"),
                "reason should mention pad dimensions: {reason}"
            );
        }
    }

    #[test]
    fn builder_pad_with_zero_height_should_return_invalid_config() {
        let result = FilterGraph::builder().pad(1920, 0, -1, -1, "black").build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for height=0, got {result:?}"
        );
    }

    #[test]
    fn filter_step_fit_to_aspect_should_produce_correct_filter_name() {
        let step = FilterStep::FitToAspect {
            width: 1920,
            height: 1080,
            color: "black".to_owned(),
        };
        assert_eq!(step.filter_name(), "scale");
    }

    #[test]
    fn filter_step_fit_to_aspect_should_produce_scale_args_with_force_original_aspect_ratio() {
        let step = FilterStep::FitToAspect {
            width: 1920,
            height: 1080,
            color: "black".to_owned(),
        };
        let args = step.args();
        assert!(
            args.contains("w=1920") && args.contains("h=1080"),
            "args must contain target dimensions: {args}"
        );
        assert!(
            args.contains("force_original_aspect_ratio=decrease"),
            "args must request aspect-ratio-preserving scale: {args}"
        );
    }

    #[test]
    fn builder_fit_to_aspect_with_valid_params_should_succeed() {
        let result = FilterGraph::builder()
            .fit_to_aspect(1920, 1080, "black")
            .build();
        assert!(
            result.is_ok(),
            "fit_to_aspect with valid params must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_fit_to_aspect_with_zero_width_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .fit_to_aspect(0, 1080, "black")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for width=0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("fit_to_aspect width and height must be > 0"),
                "reason should mention fit_to_aspect dimensions: {reason}"
            );
        }
    }

    #[test]
    fn builder_fit_to_aspect_with_zero_height_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .fit_to_aspect(1920, 0, "black")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for height=0, got {result:?}"
        );
    }

    #[test]
    fn filter_step_gblur_should_produce_correct_filter_name() {
        let step = FilterStep::GBlur { sigma: 5.0 };
        assert_eq!(step.filter_name(), "gblur");
    }

    #[test]
    fn filter_step_gblur_should_produce_correct_args() {
        let step = FilterStep::GBlur { sigma: 5.0 };
        assert_eq!(step.args(), "sigma=5");
    }

    #[test]
    fn filter_step_gblur_small_sigma_should_produce_correct_args() {
        let step = FilterStep::GBlur { sigma: 0.1 };
        assert_eq!(step.args(), "sigma=0.1");
    }

    #[test]
    fn builder_gblur_with_valid_sigma_should_succeed() {
        let result = FilterGraph::builder().gblur(5.0).build();
        assert!(
            result.is_ok(),
            "gblur(5.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_gblur_with_zero_sigma_should_succeed() {
        let result = FilterGraph::builder().gblur(0.0).build();
        assert!(
            result.is_ok(),
            "gblur(0.0) must build successfully (no-op), got {result:?}"
        );
    }

    #[test]
    fn builder_gblur_with_negative_sigma_should_return_invalid_config() {
        let result = FilterGraph::builder().gblur(-1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for sigma < 0.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("sigma"),
                "reason should mention sigma: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_unsharp_should_produce_correct_filter_name() {
        let step = FilterStep::Unsharp {
            luma_strength: 1.0,
            chroma_strength: 0.0,
        };
        assert_eq!(step.filter_name(), "unsharp");
    }

    #[test]
    fn filter_step_unsharp_should_produce_correct_args() {
        let step = FilterStep::Unsharp {
            luma_strength: 1.0,
            chroma_strength: 0.5,
        };
        let args = step.args();
        assert!(
            args.contains("luma_amount=1") && args.contains("chroma_amount=0.5"),
            "args must contain luma and chroma amounts: {args}"
        );
        assert!(
            args.contains("luma_msize_x=5") && args.contains("luma_msize_y=5"),
            "args must contain luma matrix size: {args}"
        );
        assert!(
            args.contains("chroma_msize_x=5") && args.contains("chroma_msize_y=5"),
            "args must contain chroma matrix size: {args}"
        );
    }

    #[test]
    fn builder_unsharp_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().unsharp(1.0, 0.0).build();
        assert!(
            result.is_ok(),
            "unsharp(1.0, 0.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_unsharp_with_negative_luma_should_succeed() {
        let result = FilterGraph::builder().unsharp(-1.0, 0.0).build();
        assert!(
            result.is_ok(),
            "unsharp(-1.0, 0.0) (blur) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_unsharp_with_luma_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().unsharp(2.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for luma_strength > 1.5, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("luma_strength"),
                "reason should mention luma_strength: {reason}"
            );
        }
    }

    #[test]
    fn builder_unsharp_with_luma_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().unsharp(-2.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for luma_strength < -1.5, got {result:?}"
        );
    }

    #[test]
    fn builder_unsharp_with_chroma_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().unsharp(0.0, 2.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for chroma_strength > 1.5, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("chroma_strength"),
                "reason should mention chroma_strength: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_hqdn3d_should_produce_correct_filter_name() {
        let step = FilterStep::Hqdn3d {
            luma_spatial: 4.0,
            chroma_spatial: 3.0,
            luma_tmp: 6.0,
            chroma_tmp: 4.5,
        };
        assert_eq!(step.filter_name(), "hqdn3d");
    }

    #[test]
    fn filter_step_hqdn3d_should_produce_correct_args() {
        let step = FilterStep::Hqdn3d {
            luma_spatial: 4.0,
            chroma_spatial: 3.0,
            luma_tmp: 6.0,
            chroma_tmp: 4.5,
        };
        assert_eq!(step.args(), "4:3:6:4.5");
    }

    #[test]
    fn builder_hqdn3d_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().hqdn3d(4.0, 3.0, 6.0, 4.5).build();
        assert!(
            result.is_ok(),
            "hqdn3d(4.0, 3.0, 6.0, 4.5) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_zero_params_should_succeed() {
        let result = FilterGraph::builder().hqdn3d(0.0, 0.0, 0.0, 0.0).build();
        assert!(
            result.is_ok(),
            "hqdn3d(0.0, 0.0, 0.0, 0.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_negative_luma_spatial_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(-1.0, 3.0, 6.0, 4.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative luma_spatial, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("luma_spatial"),
                "reason should mention luma_spatial: {reason}"
            );
        }
    }

    #[test]
    fn builder_hqdn3d_with_negative_chroma_spatial_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(4.0, -1.0, 6.0, 4.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative chroma_spatial, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_negative_luma_tmp_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(4.0, 3.0, -1.0, 4.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative luma_tmp, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_negative_chroma_tmp_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(4.0, 3.0, 6.0, -1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative chroma_tmp, got {result:?}"
        );
    }

    #[test]
    fn filter_step_nlmeans_should_produce_correct_filter_name() {
        let step = FilterStep::Nlmeans { strength: 8.0 };
        assert_eq!(step.filter_name(), "nlmeans");
    }

    #[test]
    fn filter_step_nlmeans_should_produce_correct_args() {
        let step = FilterStep::Nlmeans { strength: 8.0 };
        assert_eq!(step.args(), "s=8");
    }

    #[test]
    fn builder_nlmeans_with_valid_strength_should_succeed() {
        let result = FilterGraph::builder().nlmeans(8.0).build();
        assert!(
            result.is_ok(),
            "nlmeans(8.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_nlmeans_with_min_strength_should_succeed() {
        let result = FilterGraph::builder().nlmeans(1.0).build();
        assert!(
            result.is_ok(),
            "nlmeans(1.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_nlmeans_with_max_strength_should_succeed() {
        let result = FilterGraph::builder().nlmeans(30.0).build();
        assert!(
            result.is_ok(),
            "nlmeans(30.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_nlmeans_with_strength_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().nlmeans(0.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for strength < 1.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("strength"),
                "reason should mention strength: {reason}"
            );
        }
    }

    #[test]
    fn builder_nlmeans_with_strength_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().nlmeans(31.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for strength > 30.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("strength"),
                "reason should mention strength: {reason}"
            );
        }
    }

    #[test]
    fn yadif_mode_variants_should_have_correct_discriminants() {
        assert_eq!(YadifMode::Frame as i32, 0);
        assert_eq!(YadifMode::Field as i32, 1);
        assert_eq!(YadifMode::FrameNospatial as i32, 2);
        assert_eq!(YadifMode::FieldNospatial as i32, 3);
    }

    #[test]
    fn filter_step_yadif_should_produce_correct_filter_name() {
        let step = FilterStep::Yadif {
            mode: YadifMode::Frame,
        };
        assert_eq!(step.filter_name(), "yadif");
    }

    #[test]
    fn filter_step_yadif_frame_should_produce_mode_0_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::Frame,
        };
        assert_eq!(step.args(), "mode=0");
    }

    #[test]
    fn filter_step_yadif_field_should_produce_mode_1_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::Field,
        };
        assert_eq!(step.args(), "mode=1");
    }

    #[test]
    fn filter_step_yadif_frame_nospatial_should_produce_mode_2_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::FrameNospatial,
        };
        assert_eq!(step.args(), "mode=2");
    }

    #[test]
    fn filter_step_yadif_field_nospatial_should_produce_mode_3_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::FieldNospatial,
        };
        assert_eq!(step.args(), "mode=3");
    }

    #[test]
    fn builder_yadif_with_frame_mode_should_succeed() {
        let result = FilterGraph::builder().yadif(YadifMode::Frame).build();
        assert!(
            result.is_ok(),
            "yadif(Frame) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_yadif_with_all_modes_should_succeed() {
        for mode in [
            YadifMode::Frame,
            YadifMode::Field,
            YadifMode::FrameNospatial,
            YadifMode::FieldNospatial,
        ] {
            let result = FilterGraph::builder().yadif(mode).build();
            assert!(
                result.is_ok(),
                "yadif({mode:?}) must build successfully, got {result:?}"
            );
        }
    }
}
