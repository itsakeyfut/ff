//! Codec type re-exports and encode-specific extensions.
//!
//! `VideoCodec` and `AudioCodec` are the canonical types defined in
//! `ff-format` and re-exported here so callers can import them from a
//! single crate.  Encode-specific behaviour (LGPL licensing, default
//! file extension) is provided via the [`VideoCodecEncodeExt`] trait.

pub use ff_format::{AudioCodec, VideoCodec};

/// Encode-specific methods for [`VideoCodec`].
///
/// This trait adds encoding-oriented helpers to the shared `VideoCodec` type.
/// Import it to call [`is_lgpl_compatible`](Self::is_lgpl_compatible) or
/// [`default_extension`](Self::default_extension) on a codec value.
///
/// # Examples
///
/// ```
/// use ff_encode::{VideoCodec, VideoCodecEncodeExt};
///
/// assert!(VideoCodec::Vp9.is_lgpl_compatible());
/// assert_eq!(VideoCodec::H264.default_extension(), "mp4");
/// ```
pub trait VideoCodecEncodeExt {
    /// Returns `true` if the *software* encoder for this codec is
    /// LGPL-compatible (i.e. does not require a GPL or proprietary licence).
    ///
    /// **Important**: This reflects the codec family's typical software
    /// encoder licensing, not the actual encoder chosen at runtime.
    /// H.264 and H.265 return `false` because their software encoders
    /// (libx264/libx265) are GPL; hardware encoders (NVENC, QSV, etc.)
    /// are LGPL-compatible regardless.
    ///
    /// Use [`VideoEncoder::is_lgpl_compliant`](crate::VideoEncoder) to
    /// query the actual encoder selected at runtime.
    fn is_lgpl_compatible(&self) -> bool;

    /// Returns the default output file extension for this codec.
    fn default_extension(&self) -> &'static str;
}

impl VideoCodecEncodeExt for VideoCodec {
    fn is_lgpl_compatible(&self) -> bool {
        matches!(
            self,
            VideoCodec::Vp9
                | VideoCodec::Av1
                | VideoCodec::Mpeg4
                | VideoCodec::ProRes
                | VideoCodec::DnxHd
        )
    }

    fn default_extension(&self) -> &'static str {
        match self {
            VideoCodec::Vp9 | VideoCodec::Av1 => "webm",
            _ => "mp4",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_codec_is_lgpl_compatible_should_return_true_for_open_codecs() {
        assert!(VideoCodec::Vp9.is_lgpl_compatible());
        assert!(VideoCodec::Av1.is_lgpl_compatible());
        assert!(VideoCodec::Mpeg4.is_lgpl_compatible());
        assert!(VideoCodec::ProRes.is_lgpl_compatible());
        assert!(VideoCodec::DnxHd.is_lgpl_compatible());
    }

    #[test]
    fn video_codec_is_lgpl_compatible_should_return_false_for_gpl_codecs() {
        assert!(!VideoCodec::H264.is_lgpl_compatible());
        assert!(!VideoCodec::H265.is_lgpl_compatible());
    }

    #[test]
    fn video_codec_default_extension_should_return_webm_for_web_codecs() {
        assert_eq!(VideoCodec::H264.default_extension(), "mp4");
        assert_eq!(VideoCodec::Vp9.default_extension(), "webm");
        assert_eq!(VideoCodec::Av1.default_extension(), "webm");
    }

    #[test]
    fn video_and_audio_codec_default_should_be_accessible() {
        assert_eq!(VideoCodec::default(), VideoCodec::H264);
        assert_eq!(AudioCodec::default(), AudioCodec::Aac);
    }
}
