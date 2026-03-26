//! Supporting types for [`super::FilterGraphBuilder`] and [`super::FilterGraph`].

// ── Supporting enums ──────────────────────────────────────────────────────────

/// Tone-mapping algorithm for HDR-to-SDR conversion.
///
/// Used with [`super::FilterGraphBuilder::tone_map`].
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
/// See [`super::FilterGraphBuilder::three_way_cc`].
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
/// Used with [`super::FilterGraphBuilder::scale`].
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
/// Used with [`super::FilterGraphBuilder::yadif`].
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

/// Transition type for the `xfade` cross-dissolve filter.
///
/// Used with [`super::FilterGraphBuilder::xfade`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfadeTransition {
    /// Blend frames (cross-dissolve).
    Dissolve,
    /// Fade through black.
    Fade,
    /// Wipe from right to left.
    WipeLeft,
    /// Wipe from left to right.
    WipeRight,
    /// Wipe upward.
    WipeUp,
    /// Wipe downward.
    WipeDown,
    /// Slide from right.
    SlideLeft,
    /// Slide from left.
    SlideRight,
    /// Slide upward.
    SlideUp,
    /// Slide downward.
    SlideDown,
    /// Circular iris open.
    CircleOpen,
    /// Circular iris close.
    CircleClose,
    /// Fade through gray.
    FadeGrays,
    /// Pixelize transition.
    Pixelize,
}

impl XfadeTransition {
    /// Returns the `FFmpeg` `xfade` transition name string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dissolve => "dissolve",
            Self::Fade => "fade",
            Self::WipeLeft => "wipeleft",
            Self::WipeRight => "wiperight",
            Self::WipeUp => "wipeup",
            Self::WipeDown => "wipedown",
            Self::SlideLeft => "slideleft",
            Self::SlideRight => "slideright",
            Self::SlideUp => "slideup",
            Self::SlideDown => "slidedown",
            Self::CircleOpen => "circleopen",
            Self::CircleClose => "circleclose",
            Self::FadeGrays => "fadegrays",
            Self::Pixelize => "pixelize",
        }
    }
}

/// Options for the `drawtext` filter.
///
/// Used with [`super::FilterGraphBuilder::drawtext`].
#[derive(Debug, Clone)]
pub struct DrawTextOptions {
    /// Text string (UTF-8). Special characters (`:`, `'`, `\`) are escaped automatically.
    pub text: String,
    /// X position as an `FFmpeg` expression string, e.g. `"(w-text_w)/2"` or `"10"`.
    pub x: String,
    /// Y position as an `FFmpeg` expression string, e.g. `"h-th-10"` or `"10"`.
    pub y: String,
    /// Font size in points.
    pub font_size: u32,
    /// Font color as an `FFmpeg` color string, e.g. `"white"` or `"0xFFFFFF"`.
    pub font_color: String,
    /// Optional path to a TrueType font file. Uses default font when `None`.
    pub font_file: Option<String>,
    /// Opacity 0.0 (transparent) to 1.0 (opaque), applied as an alpha channel on `fontcolor`.
    pub opacity: f32,
    /// Optional background box fill color, e.g. `"black@0.5"`. No box when `None`.
    pub box_color: Option<String>,
    /// Background box border width in pixels. Ignored when `box_color` is `None`.
    pub box_border_width: u32,
}
