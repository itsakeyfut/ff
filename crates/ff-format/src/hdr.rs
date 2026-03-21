//! HDR metadata types for high-dynamic-range video.
//!
//! This module provides [`Hdr10Metadata`] and [`MasteringDisplay`] for HDR10
//! static metadata embedding.

/// HDR10 static metadata (`MaxCLL` + `MaxFALL` + mastering display).
///
/// Pass to `VideoEncoderBuilder::hdr10_metadata` (in `ff-encode`) to embed
/// HDR10 static metadata in the encoded output using
/// `AV_PKT_DATA_CONTENT_LIGHT_LEVEL` and
/// `AV_PKT_DATA_MASTERING_DISPLAY_METADATA` packet side data.
///
/// Setting this automatically configures the codec context with:
/// - `color_primaries = BT.2020`
/// - `color_trc = SMPTE ST 2084 (PQ)`
/// - `colorspace = BT.2020 NCL`
#[derive(Debug, Clone)]
pub struct Hdr10Metadata {
    /// Maximum Content Light Level in nits (e.g. 1000).
    pub max_cll: u16,
    /// Maximum Frame-Average Light Level in nits (e.g. 400).
    pub max_fall: u16,
    /// Mastering display colour volume (SMPTE ST 2086).
    pub mastering_display: MasteringDisplay,
}

/// Mastering display colour volume (SMPTE ST 2086).
///
/// Chromaticity coordinates use a denominator of 50000 (each value represents
/// `n / 50000` in CIE 1931 xy).  Luminance values use a denominator of 10000
/// (each value represents `n / 10000` nits).
///
/// # Examples
///
/// BT.2020 D65 primaries / 1000 nit peak / 0.005 nit black:
///
/// ```
/// use ff_format::hdr::MasteringDisplay;
///
/// let md = MasteringDisplay {
///     red_x: 17000,   red_y: 8500,
///     green_x: 13250, green_y: 34500,
///     blue_x: 7500,   blue_y: 3000,
///     white_x: 15635, white_y: 16450,
///     min_luminance: 50,          // 0.005 nits
///     max_luminance: 10_000_000,  // 1000 nits
/// };
/// ```
#[derive(Debug, Clone)]
pub struct MasteringDisplay {
    /// Red primary x coordinate (×50000).
    pub red_x: u16,
    /// Red primary y coordinate (×50000).
    pub red_y: u16,
    /// Green primary x coordinate (×50000).
    pub green_x: u16,
    /// Green primary y coordinate (×50000).
    pub green_y: u16,
    /// Blue primary x coordinate (×50000).
    pub blue_x: u16,
    /// Blue primary y coordinate (×50000).
    pub blue_y: u16,
    /// White point x coordinate (×50000).
    pub white_x: u16,
    /// White point y coordinate (×50000).
    pub white_y: u16,
    /// Minimum display luminance (×10000, in nits).
    pub min_luminance: u32,
    /// Maximum display luminance (×10000, in nits).
    pub max_luminance: u32,
}
