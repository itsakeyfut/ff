//! Color space and related type definitions.
//!
//! This module provides enums for color-related metadata commonly found
//! in video streams, including color space, color range, and color primaries.
//!
//! # Examples
//!
//! ```
//! use ff_format::color::{ColorSpace, ColorRange, ColorPrimaries};
//!
//! // HD video typically uses BT.709
//! let space = ColorSpace::Bt709;
//! let range = ColorRange::Limited;
//! let primaries = ColorPrimaries::Bt709;
//!
//! assert!(space.is_hd());
//! assert!(!range.is_full());
//! ```

use std::fmt;

/// Color space (matrix coefficients) for YUV to RGB conversion.
///
/// The color space defines how YUV values are converted to RGB and vice versa.
/// Different standards use different matrix coefficients for this conversion.
///
/// # Common Usage
///
/// - **BT.709**: HD content (720p, 1080p)
/// - **BT.601**: SD content (480i, 576i)
/// - **BT.2020**: UHD/HDR content (4K, 8K)
/// - **sRGB**: Computer graphics, web content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ColorSpace {
    /// ITU-R BT.709 - HD television standard (most common for HD video)
    #[default]
    Bt709,
    /// ITU-R BT.601 - SD television standard
    Bt601,
    /// ITU-R BT.2020 - UHD/HDR television standard
    Bt2020,
    /// sRGB color space - computer graphics and web
    Srgb,
    /// Color space is not specified or unknown
    Unknown,
}

impl ColorSpace {
    /// Returns the name of the color space as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorSpace;
    ///
    /// assert_eq!(ColorSpace::Bt709.name(), "bt709");
    /// assert_eq!(ColorSpace::Bt601.name(), "bt601");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Bt709 => "bt709",
            Self::Bt601 => "bt601",
            Self::Bt2020 => "bt2020",
            Self::Srgb => "srgb",
            Self::Unknown => "unknown",
        }
    }

    /// Returns `true` if this is an HD color space (BT.709).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorSpace;
    ///
    /// assert!(ColorSpace::Bt709.is_hd());
    /// assert!(!ColorSpace::Bt601.is_hd());
    /// ```
    #[must_use]
    pub const fn is_hd(&self) -> bool {
        matches!(self, Self::Bt709)
    }

    /// Returns `true` if this is an SD color space (BT.601).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorSpace;
    ///
    /// assert!(ColorSpace::Bt601.is_sd());
    /// assert!(!ColorSpace::Bt709.is_sd());
    /// ```
    #[must_use]
    pub const fn is_sd(&self) -> bool {
        matches!(self, Self::Bt601)
    }

    /// Returns `true` if this is a UHD/HDR color space (BT.2020).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorSpace;
    ///
    /// assert!(ColorSpace::Bt2020.is_uhd());
    /// assert!(!ColorSpace::Bt709.is_uhd());
    /// ```
    #[must_use]
    pub const fn is_uhd(&self) -> bool {
        matches!(self, Self::Bt2020)
    }

    /// Returns `true` if the color space is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorSpace;
    ///
    /// assert!(ColorSpace::Unknown.is_unknown());
    /// assert!(!ColorSpace::Bt709.is_unknown());
    /// ```
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl fmt::Display for ColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Color range defining the valid range of color values.
///
/// Video typically uses "limited" range where black is at level 16 and white
/// at level 235 (for 8-bit). Computer graphics typically use "full" range
/// where black is 0 and white is 255.
///
/// # Common Usage
///
/// - **Limited**: Broadcast video, Blu-ray, streaming services
/// - **Full**: Computer graphics, screenshots, game capture
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ColorRange {
    /// Limited/TV range (16-235 for Y, 16-240 for UV in 8-bit)
    #[default]
    Limited,
    /// Full/PC range (0-255 for all components in 8-bit)
    Full,
    /// Color range is not specified or unknown
    Unknown,
}

impl ColorRange {
    /// Returns the name of the color range as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorRange;
    ///
    /// assert_eq!(ColorRange::Limited.name(), "limited");
    /// assert_eq!(ColorRange::Full.name(), "full");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Limited => "limited",
            Self::Full => "full",
            Self::Unknown => "unknown",
        }
    }

    /// Returns `true` if this is full (PC) range.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorRange;
    ///
    /// assert!(ColorRange::Full.is_full());
    /// assert!(!ColorRange::Limited.is_full());
    /// ```
    #[must_use]
    pub const fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Returns `true` if this is limited (TV) range.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorRange;
    ///
    /// assert!(ColorRange::Limited.is_limited());
    /// assert!(!ColorRange::Full.is_limited());
    /// ```
    #[must_use]
    pub const fn is_limited(&self) -> bool {
        matches!(self, Self::Limited)
    }

    /// Returns `true` if the color range is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorRange;
    ///
    /// assert!(ColorRange::Unknown.is_unknown());
    /// assert!(!ColorRange::Limited.is_unknown());
    /// ```
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    /// Returns the minimum value for luma (Y) in 8-bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorRange;
    ///
    /// assert_eq!(ColorRange::Limited.luma_min_8bit(), 16);
    /// assert_eq!(ColorRange::Full.luma_min_8bit(), 0);
    /// ```
    #[must_use]
    pub const fn luma_min_8bit(&self) -> u8 {
        match self {
            Self::Limited => 16,
            Self::Full | Self::Unknown => 0,
        }
    }

    /// Returns the maximum value for luma (Y) in 8-bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorRange;
    ///
    /// assert_eq!(ColorRange::Limited.luma_max_8bit(), 235);
    /// assert_eq!(ColorRange::Full.luma_max_8bit(), 255);
    /// ```
    #[must_use]
    pub const fn luma_max_8bit(&self) -> u8 {
        match self {
            Self::Limited => 235,
            Self::Full | Self::Unknown => 255,
        }
    }
}

impl fmt::Display for ColorRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Color primaries defining the color gamut (the range of colors that can be represented).
///
/// Different standards define different primary colors (red, green, blue points)
/// which determine the overall range of colors that can be displayed.
///
/// # Common Usage
///
/// - **BT.709**: HD content, same as sRGB primaries
/// - **BT.601**: SD content (NTSC or PAL)
/// - **BT.2020**: Wide color gamut for UHD/HDR
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ColorPrimaries {
    /// ITU-R BT.709 primaries (same as sRGB, most common)
    #[default]
    Bt709,
    /// ITU-R BT.601 primaries (SD video)
    Bt601,
    /// ITU-R BT.2020 primaries (wide color gamut for UHD/HDR)
    Bt2020,
    /// Color primaries are not specified or unknown
    Unknown,
}

impl ColorPrimaries {
    /// Returns the name of the color primaries as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorPrimaries;
    ///
    /// assert_eq!(ColorPrimaries::Bt709.name(), "bt709");
    /// assert_eq!(ColorPrimaries::Bt2020.name(), "bt2020");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Bt709 => "bt709",
            Self::Bt601 => "bt601",
            Self::Bt2020 => "bt2020",
            Self::Unknown => "unknown",
        }
    }

    /// Returns `true` if this uses wide color gamut (BT.2020).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorPrimaries;
    ///
    /// assert!(ColorPrimaries::Bt2020.is_wide_gamut());
    /// assert!(!ColorPrimaries::Bt709.is_wide_gamut());
    /// ```
    #[must_use]
    pub const fn is_wide_gamut(&self) -> bool {
        matches!(self, Self::Bt2020)
    }

    /// Returns `true` if the color primaries are unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::color::ColorPrimaries;
    ///
    /// assert!(ColorPrimaries::Unknown.is_unknown());
    /// assert!(!ColorPrimaries::Bt709.is_unknown());
    /// ```
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl fmt::Display for ColorPrimaries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod color_space_tests {
        use super::*;

        #[test]
        fn test_names() {
            assert_eq!(ColorSpace::Bt709.name(), "bt709");
            assert_eq!(ColorSpace::Bt601.name(), "bt601");
            assert_eq!(ColorSpace::Bt2020.name(), "bt2020");
            assert_eq!(ColorSpace::Srgb.name(), "srgb");
            assert_eq!(ColorSpace::Unknown.name(), "unknown");
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", ColorSpace::Bt709), "bt709");
            assert_eq!(format!("{}", ColorSpace::Bt2020), "bt2020");
        }

        #[test]
        fn test_default() {
            assert_eq!(ColorSpace::default(), ColorSpace::Bt709);
        }

        #[test]
        fn test_is_hd_sd_uhd() {
            assert!(ColorSpace::Bt709.is_hd());
            assert!(!ColorSpace::Bt709.is_sd());
            assert!(!ColorSpace::Bt709.is_uhd());

            assert!(!ColorSpace::Bt601.is_hd());
            assert!(ColorSpace::Bt601.is_sd());
            assert!(!ColorSpace::Bt601.is_uhd());

            assert!(!ColorSpace::Bt2020.is_hd());
            assert!(!ColorSpace::Bt2020.is_sd());
            assert!(ColorSpace::Bt2020.is_uhd());
        }

        #[test]
        fn test_is_unknown() {
            assert!(ColorSpace::Unknown.is_unknown());
            assert!(!ColorSpace::Bt709.is_unknown());
        }

        #[test]
        fn test_debug() {
            assert_eq!(format!("{:?}", ColorSpace::Bt709), "Bt709");
            assert_eq!(format!("{:?}", ColorSpace::Srgb), "Srgb");
        }

        #[test]
        fn test_equality_and_hash() {
            use std::collections::HashSet;

            assert_eq!(ColorSpace::Bt709, ColorSpace::Bt709);
            assert_ne!(ColorSpace::Bt709, ColorSpace::Bt601);

            let mut set = HashSet::new();
            set.insert(ColorSpace::Bt709);
            set.insert(ColorSpace::Bt601);
            assert!(set.contains(&ColorSpace::Bt709));
            assert!(!set.contains(&ColorSpace::Bt2020));
        }

        #[test]
        fn test_copy() {
            let space = ColorSpace::Bt709;
            let copied = space;
            assert_eq!(space, copied);
        }
    }

    mod color_range_tests {
        use super::*;

        #[test]
        fn test_names() {
            assert_eq!(ColorRange::Limited.name(), "limited");
            assert_eq!(ColorRange::Full.name(), "full");
            assert_eq!(ColorRange::Unknown.name(), "unknown");
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", ColorRange::Limited), "limited");
            assert_eq!(format!("{}", ColorRange::Full), "full");
        }

        #[test]
        fn test_default() {
            assert_eq!(ColorRange::default(), ColorRange::Limited);
        }

        #[test]
        fn test_is_full_limited() {
            assert!(ColorRange::Full.is_full());
            assert!(!ColorRange::Full.is_limited());

            assert!(!ColorRange::Limited.is_full());
            assert!(ColorRange::Limited.is_limited());
        }

        #[test]
        fn test_is_unknown() {
            assert!(ColorRange::Unknown.is_unknown());
            assert!(!ColorRange::Limited.is_unknown());
        }

        #[test]
        fn test_luma_values() {
            assert_eq!(ColorRange::Limited.luma_min_8bit(), 16);
            assert_eq!(ColorRange::Limited.luma_max_8bit(), 235);

            assert_eq!(ColorRange::Full.luma_min_8bit(), 0);
            assert_eq!(ColorRange::Full.luma_max_8bit(), 255);

            assert_eq!(ColorRange::Unknown.luma_min_8bit(), 0);
            assert_eq!(ColorRange::Unknown.luma_max_8bit(), 255);
        }

        #[test]
        fn test_equality_and_hash() {
            use std::collections::HashSet;

            assert_eq!(ColorRange::Limited, ColorRange::Limited);
            assert_ne!(ColorRange::Limited, ColorRange::Full);

            let mut set = HashSet::new();
            set.insert(ColorRange::Limited);
            set.insert(ColorRange::Full);
            assert!(set.contains(&ColorRange::Limited));
            assert!(!set.contains(&ColorRange::Unknown));
        }
    }

    mod color_primaries_tests {
        use super::*;

        #[test]
        fn test_names() {
            assert_eq!(ColorPrimaries::Bt709.name(), "bt709");
            assert_eq!(ColorPrimaries::Bt601.name(), "bt601");
            assert_eq!(ColorPrimaries::Bt2020.name(), "bt2020");
            assert_eq!(ColorPrimaries::Unknown.name(), "unknown");
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", ColorPrimaries::Bt709), "bt709");
            assert_eq!(format!("{}", ColorPrimaries::Bt2020), "bt2020");
        }

        #[test]
        fn test_default() {
            assert_eq!(ColorPrimaries::default(), ColorPrimaries::Bt709);
        }

        #[test]
        fn test_is_wide_gamut() {
            assert!(ColorPrimaries::Bt2020.is_wide_gamut());
            assert!(!ColorPrimaries::Bt709.is_wide_gamut());
            assert!(!ColorPrimaries::Bt601.is_wide_gamut());
        }

        #[test]
        fn test_is_unknown() {
            assert!(ColorPrimaries::Unknown.is_unknown());
            assert!(!ColorPrimaries::Bt709.is_unknown());
        }

        #[test]
        fn test_equality_and_hash() {
            use std::collections::HashSet;

            assert_eq!(ColorPrimaries::Bt709, ColorPrimaries::Bt709);
            assert_ne!(ColorPrimaries::Bt709, ColorPrimaries::Bt2020);

            let mut set = HashSet::new();
            set.insert(ColorPrimaries::Bt709);
            set.insert(ColorPrimaries::Bt2020);
            assert!(set.contains(&ColorPrimaries::Bt709));
            assert!(!set.contains(&ColorPrimaries::Bt601));
        }
    }
}
