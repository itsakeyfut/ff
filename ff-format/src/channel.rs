//! Audio channel layout definitions.
//!
//! This module provides the [`ChannelLayout`] enum for representing
//! common audio channel configurations.
//!
//! # Examples
//!
//! ```
//! use ff_format::channel::ChannelLayout;
//!
//! let stereo = ChannelLayout::Stereo;
//! assert_eq!(stereo.channels(), 2);
//! assert!(stereo.is_stereo());
//!
//! let surround = ChannelLayout::Surround5_1;
//! assert_eq!(surround.channels(), 6);
//! assert!(surround.is_surround());
//! ```

use std::fmt;

/// Audio channel layout representing the speaker configuration.
///
/// This enum covers common channel layouts used in audio/video files.
/// For uncommon layouts, use `Other` with the channel count.
///
/// # Common Layouts
///
/// - **Mono**: Single channel (1.0)
/// - **Stereo**: Left + Right (2.0)
/// - **Surround 5.1**: FL + FR + FC + LFE + BL + BR (standard home theater)
/// - **Surround 7.1**: 5.1 + SL + SR (extended surround)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChannelLayout {
    /// Mono (1 channel)
    Mono,
    /// Stereo (2 channels: Left, Right)
    Stereo,
    /// 2.1 (3 channels: Left, Right, LFE)
    Stereo2_1,
    /// 3.0 (3 channels: Left, Right, Center)
    Surround3_0,
    /// 4.0 Quad (4 channels: FL, FR, BL, BR)
    Quad,
    /// 5.0 (5 channels: FL, FR, FC, BL, BR)
    Surround5_0,
    /// 5.1 (6 channels: FL, FR, FC, LFE, BL, BR)
    Surround5_1,
    /// 6.1 (7 channels: FL, FR, FC, LFE, BC, SL, SR)
    Surround6_1,
    /// 7.1 (8 channels: FL, FR, FC, LFE, BL, BR, SL, SR)
    Surround7_1,
    /// Other layout with specified channel count
    Other(u32),
}

impl ChannelLayout {
    /// Returns the number of audio channels in this layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert_eq!(ChannelLayout::Mono.channels(), 1);
    /// assert_eq!(ChannelLayout::Stereo.channels(), 2);
    /// assert_eq!(ChannelLayout::Surround5_1.channels(), 6);
    /// assert_eq!(ChannelLayout::Surround7_1.channels(), 8);
    /// assert_eq!(ChannelLayout::Other(10).channels(), 10);
    /// ```
    #[must_use]
    pub const fn channels(&self) -> u32 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Stereo2_1 | Self::Surround3_0 => 3,
            Self::Quad => 4,
            Self::Surround5_0 => 5,
            Self::Surround5_1 => 6,
            Self::Surround6_1 => 7,
            Self::Surround7_1 => 8,
            Self::Other(n) => *n,
        }
    }

    /// Returns the layout name as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert_eq!(ChannelLayout::Mono.name(), "mono");
    /// assert_eq!(ChannelLayout::Stereo.name(), "stereo");
    /// assert_eq!(ChannelLayout::Surround5_1.name(), "5.1");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Mono => "mono",
            Self::Stereo => "stereo",
            Self::Stereo2_1 => "2.1",
            Self::Surround3_0 => "3.0",
            Self::Quad => "quad",
            Self::Surround5_0 => "5.0",
            Self::Surround5_1 => "5.1",
            Self::Surround6_1 => "6.1",
            Self::Surround7_1 => "7.1",
            Self::Other(_) => "custom",
        }
    }

    /// Returns `true` if this is a mono layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert!(ChannelLayout::Mono.is_mono());
    /// assert!(!ChannelLayout::Stereo.is_mono());
    /// ```
    #[must_use]
    pub const fn is_mono(&self) -> bool {
        matches!(self, Self::Mono)
    }

    /// Returns `true` if this is a stereo layout.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert!(ChannelLayout::Stereo.is_stereo());
    /// assert!(!ChannelLayout::Mono.is_stereo());
    /// ```
    #[must_use]
    pub const fn is_stereo(&self) -> bool {
        matches!(self, Self::Stereo)
    }

    /// Returns `true` if this is a surround sound layout (more than 2 channels).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert!(ChannelLayout::Surround5_1.is_surround());
    /// assert!(ChannelLayout::Surround7_1.is_surround());
    /// assert!(!ChannelLayout::Stereo.is_surround());
    /// ```
    #[must_use]
    pub const fn is_surround(&self) -> bool {
        matches!(
            self,
            Self::Stereo2_1
                | Self::Surround3_0
                | Self::Quad
                | Self::Surround5_0
                | Self::Surround5_1
                | Self::Surround6_1
                | Self::Surround7_1
        ) || matches!(self, Self::Other(n) if *n > 2)
    }

    /// Returns `true` if this layout includes an LFE (subwoofer) channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert!(ChannelLayout::Stereo2_1.has_lfe());
    /// assert!(ChannelLayout::Surround5_1.has_lfe());
    /// assert!(!ChannelLayout::Surround5_0.has_lfe());
    /// ```
    #[must_use]
    pub const fn has_lfe(&self) -> bool {
        matches!(
            self,
            Self::Stereo2_1 | Self::Surround5_1 | Self::Surround6_1 | Self::Surround7_1
        )
    }

    /// Creates a `ChannelLayout` from a channel count.
    ///
    /// This tries to match common layouts, falling back to `Other` for
    /// uncommon channel counts.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::channel::ChannelLayout;
    ///
    /// assert_eq!(ChannelLayout::from_channels(1), ChannelLayout::Mono);
    /// assert_eq!(ChannelLayout::from_channels(2), ChannelLayout::Stereo);
    /// assert_eq!(ChannelLayout::from_channels(6), ChannelLayout::Surround5_1);
    /// assert_eq!(ChannelLayout::from_channels(10), ChannelLayout::Other(10));
    /// ```
    #[must_use]
    pub const fn from_channels(channels: u32) -> Self {
        match channels {
            1 => Self::Mono,
            2 => Self::Stereo,
            // 3 channels could be either 2.1 or 3.0, default to stereo + LFE
            3 => Self::Stereo2_1,
            4 => Self::Quad,
            5 => Self::Surround5_0,
            6 => Self::Surround5_1,
            7 => Self::Surround6_1,
            8 => Self::Surround7_1,
            n => Self::Other(n),
        }
    }
}

impl Default for ChannelLayout {
    /// Returns the default channel layout (Stereo).
    fn default() -> Self {
        Self::Stereo
    }
}

impl fmt::Display for ChannelLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl From<u32> for ChannelLayout {
    fn from(channels: u32) -> Self {
        Self::from_channels(channels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_count() {
        assert_eq!(ChannelLayout::Mono.channels(), 1);
        assert_eq!(ChannelLayout::Stereo.channels(), 2);
        assert_eq!(ChannelLayout::Stereo2_1.channels(), 3);
        assert_eq!(ChannelLayout::Surround3_0.channels(), 3);
        assert_eq!(ChannelLayout::Quad.channels(), 4);
        assert_eq!(ChannelLayout::Surround5_0.channels(), 5);
        assert_eq!(ChannelLayout::Surround5_1.channels(), 6);
        assert_eq!(ChannelLayout::Surround6_1.channels(), 7);
        assert_eq!(ChannelLayout::Surround7_1.channels(), 8);
        assert_eq!(ChannelLayout::Other(16).channels(), 16);
    }

    #[test]
    fn test_names() {
        assert_eq!(ChannelLayout::Mono.name(), "mono");
        assert_eq!(ChannelLayout::Stereo.name(), "stereo");
        assert_eq!(ChannelLayout::Stereo2_1.name(), "2.1");
        assert_eq!(ChannelLayout::Surround3_0.name(), "3.0");
        assert_eq!(ChannelLayout::Quad.name(), "quad");
        assert_eq!(ChannelLayout::Surround5_0.name(), "5.0");
        assert_eq!(ChannelLayout::Surround5_1.name(), "5.1");
        assert_eq!(ChannelLayout::Surround6_1.name(), "6.1");
        assert_eq!(ChannelLayout::Surround7_1.name(), "7.1");
        assert_eq!(ChannelLayout::Other(10).name(), "custom");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ChannelLayout::Mono), "mono");
        assert_eq!(format!("{}", ChannelLayout::Surround5_1), "5.1");
        assert_eq!(format!("{}", ChannelLayout::Other(10)), "custom");
    }

    #[test]
    fn test_default() {
        assert_eq!(ChannelLayout::default(), ChannelLayout::Stereo);
    }

    #[test]
    fn test_is_mono_stereo() {
        assert!(ChannelLayout::Mono.is_mono());
        assert!(!ChannelLayout::Stereo.is_mono());
        assert!(!ChannelLayout::Surround5_1.is_mono());

        assert!(ChannelLayout::Stereo.is_stereo());
        assert!(!ChannelLayout::Mono.is_stereo());
        assert!(!ChannelLayout::Surround5_1.is_stereo());
    }

    #[test]
    fn test_is_surround() {
        assert!(!ChannelLayout::Mono.is_surround());
        assert!(!ChannelLayout::Stereo.is_surround());
        assert!(ChannelLayout::Stereo2_1.is_surround());
        assert!(ChannelLayout::Surround3_0.is_surround());
        assert!(ChannelLayout::Quad.is_surround());
        assert!(ChannelLayout::Surround5_0.is_surround());
        assert!(ChannelLayout::Surround5_1.is_surround());
        assert!(ChannelLayout::Surround6_1.is_surround());
        assert!(ChannelLayout::Surround7_1.is_surround());

        // Other with > 2 channels is surround
        assert!(ChannelLayout::Other(4).is_surround());
        // Other with <= 2 channels is not surround
        assert!(!ChannelLayout::Other(2).is_surround());
    }

    #[test]
    fn test_has_lfe() {
        assert!(!ChannelLayout::Mono.has_lfe());
        assert!(!ChannelLayout::Stereo.has_lfe());
        assert!(ChannelLayout::Stereo2_1.has_lfe());
        assert!(!ChannelLayout::Surround3_0.has_lfe());
        assert!(!ChannelLayout::Quad.has_lfe());
        assert!(!ChannelLayout::Surround5_0.has_lfe());
        assert!(ChannelLayout::Surround5_1.has_lfe());
        assert!(ChannelLayout::Surround6_1.has_lfe());
        assert!(ChannelLayout::Surround7_1.has_lfe());
    }

    #[test]
    fn test_from_channels() {
        assert_eq!(ChannelLayout::from_channels(1), ChannelLayout::Mono);
        assert_eq!(ChannelLayout::from_channels(2), ChannelLayout::Stereo);
        assert_eq!(ChannelLayout::from_channels(3), ChannelLayout::Stereo2_1);
        assert_eq!(ChannelLayout::from_channels(4), ChannelLayout::Quad);
        assert_eq!(ChannelLayout::from_channels(5), ChannelLayout::Surround5_0);
        assert_eq!(ChannelLayout::from_channels(6), ChannelLayout::Surround5_1);
        assert_eq!(ChannelLayout::from_channels(7), ChannelLayout::Surround6_1);
        assert_eq!(ChannelLayout::from_channels(8), ChannelLayout::Surround7_1);
        assert_eq!(ChannelLayout::from_channels(10), ChannelLayout::Other(10));
    }

    #[test]
    fn test_from_u32() {
        let layout: ChannelLayout = 2u32.into();
        assert_eq!(layout, ChannelLayout::Stereo);

        let layout: ChannelLayout = 6u32.into();
        assert_eq!(layout, ChannelLayout::Surround5_1);
    }

    #[test]
    fn test_debug() {
        assert_eq!(format!("{:?}", ChannelLayout::Mono), "Mono");
        assert_eq!(format!("{:?}", ChannelLayout::Surround5_1), "Surround5_1");
        assert_eq!(format!("{:?}", ChannelLayout::Other(10)), "Other(10)");
    }

    #[test]
    fn test_equality_and_hash() {
        use std::collections::HashSet;

        assert_eq!(ChannelLayout::Stereo, ChannelLayout::Stereo);
        assert_ne!(ChannelLayout::Stereo, ChannelLayout::Mono);
        assert_eq!(ChannelLayout::Other(4), ChannelLayout::Other(4));
        assert_ne!(ChannelLayout::Other(4), ChannelLayout::Other(5));

        let mut set = HashSet::new();
        set.insert(ChannelLayout::Stereo);
        set.insert(ChannelLayout::Surround5_1);
        assert!(set.contains(&ChannelLayout::Stereo));
        assert!(!set.contains(&ChannelLayout::Mono));
    }

    #[test]
    fn test_copy() {
        let layout = ChannelLayout::Surround5_1;
        let copied = layout;
        assert_eq!(layout, copied);
        assert_eq!(layout.channels(), copied.channels());
    }
}
