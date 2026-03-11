//! Codec definitions for encoding.

/// Video codec for encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum VideoCodec {
    /// H.264 / AVC (most compatible)
    #[default]
    H264,

    /// H.265 / HEVC (high compression)
    H265,

    /// VP9 (`WebM`, royalty-free)
    Vp9,

    /// AV1 (latest, high compression, LGPL compatible)
    Av1,

    /// `ProRes` (Apple, editing)
    ProRes,

    /// `DNxHD`/`DNxHR` (Avid, editing)
    DnxHd,

    /// MPEG-4
    Mpeg4,
}

impl VideoCodec {
    /// Check if this codec specification is LGPL compatible.
    ///
    /// Returns `true` for codecs that can be used without GPL licensing.
    ///
    /// **Important**: This indicates the codec family's licensing, not the actual encoder used.
    /// H.264 and H.265 return `false` because their software encoders (libx264/libx265) are GPL,
    /// but hardware encoders (NVENC, QSV, etc.) are LGPL-compatible.
    ///
    /// Use [`VideoEncoder::is_lgpl_compliant()`](crate::VideoEncoder::is_lgpl_compliant) to check
    /// the actual encoder selected at runtime.
    ///
    /// # LGPL-Compatible Codecs
    ///
    /// - `VP9` - Google's royalty-free codec (libvpx-vp9)
    /// - `AV1` - Next-gen royalty-free codec (libaom-av1)
    /// - `ProRes` - Apple's professional codec
    /// - `DNxHD` - Avid's professional codec
    /// - `MPEG4` - ISO MPEG-4 Part 2
    ///
    /// # GPL Codecs (require licensing for commercial use)
    ///
    /// - `H264` - Requires MPEG LA license (when using libx264)
    /// - `H265` - Requires MPEG LA license (when using libx265)
    ///
    /// Note: Hardware H.264/H.265 encoders are LGPL-compatible and don't require licensing fees.
    #[must_use]
    pub const fn is_lgpl_compatible(self) -> bool {
        match self {
            Self::Vp9 | Self::Av1 | Self::Mpeg4 | Self::ProRes | Self::DnxHd => true,
            Self::H264 | Self::H265 => false, // libx264/libx265 are GPL
        }
    }

    /// Get default file extension for this codec.
    #[must_use]
    pub const fn default_extension(self) -> &'static str {
        match self {
            Self::Vp9 | Self::Av1 => "webm",
            _ => "mp4",
        }
    }
}

/// Audio codec for encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum AudioCodec {
    /// AAC (most compatible)
    #[default]
    Aac,

    /// Opus (high quality, low latency)
    Opus,

    /// MP3
    Mp3,

    /// FLAC (lossless)
    Flac,

    /// PCM (uncompressed)
    Pcm,

    /// Vorbis (OGG)
    Vorbis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_codec_lgpl() {
        assert!(VideoCodec::Vp9.is_lgpl_compatible());
        assert!(VideoCodec::Av1.is_lgpl_compatible());
        assert!(VideoCodec::Mpeg4.is_lgpl_compatible());
        assert!(!VideoCodec::H264.is_lgpl_compatible());
        assert!(!VideoCodec::H265.is_lgpl_compatible());
    }

    #[test]
    fn test_video_codec_extension() {
        assert_eq!(VideoCodec::H264.default_extension(), "mp4");
        assert_eq!(VideoCodec::Vp9.default_extension(), "webm");
        assert_eq!(VideoCodec::Av1.default_extension(), "webm");
    }

    #[test]
    fn test_default_codecs() {
        assert_eq!(VideoCodec::default(), VideoCodec::H264);
        assert_eq!(AudioCodec::default(), AudioCodec::Aac);
    }
}
