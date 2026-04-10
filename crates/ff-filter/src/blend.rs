//! Blend mode definitions for video compositing operations.

// ── BlendMode ────────────────────────────────────────────────────────────────

/// Specifies how two video layers are combined during compositing.
///
/// Variants are grouped into two families:
///
/// - **Photographic blend modes** (18) — operate on pixel values; both layers
///   are typically opaque.  Implemented via `FFmpeg`'s `blend` filter with the
///   `all_mode` option, except [`BlendMode::Normal`] which uses `overlay`.
///
/// - **Porter-Duff alpha compositing** (6) — operate on the alpha channel;
///   at least the top layer must carry an alpha channel (e.g. `rgba` or
///   `yuva420p` pixel format).
///
/// # Implementation status
///
/// Only [`BlendMode::Normal`] is fully implemented in the current version
/// (issue #327).  All other variants are defined and accepted by the builder
/// but return [`FilterError::InvalidConfig`](crate::FilterError::InvalidConfig) when
/// [`FilterGraphBuilder::build`](crate::FilterGraphBuilder::build) is called, until their dedicated
/// issues are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    // ── Photographic blend modes ──────────────────────────────────────────
    /// Standard alpha-over composite (top * opacity + bottom * (1 − opacity)).
    ///
    /// Implemented via `FFmpeg`'s `overlay=format=auto:shortest=1`.
    Normal,

    /// Multiply per-channel pixel values; darkens the result.
    ///
    /// Maps to `blend all_mode=multiply`.
    Multiply,

    /// Inverse of multiply; lightens the result.
    ///
    /// Maps to `blend all_mode=screen`.
    Screen,

    /// Combines Multiply and Screen based on base-layer luminance.
    ///
    /// Maps to `blend all_mode=overlay`.
    Overlay,

    /// Gentle contrast enhancement; 50 % gray top layer is identity.
    ///
    /// Maps to `blend all_mode=softlight`.
    SoftLight,

    /// Harsher version of Overlay; driven by the top layer's luminance.
    ///
    /// Maps to `blend all_mode=hardlight`.
    HardLight,

    /// Brightens the base by dividing it by the inverse of the blend.
    ///
    /// Maps to `blend all_mode=dodge`.
    ColorDodge,

    /// Darkens the base; inverse of Color Dodge.
    ///
    /// Maps to `blend all_mode=burn`.
    ColorBurn,

    /// Retains the darker of the two pixels per channel.
    ///
    /// Maps to `blend all_mode=darken`.
    Darken,

    /// Retains the lighter of the two pixels per channel.
    ///
    /// Maps to `blend all_mode=lighten`.
    Lighten,

    /// Per-channel absolute difference.  Useful for alignment verification.
    ///
    /// Maps to `blend all_mode=difference`.
    Difference,

    /// Similar to Difference but with lower contrast in mid-tones.
    ///
    /// Maps to `blend all_mode=exclusion`.
    Exclusion,

    /// Linear addition, clamped at maximum.
    ///
    /// Maps to `blend all_mode=addition`.
    Add,

    /// Linear subtraction, clamped at minimum.
    ///
    /// Maps to `blend all_mode=subtract`.
    Subtract,

    /// Applies the top layer's hue to the base's saturation and luminance.
    ///
    /// Maps to `blend all_mode=hue`.
    Hue,

    /// Applies the top layer's saturation to the base's hue and luminance.
    ///
    /// Maps to `blend all_mode=saturation`.
    Saturation,

    /// Applies the top layer's hue + saturation to the base's luminance.
    ///
    /// Maps to `blend all_mode=color`.
    Color,

    /// Applies the top layer's luminance to the base's hue and saturation.
    ///
    /// Maps to `blend all_mode=luminosity`.
    Luminosity,

    // ── Porter-Duff alpha compositing ─────────────────────────────────────
    /// Top layer rendered over the bottom (standard alpha compositing).
    ///
    /// Implemented via `overlay=format=auto:shortest=1`.
    PorterDuffOver,

    /// Bottom layer rendered over the top; equivalent to `Over` with inputs swapped.
    PorterDuffUnder,

    /// Top layer masked by the bottom layer's alpha (intersection).
    PorterDuffIn,

    /// Top layer visible only where the bottom layer is transparent.
    PorterDuffOut,

    /// Top layer placed atop the bottom; visible only where the bottom is opaque.
    PorterDuffAtop,

    /// Pixels from exactly one layer (XOR of opaque regions).
    PorterDuffXor,
}
