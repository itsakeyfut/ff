//! H.264 (AVC) per-codec encoding options.

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
    pub(in crate::video) fn as_str(self) -> &'static str {
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
    pub(in crate::video) fn as_str(self) -> &'static str {
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
    pub(in crate::video) fn as_str(self) -> &'static str {
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
}
