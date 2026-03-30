//! Encoding preset definitions.

/// Encoding preset (speed vs quality tradeoff).
///
/// Presets control the speed/quality balance during encoding:
/// - Faster presets = faster encoding, lower quality, larger file size
/// - Slower presets = slower encoding, higher quality, smaller file size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Preset {
    /// Ultra fast (low quality)
    Ultrafast,

    /// Faster
    Faster,

    /// Fast
    Fast,

    /// Medium (balanced)
    #[default]
    Medium,

    /// Slow (high quality)
    Slow,

    /// Slower
    Slower,

    /// Very slow (highest quality)
    Veryslow,
}

impl Preset {
    /// Convert to `FFmpeg` preset string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_as_str() {
        assert_eq!(Preset::Ultrafast.as_str(), "ultrafast");
        assert_eq!(Preset::Medium.as_str(), "medium");
        assert_eq!(Preset::Veryslow.as_str(), "veryslow");
    }

    #[test]
    fn test_default_preset() {
        assert_eq!(Preset::default(), Preset::Medium);
    }
}
