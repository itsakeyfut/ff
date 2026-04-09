//! Internal filter step representation.

use super::builder::FilterGraphBuilder;
use super::types::{
    DrawTextOptions, EqBand, Rgb, ScaleAlgorithm, ToneMap, XfadeTransition, YadifMode,
};
use crate::blend::BlendMode;

// ── FilterStep ────────────────────────────────────────────────────────────────

/// A single step in a filter chain.
///
/// Used by [`crate::FilterGraphBuilder`] to build pipeline filter graphs, and by
/// [`crate::AudioTrack::effects`] to attach per-track effects in a multi-track mix.
#[derive(Debug, Clone)]
pub enum FilterStep {
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
    /// Audio fade-in from silence starting at `start` seconds, over `duration` seconds.
    AFadeIn { start: f64, duration: f64 },
    /// Audio fade-out to silence starting at `start` seconds, over `duration` seconds.
    AFadeOut { start: f64, duration: f64 },
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
    /// Multi-band parametric equalizer (low-shelf, high-shelf, or peak bands).
    ///
    /// Each band maps to its own `FFmpeg` filter node chained in sequence.
    /// The `bands` vec must not be empty.
    ParametricEq { bands: Vec<EqBand> },
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
    /// Reverse video playback (buffers entire clip in memory — use only on short clips).
    Reverse,
    /// Reverse audio playback (buffers entire clip in memory — use only on short clips).
    AReverse,
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
    /// Burn-in SRT subtitles (hard subtitles) using the `subtitles` filter.
    SubtitlesSrt {
        /// Absolute or relative path to the `.srt` file.
        path: String,
    },
    /// Burn-in ASS/SSA styled subtitles using the `ass` filter.
    SubtitlesAss {
        /// Absolute or relative path to the `.ass` or `.ssa` file.
        path: String,
    },
    /// Playback speed change using `setpts` (video) and chained `atempo` (audio).
    ///
    /// `factor > 1.0` = fast motion; `factor < 1.0` = slow motion.
    /// Valid range: 0.1–100.0.
    ///
    /// Video path: `setpts=PTS/{factor}`.
    /// Audio path: the `atempo` filter only accepts [0.5, 2.0] per instance;
    /// `filter_inner` chains multiple instances to cover the full range.
    Speed {
        /// Speed multiplier. Must be in [0.1, 100.0].
        factor: f64,
    },
    /// EBU R128 two-pass loudness normalization.
    ///
    /// Pass 1 measures integrated loudness with `ebur128=peak=true:metadata=1`.
    /// Pass 2 applies a linear volume correction so the output reaches `target_lufs`.
    /// All audio frames are buffered in memory between the two passes — use only
    /// for clips that fit comfortably in RAM.
    LoudnessNormalize {
        /// Target integrated loudness in LUFS (e.g. −23.0). Must be < 0.0.
        target_lufs: f32,
        /// True-peak ceiling in dBTP (e.g. −1.0). Must be ≤ 0.0.
        true_peak_db: f32,
        /// Target loudness range in LU (e.g. 7.0). Must be > 0.0.
        lra: f32,
    },
    /// Peak-level two-pass normalization using `astats`.
    ///
    /// Pass 1 measures the true peak with `astats=metadata=1`.
    /// Pass 2 applies `volume={gain}dB` so the output peak reaches `target_db`.
    /// All audio frames are buffered in memory between passes — use only
    /// for clips that fit comfortably in RAM.
    NormalizePeak {
        /// Target peak level in dBFS (e.g. −1.0). Must be ≤ 0.0.
        target_db: f32,
    },
    /// Noise gate via `FFmpeg`'s `agate` filter.
    ///
    /// Audio below `threshold_db` is attenuated; audio above passes through.
    /// The threshold is converted from dBFS to the linear scale expected by
    /// `agate`'s `threshold` parameter (`linear = 10^(dB/20)`).
    ANoiseGate {
        /// Gate open/close threshold in dBFS (e.g. −40.0).
        threshold_db: f32,
        /// Attack time in milliseconds — how quickly the gate opens. Must be > 0.0.
        attack_ms: f32,
        /// Release time in milliseconds — how quickly the gate closes. Must be > 0.0.
        release_ms: f32,
    },
    /// Dynamic range compressor via `FFmpeg`'s `acompressor` filter.
    ///
    /// Reduces the dynamic range of the audio signal: peaks above
    /// `threshold_db` are attenuated by `ratio`:1.  `makeup_db` applies
    /// additional gain after compression to restore perceived loudness.
    ACompressor {
        /// Compression threshold in dBFS (e.g. −20.0).
        threshold_db: f32,
        /// Compression ratio (e.g. 4.0 = 4:1). Must be ≥ 1.0.
        ratio: f32,
        /// Attack time in milliseconds. Must be > 0.0.
        attack_ms: f32,
        /// Release time in milliseconds. Must be > 0.0.
        release_ms: f32,
        /// Make-up gain in dB applied after compression (e.g. 6.0).
        makeup_db: f32,
    },
    /// Downmix stereo to mono via `FFmpeg`'s `pan` filter.
    ///
    /// Both channels are mixed with equal weight:
    /// `mono|c0=0.5*c0+0.5*c1`.  The output has a single channel.
    StereoToMono,
    /// Remap audio channels using `FFmpeg`'s `channelmap` filter.
    ///
    /// `mapping` is a `|`-separated list of output channel names taken
    /// from input channels, e.g. `"FR|FL"` swaps left and right.
    /// Must not be empty.
    ChannelMap {
        /// `FFmpeg` channelmap mapping expression (e.g. `"FR|FL"`).
        mapping: String,
    },
    /// A/V sync correction via audio delay or advance.
    ///
    /// Positive `ms`: uses `FFmpeg`'s `adelay` filter to shift audio later.
    /// Negative `ms`: uses `FFmpeg`'s `atrim` filter to trim the audio start,
    /// effectively advancing audio by `|ms|` milliseconds.
    /// Zero `ms`: uses `adelay` with zero delay (no-op).
    AudioDelay {
        /// Delay in milliseconds. Positive = delay; negative = advance.
        ms: f64,
    },
    /// Concatenate `n` sequential video input segments via `FFmpeg`'s `concat` filter.
    ///
    /// Requires `n` video input slots (0 through `n-1`). `n` must be ≥ 2.
    ConcatVideo {
        /// Number of video input segments to concatenate. Must be ≥ 2.
        n: u32,
    },
    /// Concatenate `n` sequential audio input segments via `FFmpeg`'s `concat` filter.
    ///
    /// Requires `n` audio input slots (0 through `n-1`). `n` must be ≥ 2.
    ConcatAudio {
        /// Number of audio input segments to concatenate. Must be ≥ 2.
        n: u32,
    },
    /// Freeze a single frame for a configurable duration using `FFmpeg`'s `loop` filter.
    ///
    /// The frame nearest to `pts` seconds is held for `duration` seconds, then
    /// playback resumes. Frame numbers are approximated using a 25 fps assumption;
    /// accuracy depends on the source stream's actual frame rate.
    FreezeFrame {
        /// Timestamp of the frame to freeze, in seconds. Must be >= 0.0.
        pts: f64,
        /// Duration to hold the frozen frame, in seconds. Must be > 0.0.
        duration: f64,
    },
    /// Scrolling text ticker (right-to-left) using the `drawtext` filter.
    ///
    /// The text starts off-screen to the right and scrolls left at
    /// `speed_px_per_sec` pixels per second using the expression
    /// `x = w - t * speed`.
    Ticker {
        /// Text to display. Special characters (`\`, `:`, `'`) are escaped.
        text: String,
        /// Y position as an `FFmpeg` expression, e.g. `"h-50"` or `"10"`.
        y: String,
        /// Horizontal scroll speed in pixels per second (must be > 0.0).
        speed_px_per_sec: f32,
        /// Font size in points.
        font_size: u32,
        /// Font color as an `FFmpeg` color string, e.g. `"white"` or `"0xFFFFFF"`.
        font_color: String,
    },
    /// Join two video clips with a cross-dissolve transition.
    ///
    /// Compound step — expands in `filter_inner` to:
    /// ```text
    /// in0 → trim(end=clip_a_end+dissolve_dur) → setpts → xfade[0]
    /// in1 → trim(start=max(0, clip_b_start−dissolve_dur)) → setpts → xfade[1]
    /// ```
    ///
    /// Requires two video input slots: slot 0 = clip A, slot 1 = clip B.
    /// `clip_a_end` and `dissolve_dur` must be > 0.0.
    JoinWithDissolve {
        /// Timestamp (seconds) where clip A ends. Must be > 0.0.
        clip_a_end: f64,
        /// Timestamp (seconds) where clip B content starts (before the overlap).
        clip_b_start: f64,
        /// Cross-dissolve overlap duration in seconds. Must be > 0.0.
        dissolve_dur: f64,
    },
    /// Composite a PNG image (watermark / logo) over video with optional opacity.
    ///
    /// This is a compound step: internally it creates a `movie` source,
    /// a `lut` alpha-scaling filter, and an `overlay` compositing filter.
    /// The image file is loaded once at graph construction time.
    OverlayImage {
        /// Absolute or relative path to the `.png` file.
        path: String,
        /// Horizontal position as an `FFmpeg` expression, e.g. `"10"` or `"W-w-10"`.
        x: String,
        /// Vertical position as an `FFmpeg` expression, e.g. `"10"` or `"H-h-10"`.
        y: String,
        /// Opacity 0.0 (fully transparent) to 1.0 (fully opaque).
        opacity: f32,
    },

    /// Blend a `top` layer over the current stream (bottom) using the given mode.
    ///
    /// This is a compound step:
    /// - **Normal** mode: `[top]colorchannelmixer=aa=<opacity>[top_faded];
    ///   [bottom][top_faded]overlay=format=auto:shortest=1[out]`
    ///   (the `colorchannelmixer` step is omitted when `opacity == 1.0`).
    /// - All other modes return [`crate::FilterError::InvalidConfig`] from
    ///   [`crate::FilterGraphBuilder::build`] until implemented.
    ///
    /// The `top` builder's steps are applied to the second input slot (`in1`).
    /// `opacity` is clamped to `[0.0, 1.0]` by the builder method.
    ///
    /// `Box<FilterGraphBuilder>` is used to break the otherwise-recursive type:
    /// `FilterStep` → `FilterGraphBuilder` → `Vec<FilterStep>`.
    Blend {
        /// Filter pipeline for the top (foreground) layer.
        top: Box<FilterGraphBuilder>,
        /// How the two layers are combined.
        mode: BlendMode,
        /// Opacity of the top layer in `[0.0, 1.0]`; 1.0 = fully opaque.
        opacity: f32,
    },

    /// Remove pixels matching `color` using `FFmpeg`'s `chromakey` filter,
    /// producing a `yuva420p` output with transparent areas where the key
    /// color was detected.
    ///
    /// Use this for YCbCr-encoded sources (most video).  For RGB sources
    /// use `colorkey` instead.
    ChromaKey {
        /// `FFmpeg` color string, e.g. `"green"`, `"0x00FF00"`, `"#00FF00"`.
        color: String,
        /// Match radius in `[0.0, 1.0]`; higher = more pixels removed.
        similarity: f32,
        /// Edge softness in `[0.0, 1.0]`; `0.0` = hard edge.
        blend: f32,
    },

    /// Remove pixels matching `color` in RGB space using `FFmpeg`'s `colorkey`
    /// filter, producing an `rgba` output with transparent areas where the key
    /// color was detected.
    ///
    /// Use this for RGB-encoded sources.  For YCbCr-encoded video (most video)
    /// use `chromakey` instead.
    ColorKey {
        /// `FFmpeg` color string, e.g. `"green"`, `"0x00FF00"`, `"#00FF00"`.
        color: String,
        /// Match radius in `[0.0, 1.0]`; higher = more pixels removed.
        similarity: f32,
        /// Edge softness in `[0.0, 1.0]`; `0.0` = hard edge.
        blend: f32,
    },

    /// Reduce color spill from the key color on subject edges using `FFmpeg`'s
    /// `hue` filter to desaturate the spill hue region.
    ///
    /// Applies `hue=h=0:s=(1.0 - strength)`.  `strength=0.0` leaves the image
    /// unchanged; `strength=1.0` fully desaturates.
    ///
    /// `key_color` is stored for future use by a more targeted per-hue
    /// implementation.
    SpillSuppress {
        /// `FFmpeg` color string identifying the spill color, e.g. `"green"`.
        key_color: String,
        /// Suppression intensity in `[0.0, 1.0]`; `0.0` = no effect, `1.0` = full suppression.
        strength: f32,
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
            Self::AFadeIn { .. } | Self::AFadeOut { .. } => "afade",
            Self::Rotate { .. } => "rotate",
            Self::ToneMap(_) => "tonemap",
            Self::Volume(_) => "volume",
            Self::Amix(_) => "amix",
            // ParametricEq is a compound step; "equalizer" is used only by
            // validate_filter_steps as a best-effort existence check.  The
            // actual nodes are built by `filter_inner::add_parametric_eq_chain`.
            Self::ParametricEq { .. } => "equalizer",
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
            Self::Reverse => "reverse",
            Self::AReverse => "areverse",
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
            Self::DrawText { .. } | Self::Ticker { .. } => "drawtext",
            // "setpts" is checked at build-time; the audio path uses "atempo"
            // which is verified at graph-construction time in filter_inner.
            Self::Speed { .. } => "setpts",
            Self::FreezeFrame { .. } => "loop",
            Self::LoudnessNormalize { .. } => "ebur128",
            Self::NormalizePeak { .. } => "astats",
            Self::ANoiseGate { .. } => "agate",
            Self::ACompressor { .. } => "acompressor",
            Self::StereoToMono => "pan",
            Self::ChannelMap { .. } => "channelmap",
            // AudioDelay dispatches to adelay (positive) or atrim (negative) at
            // build time; "adelay" is returned here for validate_filter_steps only.
            Self::AudioDelay { .. } => "adelay",
            Self::ConcatVideo { .. } | Self::ConcatAudio { .. } => "concat",
            // JoinWithDissolve is a compound step (trim+setpts → xfade ← setpts+trim);
            // "xfade" is used by validate_filter_steps as the primary filter check.
            Self::JoinWithDissolve { .. } => "xfade",
            Self::SubtitlesSrt { .. } => "subtitles",
            Self::SubtitlesAss { .. } => "ass",
            // OverlayImage is a compound step (movie → lut → overlay); "overlay"
            // is used only by validate_filter_steps as a best-effort existence
            // check.  The actual graph construction is handled by
            // `filter_inner::build::add_overlay_image_step`.
            Self::OverlayImage { .. } => "overlay",
            // Blend is a compound step; "overlay" is used as the primary filter
            // for validate_filter_steps.  Unimplemented modes are caught by
            // build() before validate_filter_steps is reached.
            Self::Blend { .. } => "overlay",
            Self::ChromaKey { .. } => "chromakey",
            Self::ColorKey { .. } => "colorkey",
            Self::SpillSuppress { .. } => "hue",
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
            Self::AFadeIn { start, duration } => {
                format!("type=in:start_time={start}:duration={duration}")
            }
            Self::AFadeOut { start, duration } => {
                format!("type=out:start_time={start}:duration={duration}")
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
            // args() for ParametricEq is not used by the build loop (which is
            // bypassed in favour of add_parametric_eq_chain); provided here for
            // completeness using the first band's args.
            Self::ParametricEq { bands } => bands.first().map(EqBand::args).unwrap_or_default(),
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
            Self::HFlip | Self::VFlip | Self::Reverse | Self::AReverse => String::new(),
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
            Self::Ticker {
                text,
                y,
                speed_px_per_sec,
                font_size,
                font_color,
            } => {
                // Use the same escaping as DrawText.
                let escaped = text
                    .replace('\\', "\\\\")
                    .replace(':', "\\:")
                    .replace('\'', "\\'");
                // x = w - t * speed: at t=0 the text starts fully off the right
                // edge (x = w) and scrolls left by `speed` pixels per second.
                format!(
                    "text='{escaped}':x=w-t*{speed_px_per_sec}:y={y}:\
                     fontsize={font_size}:fontcolor={font_color}"
                )
            }
            // Video path: divide PTS by factor to change playback speed.
            // Audio path args are built by filter_inner (chained atempo).
            Self::Speed { factor } => format!("PTS/{factor}"),
            // args() is not used by the build loop for LoudnessNormalize (two-pass
            // is handled entirely in filter_inner); provided here for completeness.
            Self::LoudnessNormalize { .. } => "peak=true:metadata=1".to_string(),
            // args() is not used by the build loop for NormalizePeak (two-pass
            // is handled entirely in filter_inner); provided here for completeness.
            Self::NormalizePeak { .. } => "metadata=1".to_string(),
            Self::FreezeFrame { pts, duration } => {
                // The `loop` filter needs a frame index and a loop count, not PTS or
                // wall-clock duration.  We approximate both using 25 fps; accuracy
                // depends on the source stream's actual frame rate.
                #[allow(clippy::cast_possible_truncation)]
                let start = (*pts * 25.0) as i64;
                #[allow(clippy::cast_possible_truncation)]
                let loop_count = (*duration * 25.0) as i64;
                format!("loop={loop_count}:size=1:start={start}")
            }
            Self::SubtitlesSrt { path } | Self::SubtitlesAss { path } => {
                format!("filename={path}")
            }
            // args() for OverlayImage returns the overlay positional args (x:y).
            // These are not consumed by add_and_link_step (which is bypassed for
            // this compound step); they exist here only for completeness.
            Self::OverlayImage { x, y, .. } => format!("{x}:{y}"),
            // args() for Blend is not consumed by add_and_link_step (which is
            // bypassed in favour of add_blend_normal_step).  Provided for
            // completeness using the Normal-mode overlay args.
            Self::Blend { .. } => "format=auto:shortest=1".to_string(),
            Self::ChromaKey {
                color,
                similarity,
                blend,
            } => format!("color={color}:similarity={similarity}:blend={blend}"),
            Self::ColorKey {
                color,
                similarity,
                blend,
            } => format!("color={color}:similarity={similarity}:blend={blend}"),
            Self::SpillSuppress { strength, .. } => format!("h=0:s={}", 1.0 - strength),
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
            Self::ANoiseGate {
                threshold_db,
                attack_ms,
                release_ms,
            } => {
                // `agate` expects threshold as a linear amplitude ratio (0.0–1.0).
                let threshold_linear = 10f32.powf(threshold_db / 20.0);
                format!("threshold={threshold_linear:.6}:attack={attack_ms}:release={release_ms}")
            }
            Self::ACompressor {
                threshold_db,
                ratio,
                attack_ms,
                release_ms,
                makeup_db,
            } => {
                format!(
                    "threshold={threshold_db}dB:ratio={ratio}:attack={attack_ms}:\
                     release={release_ms}:makeup={makeup_db}dB"
                )
            }
            Self::StereoToMono => "mono|c0=0.5*c0+0.5*c1".to_string(),
            Self::ChannelMap { mapping } => format!("map={mapping}"),
            // args() is not used directly for AudioDelay — the audio build loop
            // dispatches to add_raw_filter_step with the correct filter name and
            // args based on the sign of ms.  These are provided for completeness.
            Self::AudioDelay { ms } => {
                if *ms >= 0.0 {
                    format!("delays={ms}:all=1")
                } else {
                    format!("start={}", -ms / 1000.0)
                }
            }
            Self::ConcatVideo { n } => format!("n={n}:v=1:a=0"),
            Self::ConcatAudio { n } => format!("n={n}:v=0:a=1"),
            // args() for JoinWithDissolve is not used by the build loop (which is
            // bypassed in favour of add_join_with_dissolve_step); provided here for
            // completeness using the xfade args.
            Self::JoinWithDissolve {
                clip_a_end,
                dissolve_dur,
                ..
            } => format!("transition=dissolve:duration={dissolve_dur}:offset={clip_a_end}"),
        }
    }
}
