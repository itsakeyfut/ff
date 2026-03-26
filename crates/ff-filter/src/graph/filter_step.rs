//! Internal filter step representation.

use super::types::{DrawTextOptions, Rgb, ScaleAlgorithm, ToneMap, XfadeTransition, YadifMode};

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
    /// Fade-in from black starting at `start` seconds, over `duration` seconds.
    FadeIn { start: f64, duration: f64 },
    /// Fade-out to black starting at `start` seconds, over `duration` seconds.
    FadeOut { start: f64, duration: f64 },
    /// Fade-in from white starting at `start` seconds, over `duration` seconds.
    FadeInWhite { start: f64, duration: f64 },
    /// Fade-out to white starting at `start` seconds, over `duration` seconds.
    FadeOutWhite { start: f64, duration: f64 },
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
    /// Cross-dissolve transition between two video streams (`xfade`).
    ///
    /// Requires two input slots: slot 0 is clip A, slot 1 is clip B.
    /// `duration` is the overlap length in seconds; `offset` is the PTS
    /// offset (in seconds) at which clip B begins.
    XFade {
        /// Transition style.
        transition: XfadeTransition,
        /// Overlap duration in seconds. Must be > 0.0.
        duration: f64,
        /// PTS offset (seconds) where clip B starts.
        offset: f64,
    },
    /// Draw text onto the video using the `drawtext` filter.
    DrawText {
        /// Full set of drawtext parameters.
        opts: DrawTextOptions,
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
            Self::FadeIn { .. }
            | Self::FadeOut { .. }
            | Self::FadeInWhite { .. }
            | Self::FadeOutWhite { .. } => "fade",
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
            Self::XFade { .. } => "xfade",
            Self::DrawText { .. } => "drawtext",
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
            Self::FadeIn { start, duration } => {
                format!("type=in:start_time={start}:duration={duration}")
            }
            Self::FadeOut { start, duration } => {
                format!("type=out:start_time={start}:duration={duration}")
            }
            Self::FadeInWhite { start, duration } => {
                format!("type=in:start_time={start}:duration={duration}:color=white")
            }
            Self::FadeOutWhite { start, duration } => {
                format!("type=out:start_time={start}:duration={duration}:color=white")
            }
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
            Self::XFade {
                transition,
                duration,
                offset,
            } => {
                let t = transition.as_str();
                format!("transition={t}:duration={duration}:offset={offset}")
            }
            Self::DrawText { opts } => {
                // Escape special characters recognised by the drawtext filter.
                let escaped = opts
                    .text
                    .replace('\\', "\\\\")
                    .replace(':', "\\:")
                    .replace('\'', "\\'");
                let mut parts = vec![
                    format!("text='{escaped}'"),
                    format!("x={}", opts.x),
                    format!("y={}", opts.y),
                    format!("fontsize={}", opts.font_size),
                    format!("fontcolor={}@{:.2}", opts.font_color, opts.opacity),
                ];
                if let Some(ref ff) = opts.font_file {
                    parts.push(format!("fontfile={ff}"));
                }
                if let Some(ref bc) = opts.box_color {
                    parts.push("box=1".to_string());
                    parts.push(format!("boxcolor={bc}"));
                    parts.push(format!("boxborderw={}", opts.box_border_width));
                }
                parts.join(":")
            }
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
