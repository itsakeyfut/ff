//! Seek mode configuration for decoder positioning.

/// Seek mode for positioning the decoder.
///
/// This enum determines how seeking is performed when navigating
/// through a media file.
///
/// # Performance Considerations
///
/// - [`Keyframe`](Self::Keyframe) is fastest but may land slightly before or after the target
/// - [`Exact`](Self::Exact) is slower but guarantees landing on the exact frame
/// - [`Backward`](Self::Backward) is useful for editing workflows where the previous keyframe is needed
///
/// # Examples
///
/// ```
/// use ff_decode::SeekMode;
///
/// // Default is Keyframe mode
/// let mode = SeekMode::default();
/// assert_eq!(mode, SeekMode::Keyframe);
///
/// // Use exact mode for frame-accurate positioning
/// let exact = SeekMode::Exact;
/// assert_eq!(format!("{:?}", exact), "Exact");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SeekMode {
    /// Seek to nearest keyframe (fast, may have small offset).
    ///
    /// This mode seeks to the closest keyframe to the target position.
    /// It's the fastest option but the actual position may differ from
    /// the requested position by up to the GOP (Group of Pictures) size.
    #[default]
    Keyframe = 0,

    /// Seek to exact frame (slower but precise).
    ///
    /// This mode first seeks to the previous keyframe, then decodes
    /// frames until reaching the exact target position. This guarantees
    /// frame-accurate positioning but is slower, especially for long GOPs.
    Exact = 1,

    /// Seek to keyframe at or before the target position.
    ///
    /// Similar to [`Keyframe`](Self::Keyframe), but guarantees the resulting
    /// position is at or before the target. Useful for editing workflows
    /// where you need to start decoding before a specific point.
    Backward = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_mode_default_should_be_keyframe() {
        let mode = SeekMode::default();
        assert_eq!(mode, SeekMode::Keyframe);
    }

    #[test]
    fn seek_mode_variants_should_be_accessible() {
        let modes = [SeekMode::Keyframe, SeekMode::Exact, SeekMode::Backward];
        for mode in modes {
            let _ = format!("{mode:?}");
        }
    }
}
