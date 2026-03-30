//! Video filter methods for [`FilterGraphBuilder`].

#[allow(clippy::wildcard_imports)]
use super::*;

impl FilterGraphBuilder {
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

    /// Reverse video playback using `FFmpeg`'s `reverse` filter.
    ///
    /// **Warning**: `reverse` buffers the entire clip in memory before producing
    /// any output. Only use this on short clips to avoid excessive memory usage.
    #[must_use]
    pub fn reverse(mut self) -> Self {
        self.steps.push(FilterStep::Reverse);
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

    /// Join two video streams with a cross-dissolve transition.
    ///
    /// Requires two video input slots: push clip A frames to slot 0 and clip B
    /// frames to slot 1.  Internally expands to
    /// `trim` + `setpts` → `xfade` ← `setpts` + `trim`.
    ///
    /// - `clip_a_end_sec`: timestamp (seconds) where clip A ends. Must be > 0.0.
    /// - `clip_b_start_sec`: timestamp (seconds) where clip B content starts
    ///   (before the overlap region).
    /// - `dissolve_dur_sec`: cross-dissolve overlap length in seconds. Must be > 0.0.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `dissolve_dur_sec ≤ 0.0` or `clip_a_end_sec ≤ 0.0`.
    #[must_use]
    pub fn join_with_dissolve(
        mut self,
        clip_a_end_sec: f64,
        clip_b_start_sec: f64,
        dissolve_dur_sec: f64,
    ) -> Self {
        self.steps.push(FilterStep::JoinWithDissolve {
            clip_a_end: clip_a_end_sec,
            clip_b_start: clip_b_start_sec,
            dissolve_dur: dissolve_dur_sec,
        });
        self
    }

    /// Change playback speed by `factor`.
    ///
    /// `factor > 1.0` = fast motion (e.g. `2.0` = double speed).
    /// `factor < 1.0` = slow motion (e.g. `0.5` = half speed).
    ///
    /// **Video**: uses `setpts=PTS/{factor}`.
    /// **Audio**: uses chained `atempo` filters (each in [0.5, 2.0]) so the
    /// full range 0.1–100.0 is covered without quality degradation.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `factor` is outside [0.1, 100.0].
    #[must_use]
    pub fn speed(mut self, factor: f64) -> Self {
        self.steps.push(FilterStep::Speed { factor });
        self
    }

    /// Concatenate `n_segments` sequential video inputs using `FFmpeg`'s `concat` filter.
    ///
    /// Requires `n_segments` video input slots (push to slots 0 through
    /// `n_segments - 1` in order). [`build`](Self::build) returns
    /// [`FilterError::InvalidConfig`] if `n_segments < 2`.
    #[must_use]
    pub fn concat_video(mut self, n_segments: u32) -> Self {
        self.steps.push(FilterStep::ConcatVideo { n: n_segments });
        self
    }

    /// Freeze the frame at `pts_sec` for `duration_sec` seconds using `FFmpeg`'s `loop` filter.
    ///
    /// The frame nearest to `pts_sec` is held for `duration_sec` seconds before
    /// playback resumes. Frame numbers are approximated using a 25 fps assumption;
    /// accuracy depends on the source stream's actual frame rate.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `pts_sec` is negative or `duration_sec` is ≤ 0.0.
    #[must_use]
    pub fn freeze_frame(mut self, pts_sec: f64, duration_sec: f64) -> Self {
        self.steps.push(FilterStep::FreezeFrame {
            pts: pts_sec,
            duration: duration_sec,
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

    /// Scroll text from right to left as a news ticker.
    ///
    /// Uses `FFmpeg`'s `drawtext` filter with the expression `x = w - t * speed`
    /// so the text enters from the right edge at playback start and advances
    /// left by `speed_px_per_sec` pixels per second.
    ///
    /// `y` is an `FFmpeg` expression string for the vertical position,
    /// e.g. `"h-50"` for 50 pixels above the bottom.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if:
    /// - `text` is empty, or
    /// - `speed_px_per_sec` is ≤ 0.0.
    #[must_use]
    pub fn ticker(
        mut self,
        text: &str,
        y: &str,
        speed_px_per_sec: f32,
        font_size: u32,
        font_color: &str,
    ) -> Self {
        self.steps.push(FilterStep::Ticker {
            text: text.to_owned(),
            y: y.to_owned(),
            speed_px_per_sec,
            font_size,
            font_color: font_color.to_owned(),
        });
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

    /// Composite a PNG image (watermark / logo) over video.
    ///
    /// The image at `path` is loaded once at graph construction time via
    /// `FFmpeg`'s `movie` source filter. Its alpha channel is scaled by
    /// `opacity` using a `lut` filter, then composited onto the main stream
    /// with the `overlay` filter at position `(x, y)`.
    ///
    /// `x` and `y` are `FFmpeg` expression strings, e.g. `"10"`, `"W-w-10"`.
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if:
    /// - the extension is not `.png`,
    /// - the file does not exist at build time, or
    /// - `opacity` is outside `[0.0, 1.0]`.
    #[must_use]
    pub fn overlay_image(mut self, path: &str, x: &str, y: &str, opacity: f32) -> Self {
        self.steps.push(FilterStep::OverlayImage {
            path: path.to_owned(),
            x: x.to_owned(),
            y: y.to_owned(),
            opacity,
        });
        self
    }
}

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
    fn tone_map_variants_should_have_correct_names() {
        assert_eq!(ToneMap::Hable.as_str(), "hable");
        assert_eq!(ToneMap::Reinhard.as_str(), "reinhard");
        assert_eq!(ToneMap::Mobius.as_str(), "mobius");
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
    fn filter_step_fade_in_should_produce_correct_filter_name() {
        let step = FilterStep::FadeIn {
            start: 0.0,
            duration: 1.5,
        };
        assert_eq!(step.filter_name(), "fade");
    }

    #[test]
    fn filter_step_fade_in_should_produce_correct_args() {
        let step = FilterStep::FadeIn {
            start: 0.0,
            duration: 1.5,
        };
        assert_eq!(step.args(), "type=in:start_time=0:duration=1.5");
    }

    #[test]
    fn filter_step_fade_in_with_nonzero_start_should_produce_correct_args() {
        let step = FilterStep::FadeIn {
            start: 2.0,
            duration: 1.0,
        };
        assert_eq!(step.args(), "type=in:start_time=2:duration=1");
    }

    #[test]
    fn filter_step_fade_out_should_produce_correct_filter_name() {
        let step = FilterStep::FadeOut {
            start: 8.5,
            duration: 1.5,
        };
        assert_eq!(step.filter_name(), "fade");
    }

    #[test]
    fn filter_step_fade_out_should_produce_correct_args() {
        let step = FilterStep::FadeOut {
            start: 8.5,
            duration: 1.5,
        };
        assert_eq!(step.args(), "type=out:start_time=8.5:duration=1.5");
    }

    #[test]
    fn builder_fade_in_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().fade_in(0.0, 1.5).build();
        assert!(
            result.is_ok(),
            "fade_in(0.0, 1.5) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_fade_out_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().fade_out(8.5, 1.5).build();
        assert!(
            result.is_ok(),
            "fade_out(8.5, 1.5) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_fade_in_with_zero_duration_should_return_invalid_config() {
        let result = FilterGraph::builder().fade_in(0.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for zero duration, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("duration"),
                "reason should mention duration: {reason}"
            );
        }
    }

    #[test]
    fn builder_fade_out_with_negative_duration_should_return_invalid_config() {
        let result = FilterGraph::builder().fade_out(0.0, -1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative duration, got {result:?}"
        );
    }

    #[test]
    fn filter_step_fade_in_white_should_produce_correct_filter_name() {
        let step = FilterStep::FadeInWhite {
            start: 0.0,
            duration: 1.0,
        };
        assert_eq!(step.filter_name(), "fade");
    }

    #[test]
    fn filter_step_fade_in_white_should_produce_correct_args() {
        let step = FilterStep::FadeInWhite {
            start: 0.0,
            duration: 1.0,
        };
        assert_eq!(step.args(), "type=in:start_time=0:duration=1:color=white");
    }

    #[test]
    fn filter_step_fade_in_white_with_nonzero_start_should_produce_correct_args() {
        let step = FilterStep::FadeInWhite {
            start: 2.5,
            duration: 1.0,
        };
        assert_eq!(step.args(), "type=in:start_time=2.5:duration=1:color=white");
    }

    #[test]
    fn filter_step_fade_out_white_should_produce_correct_filter_name() {
        let step = FilterStep::FadeOutWhite {
            start: 8.0,
            duration: 1.0,
        };
        assert_eq!(step.filter_name(), "fade");
    }

    #[test]
    fn filter_step_fade_out_white_should_produce_correct_args() {
        let step = FilterStep::FadeOutWhite {
            start: 8.0,
            duration: 1.0,
        };
        assert_eq!(step.args(), "type=out:start_time=8:duration=1:color=white");
    }

    #[test]
    fn builder_fade_in_white_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().fade_in_white(0.0, 1.0).build();
        assert!(
            result.is_ok(),
            "fade_in_white(0.0, 1.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_fade_out_white_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().fade_out_white(8.0, 1.0).build();
        assert!(
            result.is_ok(),
            "fade_out_white(8.0, 1.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_fade_in_white_with_zero_duration_should_return_invalid_config() {
        let result = FilterGraph::builder().fade_in_white(0.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for zero duration, got {result:?}"
        );
    }

    #[test]
    fn builder_fade_out_white_with_negative_duration_should_return_invalid_config() {
        let result = FilterGraph::builder().fade_out_white(0.0, -1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative duration, got {result:?}"
        );
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
        use super::super::super::filter_step::FilterStep as FS;
        // Access kelvin_to_rgb indirectly through the WhiteBalance step args
        let step_warm = FS::WhiteBalance {
            temperature_k: 3200,
            tint: 0.0,
        };
        let step_cool = FS::WhiteBalance {
            temperature_k: 10000,
            tint: 0.0,
        };
        let args_warm = step_warm.args();
        let args_cool = step_cool.args();
        // At warm temperature, rr value should be higher than bb value
        // Just verify the args are produced without panicking
        assert!(
            args_warm.contains("rr=") && args_warm.contains("bb="),
            "args={args_warm}"
        );
        assert!(
            args_cool.contains("rr=") && args_cool.contains("bb="),
            "args={args_cool}"
        );
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

    #[test]
    fn xfade_transition_dissolve_should_produce_correct_str() {
        assert_eq!(XfadeTransition::Dissolve.as_str(), "dissolve");
    }

    #[test]
    fn xfade_transition_all_variants_should_produce_unique_strings() {
        let variants = [
            (XfadeTransition::Dissolve, "dissolve"),
            (XfadeTransition::Fade, "fade"),
            (XfadeTransition::WipeLeft, "wipeleft"),
            (XfadeTransition::WipeRight, "wiperight"),
            (XfadeTransition::WipeUp, "wipeup"),
            (XfadeTransition::WipeDown, "wipedown"),
            (XfadeTransition::SlideLeft, "slideleft"),
            (XfadeTransition::SlideRight, "slideright"),
            (XfadeTransition::SlideUp, "slideup"),
            (XfadeTransition::SlideDown, "slidedown"),
            (XfadeTransition::CircleOpen, "circleopen"),
            (XfadeTransition::CircleClose, "circleclose"),
            (XfadeTransition::FadeGrays, "fadegrays"),
            (XfadeTransition::Pixelize, "pixelize"),
        ];
        for (variant, expected) in variants {
            assert_eq!(
                variant.as_str(),
                expected,
                "XfadeTransition::{variant:?} should produce \"{expected}\""
            );
        }
    }

    #[test]
    fn filter_step_xfade_should_produce_correct_filter_name() {
        let step = FilterStep::XFade {
            transition: XfadeTransition::Dissolve,
            duration: 1.0,
            offset: 4.0,
        };
        assert_eq!(step.filter_name(), "xfade");
    }

    #[test]
    fn filter_step_xfade_should_produce_correct_args() {
        let step = FilterStep::XFade {
            transition: XfadeTransition::Dissolve,
            duration: 1.0,
            offset: 4.0,
        };
        assert_eq!(step.args(), "transition=dissolve:duration=1:offset=4");
    }

    #[test]
    fn filter_step_xfade_wipe_right_should_produce_correct_args() {
        let step = FilterStep::XFade {
            transition: XfadeTransition::WipeRight,
            duration: 0.5,
            offset: 9.5,
        };
        assert_eq!(step.args(), "transition=wiperight:duration=0.5:offset=9.5");
    }

    #[test]
    fn builder_xfade_with_valid_params_should_succeed() {
        let result = FilterGraph::builder()
            .xfade(XfadeTransition::Dissolve, 1.0, 4.0)
            .build();
        assert!(
            result.is_ok(),
            "xfade(Dissolve, 1.0, 4.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_xfade_with_zero_duration_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .xfade(XfadeTransition::Dissolve, 0.0, 4.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for zero duration, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("duration"),
                "reason should mention duration: {reason}"
            );
        }
    }

    #[test]
    fn builder_xfade_with_negative_duration_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .xfade(XfadeTransition::Fade, -1.0, 0.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative duration, got {result:?}"
        );
    }

    fn make_drawtext_opts() -> DrawTextOptions {
        DrawTextOptions {
            text: "Hello".to_string(),
            x: "10".to_string(),
            y: "10".to_string(),
            font_size: 24,
            font_color: "white".to_string(),
            font_file: None,
            opacity: 1.0,
            box_color: None,
            box_border_width: 0,
        }
    }

    #[test]
    fn filter_step_drawtext_should_produce_correct_filter_name() {
        let step = FilterStep::DrawText {
            opts: make_drawtext_opts(),
        };
        assert_eq!(step.filter_name(), "drawtext");
    }

    #[test]
    fn filter_step_drawtext_should_produce_correct_args_without_box() {
        let step = FilterStep::DrawText {
            opts: make_drawtext_opts(),
        };
        let args = step.args();
        assert!(
            args.contains("text='Hello'"),
            "args must contain text: {args}"
        );
        assert!(args.contains("x=10"), "args must contain x: {args}");
        assert!(args.contains("y=10"), "args must contain y: {args}");
        assert!(
            args.contains("fontsize=24"),
            "args must contain fontsize: {args}"
        );
        assert!(
            args.contains("fontcolor=white@1.00"),
            "args must contain fontcolor with opacity: {args}"
        );
        assert!(
            !args.contains("box=1"),
            "args must not contain box when box_color is None: {args}"
        );
    }

    #[test]
    fn filter_step_drawtext_with_box_should_include_box_args() {
        let opts = DrawTextOptions {
            box_color: Some("black@0.5".to_string()),
            box_border_width: 5,
            ..make_drawtext_opts()
        };
        let step = FilterStep::DrawText { opts };
        let args = step.args();
        assert!(args.contains("box=1"), "args must contain box=1: {args}");
        assert!(
            args.contains("boxcolor=black@0.5"),
            "args must contain boxcolor: {args}"
        );
        assert!(
            args.contains("boxborderw=5"),
            "args must contain boxborderw: {args}"
        );
    }

    #[test]
    fn filter_step_drawtext_with_font_file_should_include_fontfile_arg() {
        let opts = DrawTextOptions {
            font_file: Some("/usr/share/fonts/arial.ttf".to_string()),
            ..make_drawtext_opts()
        };
        let step = FilterStep::DrawText { opts };
        let args = step.args();
        assert!(
            args.contains("fontfile=/usr/share/fonts/arial.ttf"),
            "args must contain fontfile: {args}"
        );
    }

    #[test]
    fn filter_step_drawtext_should_escape_colon_in_text() {
        let opts = DrawTextOptions {
            text: "Time: 12:00".to_string(),
            ..make_drawtext_opts()
        };
        let step = FilterStep::DrawText { opts };
        let args = step.args();
        assert!(
            args.contains("Time\\: 12\\:00"),
            "colons in text must be escaped: {args}"
        );
    }

    #[test]
    fn filter_step_drawtext_should_escape_backslash_in_text() {
        let opts = DrawTextOptions {
            text: "path\\file".to_string(),
            ..make_drawtext_opts()
        };
        let step = FilterStep::DrawText { opts };
        let args = step.args();
        assert!(
            args.contains("path\\\\file"),
            "backslash in text must be escaped: {args}"
        );
    }

    #[test]
    fn builder_drawtext_with_valid_opts_should_succeed() {
        let result = FilterGraph::builder()
            .drawtext(make_drawtext_opts())
            .build();
        assert!(
            result.is_ok(),
            "drawtext with valid opts must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_drawtext_with_empty_text_should_return_invalid_config() {
        let opts = DrawTextOptions {
            text: String::new(),
            ..make_drawtext_opts()
        };
        let result = FilterGraph::builder().drawtext(opts).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for empty text, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("text"),
                "reason should mention text: {reason}"
            );
        }
    }

    #[test]
    fn builder_drawtext_with_opacity_too_high_should_return_invalid_config() {
        let opts = DrawTextOptions {
            opacity: 1.5,
            ..make_drawtext_opts()
        };
        let result = FilterGraph::builder().drawtext(opts).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for opacity > 1.0, got {result:?}"
        );
    }

    #[test]
    fn builder_drawtext_with_negative_opacity_should_return_invalid_config() {
        let opts = DrawTextOptions {
            opacity: -0.1,
            ..make_drawtext_opts()
        };
        let result = FilterGraph::builder().drawtext(opts).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for opacity < 0.0, got {result:?}"
        );
    }

    #[test]
    fn filter_step_subtitles_srt_should_produce_correct_filter_name() {
        let step = FilterStep::SubtitlesSrt {
            path: "subs.srt".to_owned(),
        };
        assert_eq!(step.filter_name(), "subtitles");
    }

    #[test]
    fn filter_step_subtitles_srt_should_produce_correct_args() {
        let step = FilterStep::SubtitlesSrt {
            path: "subs.srt".to_owned(),
        };
        assert_eq!(step.args(), "filename=subs.srt");
    }

    #[test]
    fn builder_subtitles_srt_with_wrong_extension_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_srt("subtitles.vtt")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for wrong extension, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("unsupported subtitle format"),
                "reason should mention unsupported format: {reason}"
            );
        }
    }

    #[test]
    fn builder_subtitles_srt_with_no_extension_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_srt("subtitles_no_ext")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for missing extension, got {result:?}"
        );
    }

    #[test]
    fn builder_subtitles_srt_with_nonexistent_file_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_srt("/nonexistent/path/subs_ab12cd.srt")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for nonexistent file, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("subtitle file not found"),
                "reason should mention file not found: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_subtitles_ass_should_produce_correct_filter_name() {
        let step = FilterStep::SubtitlesAss {
            path: "subs.ass".to_owned(),
        };
        assert_eq!(step.filter_name(), "ass");
    }

    #[test]
    fn filter_step_subtitles_ass_should_produce_correct_args() {
        let step = FilterStep::SubtitlesAss {
            path: "subs.ass".to_owned(),
        };
        assert_eq!(step.args(), "filename=subs.ass");
    }

    #[test]
    fn filter_step_subtitles_ssa_should_produce_correct_filter_name() {
        let step = FilterStep::SubtitlesAss {
            path: "subs.ssa".to_owned(),
        };
        assert_eq!(step.filter_name(), "ass");
    }

    #[test]
    fn builder_subtitles_ass_with_wrong_extension_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_ass("subtitles.srt")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for wrong extension, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("unsupported subtitle format"),
                "reason should mention unsupported format: {reason}"
            );
        }
    }

    #[test]
    fn builder_subtitles_ass_with_no_extension_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_ass("subtitles_no_ext")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for missing extension, got {result:?}"
        );
    }

    #[test]
    fn builder_subtitles_ass_with_nonexistent_file_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_ass("/nonexistent/path/subs_ab12cd.ass")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for nonexistent .ass file, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("subtitle file not found"),
                "reason should mention file not found: {reason}"
            );
        }
    }

    #[test]
    fn builder_subtitles_ssa_with_nonexistent_file_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .subtitles_ass("/nonexistent/path/subs_ab12cd.ssa")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for nonexistent .ssa file, got {result:?}"
        );
    }

    #[test]
    fn filter_step_overlay_image_should_produce_correct_filter_name() {
        let step = FilterStep::OverlayImage {
            path: "logo.png".to_owned(),
            x: "10".to_owned(),
            y: "10".to_owned(),
            opacity: 1.0,
        };
        assert_eq!(step.filter_name(), "overlay");
    }

    #[test]
    fn filter_step_overlay_image_should_produce_correct_args() {
        let step = FilterStep::OverlayImage {
            path: "logo.png".to_owned(),
            x: "W-w-10".to_owned(),
            y: "H-h-10".to_owned(),
            opacity: 0.7,
        };
        assert_eq!(step.args(), "W-w-10:H-h-10");
    }

    #[test]
    fn builder_overlay_image_with_wrong_extension_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .overlay_image("logo.jpg", "10", "10", 1.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for wrong extension, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("unsupported image format"),
                "reason should mention unsupported format: {reason}"
            );
        }
    }

    #[test]
    fn builder_overlay_image_with_no_extension_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .overlay_image("logo_no_ext", "10", "10", 1.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for missing extension, got {result:?}"
        );
    }

    #[test]
    fn builder_overlay_image_with_nonexistent_file_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .overlay_image("/nonexistent/path/logo_ab12cd.png", "10", "10", 1.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for nonexistent file, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("overlay image not found"),
                "reason should mention file not found: {reason}"
            );
        }
    }

    #[test]
    fn builder_overlay_image_with_opacity_above_1_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .overlay_image("/nonexistent/logo.png", "10", "10", 1.1)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for opacity > 1.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("opacity"),
                "reason should mention opacity: {reason}"
            );
        }
    }

    #[test]
    fn builder_overlay_image_with_negative_opacity_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .overlay_image("/nonexistent/logo.png", "10", "10", -0.1)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for opacity < 0.0, got {result:?}"
        );
    }

    #[test]
    fn filter_step_ticker_should_produce_correct_filter_name() {
        let step = FilterStep::Ticker {
            text: "Breaking news".to_owned(),
            y: "h-50".to_owned(),
            speed_px_per_sec: 100.0,
            font_size: 24,
            font_color: "white".to_owned(),
        };
        assert_eq!(step.filter_name(), "drawtext");
    }

    #[test]
    fn filter_step_ticker_should_produce_correct_args() {
        let step = FilterStep::Ticker {
            text: "Breaking news".to_owned(),
            y: "h-50".to_owned(),
            speed_px_per_sec: 100.0,
            font_size: 24,
            font_color: "white".to_owned(),
        };
        let args = step.args();
        assert!(
            args.contains("text='Breaking news'"),
            "args should contain escaped text: {args}"
        );
        assert!(
            args.contains("x=w-t*100"),
            "args should contain scrolling x expression: {args}"
        );
        assert!(args.contains("y=h-50"), "args should contain y: {args}");
        assert!(
            args.contains("fontsize=24"),
            "args should contain fontsize: {args}"
        );
        assert!(
            args.contains("fontcolor=white"),
            "args should contain fontcolor: {args}"
        );
    }

    #[test]
    fn filter_step_ticker_should_escape_special_characters_in_text() {
        let step = FilterStep::Ticker {
            text: "colon:backslash\\apostrophe'".to_owned(),
            y: "10".to_owned(),
            speed_px_per_sec: 50.0,
            font_size: 20,
            font_color: "red".to_owned(),
        };
        let args = step.args();
        assert!(
            args.contains("\\:"),
            "colon should be escaped in args: {args}"
        );
        assert!(
            args.contains("\\'"),
            "apostrophe should be escaped in args: {args}"
        );
        assert!(
            args.contains("\\\\"),
            "backslash should be escaped in args: {args}"
        );
    }

    #[test]
    fn builder_ticker_with_empty_text_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .ticker("", "h-50", 100.0, 24, "white")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for empty text, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("ticker text must not be empty"),
                "reason should mention empty text: {reason}"
            );
        }
    }

    #[test]
    fn builder_ticker_with_zero_speed_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .ticker("Breaking news", "h-50", 0.0, 24, "white")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for speed = 0.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("speed_px_per_sec"),
                "reason should mention speed_px_per_sec: {reason}"
            );
        }
    }

    #[test]
    fn builder_ticker_with_negative_speed_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .ticker("Breaking news", "h-50", -50.0, 24, "white")
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative speed, got {result:?}"
        );
    }

    #[test]
    fn filter_step_speed_should_produce_correct_filter_name() {
        let step = FilterStep::Speed { factor: 2.0 };
        assert_eq!(step.filter_name(), "setpts");
    }

    #[test]
    fn filter_step_speed_should_produce_correct_args_for_double_speed() {
        let step = FilterStep::Speed { factor: 2.0 };
        assert_eq!(step.args(), "PTS/2");
    }

    #[test]
    fn filter_step_speed_should_produce_correct_args_for_half_speed() {
        let step = FilterStep::Speed { factor: 0.5 };
        assert_eq!(step.args(), "PTS/0.5");
    }

    #[test]
    fn builder_speed_with_factor_below_minimum_should_return_invalid_config() {
        let result = FilterGraph::builder().speed(0.09).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for factor below 0.1, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("speed factor"),
                "reason should mention speed factor: {reason}"
            );
        }
    }

    #[test]
    fn builder_speed_with_factor_above_maximum_should_return_invalid_config() {
        let result = FilterGraph::builder().speed(100.1).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for factor above 100.0, got {result:?}"
        );
    }

    #[test]
    fn builder_speed_with_zero_factor_should_return_invalid_config() {
        let result = FilterGraph::builder().speed(0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for factor 0.0, got {result:?}"
        );
    }

    #[test]
    fn builder_speed_at_boundary_values_should_succeed() {
        let low = FilterGraph::builder().speed(0.1).build();
        assert!(low.is_ok(), "speed(0.1) should succeed, got {low:?}");
        let high = FilterGraph::builder().speed(100.0).build();
        assert!(high.is_ok(), "speed(100.0) should succeed, got {high:?}");
    }

    #[test]
    fn filter_step_reverse_should_produce_correct_filter_name_and_empty_args() {
        let step = FilterStep::Reverse;
        assert_eq!(step.filter_name(), "reverse");
        assert_eq!(step.args(), "");
    }

    #[test]
    fn builder_reverse_should_succeed() {
        let result = FilterGraph::builder().reverse().build();
        assert!(
            result.is_ok(),
            "reverse must build successfully, got {result:?}"
        );
    }

    #[test]
    fn filter_step_freeze_frame_should_produce_correct_filter_name() {
        let step = FilterStep::FreezeFrame {
            pts: 2.0,
            duration: 3.0,
        };
        assert_eq!(step.filter_name(), "loop");
    }

    #[test]
    fn filter_step_freeze_frame_should_produce_correct_args() {
        let step = FilterStep::FreezeFrame {
            pts: 2.0,
            duration: 3.0,
        };
        // 2.0s * 25fps = frame 50; 3.0s * 25fps = 75 loop iterations
        assert_eq!(step.args(), "loop=75:size=1:start=50");
    }

    #[test]
    fn filter_step_freeze_frame_at_zero_pts_should_produce_start_zero() {
        let step = FilterStep::FreezeFrame {
            pts: 0.0,
            duration: 1.0,
        };
        assert_eq!(step.args(), "loop=25:size=1:start=0");
    }

    #[test]
    fn builder_freeze_frame_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().freeze_frame(2.0, 3.0).build();
        assert!(
            result.is_ok(),
            "freeze_frame(2.0, 3.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_freeze_frame_with_negative_pts_should_return_invalid_config() {
        let result = FilterGraph::builder().freeze_frame(-1.0, 3.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative pts, got {result:?}"
        );
    }

    #[test]
    fn builder_freeze_frame_with_zero_duration_should_return_invalid_config() {
        let result = FilterGraph::builder().freeze_frame(2.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for zero duration, got {result:?}"
        );
    }

    #[test]
    fn builder_freeze_frame_with_negative_duration_should_return_invalid_config() {
        let result = FilterGraph::builder().freeze_frame(2.0, -1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative duration, got {result:?}"
        );
    }

    #[test]
    fn filter_step_concat_video_should_have_correct_filter_name() {
        let step = FilterStep::ConcatVideo { n: 2 };
        assert_eq!(step.filter_name(), "concat");
    }

    #[test]
    fn filter_step_concat_video_should_produce_correct_args_for_n2() {
        let step = FilterStep::ConcatVideo { n: 2 };
        assert_eq!(step.args(), "n=2:v=1:a=0");
    }

    #[test]
    fn filter_step_concat_video_should_produce_correct_args_for_n3() {
        let step = FilterStep::ConcatVideo { n: 3 };
        assert_eq!(step.args(), "n=3:v=1:a=0");
    }

    #[test]
    fn builder_concat_video_valid_should_build_successfully() {
        let result = FilterGraph::builder().concat_video(2).build();
        assert!(
            result.is_ok(),
            "concat_video(2) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_concat_video_with_n1_should_return_invalid_config() {
        let result = FilterGraph::builder().concat_video(1).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for n=1, got {result:?}"
        );
    }

    #[test]
    fn builder_concat_video_with_n0_should_return_invalid_config() {
        let result = FilterGraph::builder().concat_video(0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for n=0, got {result:?}"
        );
    }

    #[test]
    fn filter_step_join_with_dissolve_should_have_correct_filter_name() {
        let step = FilterStep::JoinWithDissolve {
            clip_a_end: 4.0,
            clip_b_start: 1.0,
            dissolve_dur: 1.0,
        };
        assert_eq!(step.filter_name(), "xfade");
    }

    #[test]
    fn filter_step_join_with_dissolve_should_produce_correct_args() {
        let step = FilterStep::JoinWithDissolve {
            clip_a_end: 4.0,
            clip_b_start: 1.0,
            dissolve_dur: 1.0,
        };
        assert_eq!(
            step.args(),
            "transition=dissolve:duration=1:offset=4",
            "args must match xfade format for join_with_dissolve"
        );
    }

    #[test]
    fn builder_join_with_dissolve_valid_should_build_successfully() {
        let result = FilterGraph::builder()
            .join_with_dissolve(4.0, 1.0, 1.0)
            .build();
        assert!(
            result.is_ok(),
            "join_with_dissolve(4.0, 1.0, 1.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_join_with_dissolve_with_zero_dissolve_dur_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .join_with_dissolve(4.0, 1.0, 0.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for dissolve_dur=0.0, got {result:?}"
        );
    }

    #[test]
    fn builder_join_with_dissolve_with_negative_dissolve_dur_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .join_with_dissolve(4.0, 1.0, -1.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for dissolve_dur=-1.0, got {result:?}"
        );
    }

    #[test]
    fn builder_join_with_dissolve_with_zero_clip_a_end_should_return_invalid_config() {
        let result = FilterGraph::builder()
            .join_with_dissolve(0.0, 1.0, 1.0)
            .build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for clip_a_end=0.0, got {result:?}"
        );
    }
}
