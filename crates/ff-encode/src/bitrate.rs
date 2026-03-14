/// Maximum CRF value accepted by H.264/H.265 encoders.
pub const CRF_MAX: u32 = 51;

/// Bitrate control mode for video encoding.
///
/// Passed to [`crate::VideoEncoderBuilder::bitrate_mode`].
#[derive(Debug, Clone, PartialEq)]
pub enum BitrateMode {
    /// Constant bitrate in bits per second.
    Cbr(u64),
    /// Variable bitrate: target average bitrate and hard ceiling (both bps).
    Vbr { target: u64, max: u64 },
    /// Constant rate factor — quality-based (0–51 for H.264/H.265; lower = better).
    Crf(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbr_should_store_bitrate() {
        assert!(matches!(
            BitrateMode::Cbr(5_000_000),
            BitrateMode::Cbr(5_000_000)
        ));
    }

    #[test]
    fn vbr_should_store_target_and_max() {
        let mode = BitrateMode::Vbr {
            target: 4_000_000,
            max: 6_000_000,
        };
        assert!(matches!(
            mode,
            BitrateMode::Vbr {
                target: 4_000_000,
                max: 6_000_000
            }
        ));
    }

    #[test]
    fn crf_should_store_quality_value() {
        assert!(matches!(BitrateMode::Crf(23), BitrateMode::Crf(23)));
    }
}
