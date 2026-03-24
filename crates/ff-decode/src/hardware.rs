//! Hardware acceleration configuration for video decoding.

/// Hardware acceleration configuration.
///
/// This enum specifies which hardware acceleration method to use for
/// video decoding. Hardware acceleration can significantly improve
/// decoding performance, especially for high-resolution content.
///
/// # Platform Support
///
/// | Mode | Platform | GPU Required |
/// |------|----------|--------------|
/// | [`Nvdec`](Self::Nvdec) | Windows/Linux | NVIDIA |
/// | [`Qsv`](Self::Qsv) | Windows/Linux | Intel |
/// | [`Amf`](Self::Amf) | Windows/Linux | AMD |
/// | [`VideoToolbox`](Self::VideoToolbox) | macOS/iOS | Any |
/// | [`Vaapi`](Self::Vaapi) | Linux | Various |
///
/// # Fallback Behavior
///
/// When [`Auto`](Self::Auto) is used, the decoder will try available
/// accelerators in order of preference and fall back to software
/// decoding if none are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HardwareAccel {
    /// Automatically detect and use available hardware.
    ///
    /// The decoder will probe for available hardware accelerators
    /// and use the best one available. Falls back to software decoding
    /// if no hardware acceleration is available.
    #[default]
    Auto,

    /// Disable hardware acceleration (CPU only).
    ///
    /// Forces software decoding using the CPU. This may be useful for
    /// debugging, consistency, or when hardware acceleration causes issues.
    None,

    /// NVIDIA NVDEC.
    ///
    /// Uses NVIDIA's dedicated video decoding hardware. Supports most
    /// common codecs including H.264, H.265, VP9, and AV1 (on newer GPUs).
    /// Requires an NVIDIA GPU with NVDEC support.
    Nvdec,

    /// Intel Quick Sync Video.
    ///
    /// Uses Intel's integrated GPU video engine. Available on most
    /// Intel CPUs with integrated graphics. Supports H.264, H.265,
    /// VP9, and AV1 (on newer platforms).
    Qsv,

    /// AMD Advanced Media Framework.
    ///
    /// Uses AMD's dedicated video decoding hardware. Available on AMD
    /// GPUs and APUs. Supports H.264, H.265, and VP9.
    Amf,

    /// Apple `VideoToolbox`.
    ///
    /// Uses Apple's hardware video decoding on macOS and iOS. Works with
    /// both Intel and Apple Silicon Macs. Supports H.264, H.265, and `ProRes`.
    VideoToolbox,

    /// Video Acceleration API (Linux).
    ///
    /// A Linux-specific API that provides hardware-accelerated video
    /// decoding across different GPU vendors. Widely supported on
    /// Intel, AMD, and NVIDIA GPUs on Linux.
    Vaapi,
}

impl HardwareAccel {
    /// Returns `true` if this represents an enabled hardware accelerator.
    ///
    /// Returns `false` for [`None`](Self::None) and [`Auto`](Self::Auto).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::HardwareAccel;
    ///
    /// assert!(!HardwareAccel::Auto.is_specific());
    /// assert!(!HardwareAccel::None.is_specific());
    /// assert!(HardwareAccel::Nvdec.is_specific());
    /// assert!(HardwareAccel::Qsv.is_specific());
    /// ```
    #[must_use]
    pub const fn is_specific(&self) -> bool {
        !matches!(self, Self::Auto | Self::None)
    }

    /// Returns the name of the hardware accelerator.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::HardwareAccel;
    ///
    /// assert_eq!(HardwareAccel::Auto.name(), "auto");
    /// assert_eq!(HardwareAccel::Nvdec.name(), "nvdec");
    /// assert_eq!(HardwareAccel::VideoToolbox.name(), "videotoolbox");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Nvdec => "nvdec",
            Self::Qsv => "qsv",
            Self::Amf => "amf",
            Self::VideoToolbox => "videotoolbox",
            Self::Vaapi => "vaapi",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_accel_default_should_be_auto() {
        let accel = HardwareAccel::default();
        assert_eq!(accel, HardwareAccel::Auto);
    }

    #[test]
    fn hardware_accel_is_specific_should_return_false_for_auto_and_none() {
        assert!(!HardwareAccel::Auto.is_specific());
        assert!(!HardwareAccel::None.is_specific());
    }

    #[test]
    fn hardware_accel_is_specific_should_return_true_for_explicit_accelerators() {
        assert!(HardwareAccel::Nvdec.is_specific());
        assert!(HardwareAccel::Qsv.is_specific());
        assert!(HardwareAccel::Amf.is_specific());
        assert!(HardwareAccel::VideoToolbox.is_specific());
        assert!(HardwareAccel::Vaapi.is_specific());
    }

    #[test]
    fn hardware_accel_name_should_return_correct_strings() {
        assert_eq!(HardwareAccel::Auto.name(), "auto");
        assert_eq!(HardwareAccel::None.name(), "none");
        assert_eq!(HardwareAccel::Nvdec.name(), "nvdec");
        assert_eq!(HardwareAccel::Qsv.name(), "qsv");
        assert_eq!(HardwareAccel::Amf.name(), "amf");
        assert_eq!(HardwareAccel::VideoToolbox.name(), "videotoolbox");
        assert_eq!(HardwareAccel::Vaapi.name(), "vaapi");
    }

    #[test]
    fn hardware_accel_variants_should_be_accessible() {
        let accels = [
            HardwareAccel::Auto,
            HardwareAccel::None,
            HardwareAccel::Nvdec,
            HardwareAccel::Qsv,
            HardwareAccel::Amf,
            HardwareAccel::VideoToolbox,
            HardwareAccel::Vaapi,
        ];
        for accel in accels {
            let _ = format!("{accel:?}");
        }
    }
}
