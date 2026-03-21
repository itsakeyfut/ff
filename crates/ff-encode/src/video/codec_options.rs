//! Per-codec encoding options for [`VideoEncoderBuilder`](super::builder::VideoEncoderBuilder).
//!
//! Pass a [`VideoCodecOptions`] value to
//! `VideoEncoderBuilder::codec_options()` to control codec-specific behaviour.
//! Options are applied via `av_opt_set` / direct field assignment **before**
//! `avcodec_open2`.  Any option that the chosen encoder does not support is
//! logged as a warning and skipped — it never causes `build()` to return an
//! error.

/// Per-codec encoding options.
///
/// The variant must match the codec passed to
/// `VideoEncoderBuilder::video_codec()`.  A mismatch is silently ignored
/// (the options are not applied).
///
/// Variants without struct fields (`Vp9`, `ProRes`, `Dnxhd`) are reserved
/// for future issues and currently have no effect.
#[derive(Debug, Clone)]
pub enum VideoCodecOptions {
    /// H.264 (AVC) encoding options.
    H264(H264Options),
    /// H.265 (HEVC) encoding options.
    H265(H265Options),
    /// AV1 (libaom-av1) encoding options.
    Av1(Av1Options),
    /// AV1 (SVT-AV1 / libsvtav1) encoding options.
    Av1Svt(SvtAv1Options),
    /// VP9 encoding options (reserved for a future issue).
    Vp9(Vp9Options),
    /// Apple ProRes encoding options (reserved for a future issue).
    ProRes(ProResOptions),
    /// Avid DNxHD / DNxHR encoding options (reserved for a future issue).
    Dnxhd(DnxhdOptions),
}

// ── H.264 ────────────────────────────────────────────────────────────────────

/// H.264 (AVC) per-codec options.
#[derive(Debug, Clone)]
pub struct H264Options {
    /// Encoding profile.
    pub profile: H264Profile,
    /// Encoding level as an integer (e.g. `31` = 3.1, `40` = 4.0, `51` = 5.1).
    ///
    /// `None` leaves the encoder default.
    pub level: Option<u32>,
    /// Maximum consecutive B-frames (0–16).
    pub bframes: u32,
    /// GOP size: number of frames between keyframes.
    pub gop_size: u32,
    /// Number of reference frames.
    pub refs: u32,
    /// libx264 encoding speed/quality preset.
    ///
    /// Overrides the global [`Preset`](crate::Preset) when set. `None` falls
    /// back to whatever the builder's `.preset()` selector chose.
    /// Hardware encoders do not support libx264 presets — the option is
    /// silently skipped with a `warn!` log when unsupported.
    pub preset: Option<H264Preset>,
    /// libx264 perceptual tuning parameter.
    ///
    /// `None` leaves the encoder default. Hardware encoders ignore this
    /// option (logged as a warning and skipped).
    pub tune: Option<H264Tune>,
}

impl Default for H264Options {
    fn default() -> Self {
        Self {
            profile: H264Profile::High,
            level: None,
            bframes: 2,
            gop_size: 250,
            refs: 3,
            preset: None,
            tune: None,
        }
    }
}

/// H.264 encoding profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum H264Profile {
    /// Baseline profile — no B-frames, no CABAC (low latency / mobile).
    Baseline,
    /// Main profile.
    Main,
    /// High profile (recommended for most uses).
    #[default]
    High,
    /// High 10-bit profile.
    High10,
}

impl H264Profile {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Main => "main",
            Self::High => "high",
            Self::High10 => "high10",
        }
    }
}

/// libx264 encoding speed/quality preset.
///
/// Slower presets produce higher quality at the same bitrate but take longer
/// to encode. Not supported by hardware encoders (NVENC, QSV, etc.) — the
/// option is skipped with a warning when the encoder does not recognise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Preset {
    /// Fastest encoding, lowest quality.
    Ultrafast,
    /// Very fast encoding.
    Superfast,
    /// Fast encoding.
    Veryfast,
    /// Faster than default.
    Faster,
    /// Slightly faster than default.
    Fast,
    /// Default preset — balanced speed and quality.
    Medium,
    /// Slower encoding, better quality.
    Slow,
    /// Noticeably slower, noticeably better quality.
    Slower,
    /// Very slow, near-optimal quality.
    Veryslow,
    /// Slowest, maximum compression (not recommended for production).
    Placebo,
}

impl H264Preset {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
            Self::Placebo => "placebo",
        }
    }
}

/// libx264 perceptual tuning parameter.
///
/// Adjusts encoder settings for a specific type of source content or
/// quality metric. Not supported by hardware encoders — skipped with a
/// warning when the encoder does not recognise the option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Tune {
    /// Optimised for live-action film content.
    Film,
    /// Optimised for animation.
    Animation,
    /// Optimised for grainy content.
    Grain,
    /// Optimised for still images (single-frame encoding).
    Stillimage,
    /// Optimise for PSNR quality metric.
    Psnr,
    /// Optimise for SSIM quality metric.
    Ssim,
    /// Reduce decoding complexity (disables certain features).
    Fastdecode,
    /// Minimise encoding latency (no B-frames, no lookahead).
    Zerolatency,
}

impl H264Tune {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Film => "film",
            Self::Animation => "animation",
            Self::Grain => "grain",
            Self::Stillimage => "stillimage",
            Self::Psnr => "psnr",
            Self::Ssim => "ssim",
            Self::Fastdecode => "fastdecode",
            Self::Zerolatency => "zerolatency",
        }
    }
}

// ── H.265 ────────────────────────────────────────────────────────────────────

/// H.265 (HEVC) per-codec options.
#[derive(Debug, Clone)]
pub struct H265Options {
    /// Encoding profile.
    pub profile: H265Profile,
    /// Encoding tier.
    pub tier: H265Tier,
    /// Encoding level as an integer (e.g. `31` = 3.1, `51` = 5.1).
    ///
    /// `None` leaves the encoder default.
    pub level: Option<u32>,
    /// libx265 encoding speed/quality preset (e.g. `"ultrafast"`, `"medium"`, `"slow"`).
    ///
    /// `None` leaves the encoder default. Invalid or unsupported values are logged as a
    /// warning and skipped — `build()` never fails due to an unsupported preset.
    /// Hardware HEVC encoders (hevc_nvenc, etc.) ignore this option.
    pub preset: Option<String>,
    /// Raw x265-params string passed verbatim to libx265 (e.g. `"ctu=32:ref=4"`).
    ///
    /// **Note**: H.265 encoding requires an FFmpeg build with `--enable-libx265`.
    ///
    /// An invalid parameter string is logged as a warning and skipped. It never causes
    /// `build()` to return an error.
    pub x265_params: Option<String>,
}

impl Default for H265Options {
    fn default() -> Self {
        Self {
            profile: H265Profile::Main,
            tier: H265Tier::Main,
            level: None,
            preset: None,
            x265_params: None,
        }
    }
}

/// H.265 encoding profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum H265Profile {
    /// Main profile (8-bit, 4:2:0).
    #[default]
    Main,
    /// Main 10-bit profile (HDR-capable).
    Main10,
}

impl H265Profile {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Main10 => "main10",
        }
    }
}

/// H.265 encoding tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum H265Tier {
    /// Main tier — lower bitrate ceiling (consumer content).
    #[default]
    Main,
    /// High tier — higher bitrate ceiling (broadcast / professional).
    High,
}

impl H265Tier {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::High => "high",
        }
    }
}

// ── AV1 ──────────────────────────────────────────────────────────────────────

/// AV1 per-codec options (libaom-av1).
#[derive(Debug, Clone)]
pub struct Av1Options {
    /// CPU effort level: `0` = slowest / best quality, `8` = fastest / lowest quality.
    pub cpu_used: u8,
    /// Log2 of the number of tile rows (0–6).
    pub tile_rows: u8,
    /// Log2 of the number of tile columns (0–6).
    pub tile_cols: u8,
    /// Encoding usage mode.
    pub usage: Av1Usage,
}

impl Default for Av1Options {
    fn default() -> Self {
        Self {
            cpu_used: 4,
            tile_rows: 0,
            tile_cols: 0,
            usage: Av1Usage::VoD,
        }
    }
}

/// AV1 encoding usage mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Av1Usage {
    /// Video-on-demand: maximise quality at the cost of speed.
    #[default]
    VoD,
    /// Real-time: minimise encoding latency.
    RealTime,
}

impl Av1Usage {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::VoD => "vod",
            Self::RealTime => "realtime",
        }
    }
}

// ── SVT-AV1 ──────────────────────────────────────────────────────────────────

/// SVT-AV1 (libsvtav1) per-codec options.
///
/// **Note**: Requires an FFmpeg build with `--enable-libsvtav1` (LGPL).
/// `build()` returns [`EncodeError::EncoderUnavailable`](crate::EncodeError::EncoderUnavailable)
/// when libsvtav1 is absent from the FFmpeg build.
#[derive(Debug, Clone)]
pub struct SvtAv1Options {
    /// Encoder preset: 0 = best quality / slowest, 13 = fastest / lowest quality.
    pub preset: u8,
    /// Log2 number of tile rows (0–6).
    pub tile_rows: u8,
    /// Log2 number of tile columns (0–6).
    pub tile_cols: u8,
    /// Raw SVT-AV1 params string passed verbatim (e.g. `"fast-decode=1:hdr=0"`).
    ///
    /// `None` leaves the encoder default. Invalid values are logged as a warning
    /// and skipped — `build()` never fails due to an unsupported parameter.
    pub svtav1_params: Option<String>,
}

impl Default for SvtAv1Options {
    fn default() -> Self {
        Self {
            preset: 8,
            tile_rows: 0,
            tile_cols: 0,
            svtav1_params: None,
        }
    }
}

// ── VP9 ───────────────────────────────────────────────────────────────────────

/// VP9 (libvpx-vp9) per-codec options.
#[derive(Debug, Clone, Default)]
pub struct Vp9Options {
    /// Encoder speed/quality trade-off: -8 = best quality / slowest, 8 = fastest.
    pub cpu_used: i8,
    /// Constrained Quality level (0–63).
    ///
    /// `Some(q)` enables CQ mode: sets `bit_rate = 0` and applies `q` as the `crf`
    /// option, producing variable-bitrate output governed by perceptual quality.
    /// `None` uses the bitrate mode configured on the builder.
    pub cq_level: Option<u8>,
    /// Log2 number of tile columns (0–6).
    pub tile_columns: u8,
    /// Log2 number of tile rows (0–6).
    pub tile_rows: u8,
    /// Enable row-based multithreading for better CPU utilisation.
    pub row_mt: bool,
}

// ── Apple ProRes ──────────────────────────────────────────────────────────────

/// Apple ProRes encoding profile.
///
/// Controls quality and chroma sampling. 422 profiles use `yuv422p10le`;
/// 4444 profiles use `yuva444p10le`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProResProfile {
    /// 422 Proxy — lowest data rate, offline editing.
    Proxy,
    /// 422 LT — lightweight, good for editing proxies.
    Lt,
    /// 422 Standard — production quality (default).
    #[default]
    Standard,
    /// 422 HQ — high quality, recommended for mastering.
    Hq,
    /// 4444 — full chroma, supports alpha channel.
    P4444,
    /// 4444 XQ — maximum quality 4444 variant.
    P4444Xq,
}

impl ProResProfile {
    /// Returns the integer profile ID passed to `prores_ks` via `av_opt_set`.
    pub(super) fn profile_id(self) -> u8 {
        match self {
            Self::Proxy => 0,
            Self::Lt => 1,
            Self::Standard => 2,
            Self::Hq => 3,
            Self::P4444 => 4,
            Self::P4444Xq => 5,
        }
    }

    /// Returns `true` for 4444 profiles that require `yuva444p10le` pixel format.
    pub(super) fn is_4444(self) -> bool {
        matches!(self, Self::P4444 | Self::P4444Xq)
    }
}

/// Apple ProRes per-codec options.
///
/// Requires an FFmpeg build with `prores_ks` encoder support. Output should
/// use a `.mov` container.
#[derive(Debug, Clone)]
pub struct ProResOptions {
    /// ProRes encoding profile controlling quality and chroma sampling.
    pub profile: ProResProfile,
    /// Optional 4-byte FourCC vendor tag embedded in the stream.
    ///
    /// Set to `Some([b'a', b'p', b'p', b'l'])` to mimic Apple encoders.
    /// `None` leaves the encoder default.
    pub vendor: Option<[u8; 4]>,
}

impl Default for ProResOptions {
    fn default() -> Self {
        Self {
            profile: ProResProfile::Standard,
            vendor: None,
        }
    }
}

// ── Placeholder variants ──────────────────────────────────────────────────────

/// Avid DNxHD / DNxHR per-codec options (reserved for a future issue).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct DnxhdOptions {}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h264_profile_should_return_correct_str() {
        assert_eq!(H264Profile::Baseline.as_str(), "baseline");
        assert_eq!(H264Profile::Main.as_str(), "main");
        assert_eq!(H264Profile::High.as_str(), "high");
        assert_eq!(H264Profile::High10.as_str(), "high10");
    }

    #[test]
    fn h265_profile_should_return_correct_str() {
        assert_eq!(H265Profile::Main.as_str(), "main");
        assert_eq!(H265Profile::Main10.as_str(), "main10");
    }

    #[test]
    fn h265_tier_should_return_correct_str() {
        assert_eq!(H265Tier::Main.as_str(), "main");
        assert_eq!(H265Tier::High.as_str(), "high");
    }

    #[test]
    fn av1_usage_should_return_correct_str() {
        assert_eq!(Av1Usage::VoD.as_str(), "vod");
        assert_eq!(Av1Usage::RealTime.as_str(), "realtime");
    }

    #[test]
    fn h264_options_default_should_have_high_profile() {
        let opts = H264Options::default();
        assert_eq!(opts.profile, H264Profile::High);
        assert_eq!(opts.level, None);
        assert_eq!(opts.bframes, 2);
        assert_eq!(opts.gop_size, 250);
        assert_eq!(opts.refs, 3);
        assert!(opts.preset.is_none());
        assert!(opts.tune.is_none());
    }

    #[test]
    fn h264_preset_should_return_correct_str() {
        assert_eq!(H264Preset::Ultrafast.as_str(), "ultrafast");
        assert_eq!(H264Preset::Superfast.as_str(), "superfast");
        assert_eq!(H264Preset::Veryfast.as_str(), "veryfast");
        assert_eq!(H264Preset::Faster.as_str(), "faster");
        assert_eq!(H264Preset::Fast.as_str(), "fast");
        assert_eq!(H264Preset::Medium.as_str(), "medium");
        assert_eq!(H264Preset::Slow.as_str(), "slow");
        assert_eq!(H264Preset::Slower.as_str(), "slower");
        assert_eq!(H264Preset::Veryslow.as_str(), "veryslow");
        assert_eq!(H264Preset::Placebo.as_str(), "placebo");
    }

    #[test]
    fn h264_tune_should_return_correct_str() {
        assert_eq!(H264Tune::Film.as_str(), "film");
        assert_eq!(H264Tune::Animation.as_str(), "animation");
        assert_eq!(H264Tune::Grain.as_str(), "grain");
        assert_eq!(H264Tune::Stillimage.as_str(), "stillimage");
        assert_eq!(H264Tune::Psnr.as_str(), "psnr");
        assert_eq!(H264Tune::Ssim.as_str(), "ssim");
        assert_eq!(H264Tune::Fastdecode.as_str(), "fastdecode");
        assert_eq!(H264Tune::Zerolatency.as_str(), "zerolatency");
    }

    #[test]
    fn h265_options_default_should_have_main_profile() {
        let opts = H265Options::default();
        assert_eq!(opts.profile, H265Profile::Main);
        assert_eq!(opts.tier, H265Tier::Main);
        assert_eq!(opts.level, None);
        assert!(opts.preset.is_none());
        assert!(opts.x265_params.is_none());
    }

    #[test]
    fn h265_preset_should_be_none_by_default() {
        let opts = H265Options::default();
        assert!(opts.preset.is_none());
    }

    #[test]
    fn h265_x265_params_should_be_none_by_default() {
        let opts = H265Options::default();
        assert!(opts.x265_params.is_none());
    }

    #[test]
    fn av1_options_default_should_have_vod_usage() {
        let opts = Av1Options::default();
        assert_eq!(opts.cpu_used, 4);
        assert_eq!(opts.tile_rows, 0);
        assert_eq!(opts.tile_cols, 0);
        assert_eq!(opts.usage, Av1Usage::VoD);
    }

    #[test]
    fn video_codec_options_enum_variants_are_accessible() {
        let _h264 = VideoCodecOptions::H264(H264Options::default());
        let _h265 = VideoCodecOptions::H265(H265Options::default());
        let _av1 = VideoCodecOptions::Av1(Av1Options::default());
        let _av1svt = VideoCodecOptions::Av1Svt(SvtAv1Options::default());
        let _vp9 = VideoCodecOptions::Vp9(Vp9Options::default());
        let _prores = VideoCodecOptions::ProRes(ProResOptions::default());
        let _dnxhd = VideoCodecOptions::Dnxhd(DnxhdOptions::default());
    }

    #[test]
    fn vp9_options_default_should_have_cpu_used_0() {
        let opts = Vp9Options::default();
        assert_eq!(opts.cpu_used, 0);
        assert_eq!(opts.tile_columns, 0);
        assert_eq!(opts.tile_rows, 0);
        assert!(!opts.row_mt);
    }

    #[test]
    fn vp9_options_default_should_have_no_cq_level() {
        let opts = Vp9Options::default();
        assert!(opts.cq_level.is_none());
    }

    #[test]
    fn svtav1_options_default_should_have_preset_8() {
        let opts = SvtAv1Options::default();
        assert_eq!(opts.preset, 8);
        assert_eq!(opts.tile_rows, 0);
        assert_eq!(opts.tile_cols, 0);
        assert!(opts.svtav1_params.is_none());
    }

    #[test]
    fn prores_options_default_should_have_standard_profile() {
        let opts = ProResOptions::default();
        assert_eq!(opts.profile, ProResProfile::Standard);
        assert!(opts.vendor.is_none());
    }

    #[test]
    fn prores_profile_ids_should_match_spec() {
        assert_eq!(ProResProfile::Proxy.profile_id(), 0);
        assert_eq!(ProResProfile::Lt.profile_id(), 1);
        assert_eq!(ProResProfile::Standard.profile_id(), 2);
        assert_eq!(ProResProfile::Hq.profile_id(), 3);
        assert_eq!(ProResProfile::P4444.profile_id(), 4);
        assert_eq!(ProResProfile::P4444Xq.profile_id(), 5);
    }

    #[test]
    fn prores_profile_is_4444_should_return_true_for_4444_variants() {
        assert!(!ProResProfile::Proxy.is_4444());
        assert!(!ProResProfile::Lt.is_4444());
        assert!(!ProResProfile::Standard.is_4444());
        assert!(!ProResProfile::Hq.is_4444());
        assert!(ProResProfile::P4444.is_4444());
        assert!(ProResProfile::P4444Xq.is_4444());
    }
}
