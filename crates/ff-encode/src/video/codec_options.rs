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
    /// AV1 encoding options.
    Av1(Av1Options),
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
}

impl Default for H264Options {
    fn default() -> Self {
        Self {
            profile: H264Profile::High,
            level: None,
            bframes: 2,
            gop_size: 250,
            refs: 3,
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
}

impl Default for H265Options {
    fn default() -> Self {
        Self {
            profile: H265Profile::Main,
            tier: H265Tier::Main,
            level: None,
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

// ── Placeholder variants ──────────────────────────────────────────────────────

/// VP9 per-codec options (reserved for a future issue).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Vp9Options {}

/// Apple ProRes per-codec options (reserved for a future issue).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ProResOptions {}

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
    }

    #[test]
    fn h265_options_default_should_have_main_profile() {
        let opts = H265Options::default();
        assert_eq!(opts.profile, H265Profile::Main);
        assert_eq!(opts.tier, H265Tier::Main);
        assert_eq!(opts.level, None);
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
        let _vp9 = VideoCodecOptions::Vp9(Vp9Options::default());
        let _prores = VideoCodecOptions::ProRes(ProResOptions::default());
        let _dnxhd = VideoCodecOptions::Dnxhd(DnxhdOptions::default());
    }
}
