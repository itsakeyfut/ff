//! [`FilterGraphBuilder`] — consuming builder for filter graphs.

use std::path::Path;

use super::FilterGraph;
use super::filter_step::FilterStep;
use super::types::{
    DrawTextOptions, HwAccel, Rgb, ScaleAlgorithm, ToneMap, XfadeTransition, YadifMode,
};
use crate::error::FilterError;
use crate::filter_inner::FilterGraphInner;

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

    /// Fade in from black, starting at `start_sec` seconds and reaching full
    /// brightness after `duration_sec` seconds.
    #[must_use]
    pub fn fade_in(mut self, start_sec: f64, duration_sec: f64) -> Self {
        self.steps.push(FilterStep::FadeIn {
            start: start_sec,
            duration: duration_sec,
        });
        self
    }

    /// Fade out to black, starting at `start_sec` seconds and reaching full
    /// black after `duration_sec` seconds.
    #[must_use]
    pub fn fade_out(mut self, start_sec: f64, duration_sec: f64) -> Self {
        self.steps.push(FilterStep::FadeOut {
            start: start_sec,
            duration: duration_sec,
        });
        self
    }

    /// Fade in from white, starting at `start_sec` seconds and reaching full
    /// brightness after `duration_sec` seconds.
    #[must_use]
    pub fn fade_in_white(mut self, start_sec: f64, duration_sec: f64) -> Self {
        self.steps.push(FilterStep::FadeInWhite {
            start: start_sec,
            duration: duration_sec,
        });
        self
    }

    /// Fade out to white, starting at `start_sec` seconds and reaching full
    /// white after `duration_sec` seconds.
    #[must_use]
    pub fn fade_out_white(mut self, start_sec: f64, duration_sec: f64) -> Self {
        self.steps.push(FilterStep::FadeOutWhite {
            start: start_sec,
            duration: duration_sec,
        });
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

    /// Apply a cross-dissolve transition between two video streams using `xfade`.
    ///
    /// Requires two input slots: slot 0 is clip A (first clip), slot 1 is clip B
    /// (second clip). Call [`FilterGraph::push_video`] with slot 0 for clip A
    /// frames and slot 1 for clip B frames.
    ///
    /// - `transition`: the visual transition style.
    /// - `duration`: length of the overlap in seconds. Must be > 0.0.
    /// - `offset`: PTS offset (seconds) at which clip B starts playing.
    #[must_use]
    pub fn xfade(mut self, transition: XfadeTransition, duration: f64, offset: f64) -> Self {
        self.steps.push(FilterStep::XFade {
            transition,
            duration,
            offset,
        });
        self
    }

    /// Overlay text onto the video using the `drawtext` filter.
    ///
    /// See [`DrawTextOptions`] for all configurable fields including position,
    /// font, size, color, opacity, and optional background box.
    #[must_use]
    pub fn drawtext(mut self, opts: DrawTextOptions) -> Self {
        self.steps.push(FilterStep::DrawText { opts });
        self
    }

    /// Burn SRT subtitles into the video (hard subtitles).
    ///
    /// Subtitles are read from the `.srt` file at `srt_path` and rendered
    /// at the timecodes defined in the file using `FFmpeg`'s `subtitles` filter.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if:
    /// - the extension is not `.srt`, or
    /// - the file does not exist at build time.
    #[must_use]
    pub fn subtitles_srt(mut self, srt_path: &str) -> Self {
        self.steps.push(FilterStep::SubtitlesSrt {
            path: srt_path.to_owned(),
        });
        self
    }

    /// Burn ASS/SSA styled subtitles into the video (hard subtitles).
    ///
    /// Subtitles are read from the `.ass` or `.ssa` file at `ass_path` and
    /// rendered with full styling using `FFmpeg`'s dedicated `ass` filter,
    /// which preserves fonts, colours, and positioning better than the generic
    /// `subtitles` filter.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if:
    /// - the extension is not `.ass` or `.ssa`, or
    /// - the file does not exist at build time.
    #[must_use]
    pub fn subtitles_ass(mut self, ass_path: &str) -> Self {
        self.steps.push(FilterStep::SubtitlesAss {
            path: ass_path.to_owned(),
        });
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
            if let FilterStep::FadeIn { duration, .. }
            | FilterStep::FadeOut { duration, .. }
            | FilterStep::FadeInWhite { duration, .. }
            | FilterStep::FadeOutWhite { duration, .. } = step
                && *duration <= 0.0
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!("fade duration {duration} must be > 0.0"),
                });
            }
            if let FilterStep::XFade { duration, .. } = step
                && *duration <= 0.0
            {
                return Err(FilterError::InvalidConfig {
                    reason: format!("xfade duration {duration} must be > 0.0"),
                });
            }
            if let FilterStep::DrawText { opts } = step {
                if opts.text.is_empty() {
                    return Err(FilterError::InvalidConfig {
                        reason: "drawtext text must not be empty".to_string(),
                    });
                }
                if !(0.0..=1.0).contains(&opts.opacity) {
                    return Err(FilterError::InvalidConfig {
                        reason: format!(
                            "drawtext opacity {} out of range [0.0, 1.0]",
                            opts.opacity
                        ),
                    });
                }
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
            if let FilterStep::SubtitlesSrt { path } = step {
                let ext = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if ext != "srt" {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("unsupported subtitle format: .{ext}; expected .srt"),
                    });
                }
                if !Path::new(path).exists() {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("subtitle file not found: {path}"),
                    });
                }
            }
            if let FilterStep::SubtitlesAss { path } = step {
                let ext = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if !matches!(ext, "ass" | "ssa") {
                    return Err(FilterError::InvalidConfig {
                        reason: format!(
                            "unsupported subtitle format: .{ext}; expected .ass or .ssa"
                        ),
                    });
                }
                if !Path::new(path).exists() {
                    return Err(FilterError::InvalidConfig {
                        reason: format!("subtitle file not found: {path}"),
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

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
