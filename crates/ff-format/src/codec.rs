//! Video and audio codec definitions.
//!
//! This module provides enums for identifying video and audio codecs
//! commonly used in media files.
//!
//! # Examples
//!
//! ```
//! use ff_format::codec::{VideoCodec, AudioCodec};
//!
//! let video = VideoCodec::H264;
//! assert!(video.is_h264_family());
//! assert_eq!(video.name(), "h264");
//!
//! let audio = AudioCodec::Aac;
//! assert!(audio.is_lossy());
//! ```

use std::fmt;

/// Video codec identifier.
///
/// This enum represents common video codecs used in media files.
/// It covers the most widely used codecs while remaining extensible
/// via the `Unknown` variant.
///
/// # Common Usage
///
/// - **H.264/AVC**: Most common codec for HD video, excellent compatibility
/// - **H.265/HEVC**: Better compression than H.264, used for 4K content
/// - **VP9**: Google's open codec for web video streaming
/// - **AV1**: Next-gen open codec, excellent compression
/// - **Apple `ProRes`**: Apple's professional editing codec
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum VideoCodec {
    /// H.264/AVC - most common video codec
    #[default]
    H264,
    /// H.265/HEVC - successor to H.264, better compression
    H265,
    /// VP8 - Google's older open codec
    Vp8,
    /// VP9 - Google's open codec for web video streaming
    Vp9,
    /// AV1 - Alliance for Open Media codec, next-gen compression
    Av1,
    /// AV1 encoded via SVT-AV1 (libsvtav1) — LGPL-licensed, often faster than libaom-av1.
    ///
    /// Requires an `FFmpeg` build with `--enable-libsvtav1`.
    Av1Svt,
    /// Apple's professional editing codec
    ProRes,
    /// Avid DNxHD/DNxHR — professional editing codec for post-production
    DnxHd,
    /// MPEG-4 Part 2 - older codec, legacy support
    Mpeg4,
    /// MPEG-2 Video - DVD and broadcast standard
    Mpeg2,
    /// MJPEG - Motion JPEG, used by some cameras
    Mjpeg,
    /// PNG - lossless image codec, used for image sequence output
    Png,
    /// FFV1 - lossless video codec for archival use
    Ffv1,
    /// Unknown or unsupported codec
    Unknown,
}

impl VideoCodec {
    /// Returns the codec name as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert_eq!(VideoCodec::H264.name(), "h264");
    /// assert_eq!(VideoCodec::H265.name(), "hevc");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "hevc",
            Self::Vp8 => "vp8",
            Self::Vp9 => "vp9",
            Self::Av1 | Self::Av1Svt => "av1",
            Self::ProRes => "prores",
            Self::DnxHd => "dnxhd",
            Self::Mpeg4 => "mpeg4",
            Self::Mpeg2 => "mpeg2video",
            Self::Mjpeg => "mjpeg",
            Self::Png => "png",
            Self::Ffv1 => "ffv1",
            Self::Unknown => "unknown",
        }
    }

    /// Returns the human-readable display name for the codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert_eq!(VideoCodec::H264.display_name(), "H.264/AVC");
    /// assert_eq!(VideoCodec::H265.display_name(), "H.265/HEVC");
    /// ```
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::H264 => "H.264/AVC",
            Self::H265 => "H.265/HEVC",
            Self::Vp8 => "VP8",
            Self::Vp9 => "VP9",
            Self::Av1 => "AV1",
            Self::Av1Svt => "AV1 (SVT)",
            Self::ProRes => "Apple ProRes",
            Self::DnxHd => "Avid DNxHD/DNxHR",
            Self::Mpeg4 => "MPEG-4 Part 2",
            Self::Mpeg2 => "MPEG-2",
            Self::Mjpeg => "Motion JPEG",
            Self::Png => "PNG",
            Self::Ffv1 => "FFV1",
            Self::Unknown => "Unknown",
        }
    }

    /// Returns `true` if this is part of the H.264 family.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert!(VideoCodec::H264.is_h264_family());
    /// assert!(!VideoCodec::H265.is_h264_family());
    /// ```
    #[must_use]
    pub const fn is_h264_family(&self) -> bool {
        matches!(self, Self::H264)
    }

    /// Returns `true` if this is part of the H.265/HEVC family.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert!(VideoCodec::H265.is_h265_family());
    /// assert!(!VideoCodec::H264.is_h265_family());
    /// ```
    #[must_use]
    pub const fn is_h265_family(&self) -> bool {
        matches!(self, Self::H265)
    }

    /// Returns `true` if this is a Google/WebM codec (VP8, VP9).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert!(VideoCodec::Vp8.is_vp_family());
    /// assert!(VideoCodec::Vp9.is_vp_family());
    /// assert!(!VideoCodec::H264.is_vp_family());
    /// ```
    #[must_use]
    pub const fn is_vp_family(&self) -> bool {
        matches!(self, Self::Vp8 | Self::Vp9)
    }

    /// Returns `true` if this is a professional/editing codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert!(VideoCodec::ProRes.is_professional());
    /// assert!(!VideoCodec::H264.is_professional());
    /// ```
    #[must_use]
    pub const fn is_professional(&self) -> bool {
        matches!(self, Self::ProRes | Self::DnxHd)
    }

    /// Returns `true` if this codec supports hardware acceleration on most platforms.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert!(VideoCodec::H264.has_hardware_support());
    /// assert!(VideoCodec::H265.has_hardware_support());
    /// assert!(!VideoCodec::ProRes.has_hardware_support());
    /// ```
    #[must_use]
    pub const fn has_hardware_support(&self) -> bool {
        matches!(self, Self::H264 | Self::H265 | Self::Vp9 | Self::Av1)
    }

    /// Returns `true` if the codec is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::VideoCodec;
    ///
    /// assert!(VideoCodec::Unknown.is_unknown());
    /// assert!(!VideoCodec::H264.is_unknown());
    /// ```
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Audio codec identifier.
///
/// This enum represents common audio codecs used in media files.
/// It covers the most widely used codecs while remaining extensible
/// via the `Unknown` variant.
///
/// # Common Usage
///
/// - **AAC**: Most common for streaming and mobile
/// - **MP3**: Legacy but still widely supported
/// - **Opus**: Excellent quality at low bitrates, used for voice communication
/// - **FLAC**: Lossless compression
/// - **PCM**: Uncompressed audio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum AudioCodec {
    /// AAC (Advanced Audio Coding) - most common lossy codec
    #[default]
    Aac,
    /// MP3 (MPEG-1 Audio Layer 3) - legacy lossy codec
    Mp3,
    /// Opus - modern lossy codec, excellent at low bitrates
    Opus,
    /// FLAC (Free Lossless Audio Codec) - lossless compression
    Flac,
    /// PCM (Pulse Code Modulation) - uncompressed audio
    Pcm,
    /// PCM signed 16-bit little-endian — uncompressed, explicit bit depth
    Pcm16,
    /// PCM signed 24-bit little-endian — uncompressed, professional audio
    Pcm24,
    /// Vorbis - open lossy codec, used in Ogg containers
    Vorbis,
    /// AC3 (Dolby Digital) - surround sound codec
    Ac3,
    /// EAC3 (Dolby Digital Plus) - enhanced AC3
    Eac3,
    /// DTS (Digital Theater Systems) - surround sound codec
    Dts,
    /// ALAC (Apple Lossless Audio Codec)
    Alac,
    /// Unknown or unsupported codec
    Unknown,
}

impl AudioCodec {
    /// Returns the codec name as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::AudioCodec;
    ///
    /// assert_eq!(AudioCodec::Aac.name(), "aac");
    /// assert_eq!(AudioCodec::Flac.name(), "flac");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Mp3 => "mp3",
            Self::Opus => "opus",
            Self::Flac => "flac",
            Self::Pcm => "pcm",
            Self::Pcm16 => "pcm_s16le",
            Self::Pcm24 => "pcm_s24le",
            Self::Vorbis => "vorbis",
            Self::Ac3 => "ac3",
            Self::Eac3 => "eac3",
            Self::Dts => "dts",
            Self::Alac => "alac",
            Self::Unknown => "unknown",
        }
    }

    /// Returns the human-readable display name for the codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::AudioCodec;
    ///
    /// assert_eq!(AudioCodec::Aac.display_name(), "AAC");
    /// assert_eq!(AudioCodec::Flac.display_name(), "FLAC");
    /// ```
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Aac => "AAC",
            Self::Mp3 => "MP3",
            Self::Opus => "Opus",
            Self::Flac => "FLAC",
            Self::Pcm => "PCM",
            Self::Pcm16 => "PCM 16-bit",
            Self::Pcm24 => "PCM 24-bit",
            Self::Vorbis => "Vorbis",
            Self::Ac3 => "Dolby Digital (AC-3)",
            Self::Eac3 => "Dolby Digital Plus (E-AC-3)",
            Self::Dts => "DTS",
            Self::Alac => "Apple Lossless",
            Self::Unknown => "Unknown",
        }
    }

    /// Returns `true` if this is a lossy codec.
    ///
    /// Lossy codecs discard some audio data for smaller file sizes.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::AudioCodec;
    ///
    /// assert!(AudioCodec::Aac.is_lossy());
    /// assert!(AudioCodec::Mp3.is_lossy());
    /// assert!(!AudioCodec::Flac.is_lossy());
    /// ```
    #[must_use]
    pub const fn is_lossy(&self) -> bool {
        matches!(
            self,
            Self::Aac | Self::Mp3 | Self::Opus | Self::Vorbis | Self::Ac3 | Self::Eac3 | Self::Dts
        )
    }

    /// Returns `true` if this is a lossless codec.
    ///
    /// Lossless codecs preserve all audio data.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::AudioCodec;
    ///
    /// assert!(AudioCodec::Flac.is_lossless());
    /// assert!(AudioCodec::Pcm.is_lossless());
    /// assert!(AudioCodec::Alac.is_lossless());
    /// assert!(!AudioCodec::Aac.is_lossless());
    /// ```
    #[must_use]
    pub const fn is_lossless(&self) -> bool {
        matches!(
            self,
            Self::Flac | Self::Pcm | Self::Pcm16 | Self::Pcm24 | Self::Alac
        )
    }

    /// Returns `true` if this is a surround sound codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::AudioCodec;
    ///
    /// assert!(AudioCodec::Ac3.is_surround());
    /// assert!(AudioCodec::Dts.is_surround());
    /// assert!(!AudioCodec::Aac.is_surround());
    /// ```
    #[must_use]
    pub const fn is_surround(&self) -> bool {
        matches!(self, Self::Ac3 | Self::Eac3 | Self::Dts)
    }

    /// Returns `true` if the codec is unknown.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::AudioCodec;
    ///
    /// assert!(AudioCodec::Unknown.is_unknown());
    /// assert!(!AudioCodec::Aac.is_unknown());
    /// ```
    #[must_use]
    pub const fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Subtitle codec identifier.
///
/// This enum represents common subtitle codecs used in media files.
/// It covers text-based and bitmap-based formats while remaining extensible
/// via the `Other` variant.
///
/// Note: No `Copy` or `Default` derive because `Other(String)` is not `Copy`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubtitleCodec {
    /// `SubRip` / SRT — text-based timed subtitles
    Srt,
    /// ASS/SSA (Advanced `SubStation` Alpha) — styled text subtitles
    Ass,
    /// DVB bitmap subtitles (digital broadcast)
    Dvb,
    /// HDMV/PGS — Blu-ray bitmap subtitles
    Hdmv,
    /// `WebVTT` — web-standard text subtitles
    Webvtt,
    /// Unrecognized codec; raw codec name stored for transparency
    Other(String),
}

impl SubtitleCodec {
    /// Returns the codec name as a short string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::SubtitleCodec;
    ///
    /// assert_eq!(SubtitleCodec::Srt.name(), "srt");
    /// assert_eq!(SubtitleCodec::Ass.name(), "ass");
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Srt => "srt",
            Self::Ass => "ass",
            Self::Dvb => "dvb_subtitle",
            Self::Hdmv => "hdmv_pgs_subtitle",
            Self::Webvtt => "webvtt",
            Self::Other(name) => name.as_str(),
        }
    }

    /// Returns the human-readable display name for the codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::SubtitleCodec;
    ///
    /// assert_eq!(SubtitleCodec::Srt.display_name(), "SubRip (SRT)");
    /// assert_eq!(SubtitleCodec::Hdmv.display_name(), "HDMV/PGS");
    /// ```
    #[must_use]
    pub fn display_name(&self) -> &str {
        match self {
            Self::Srt => "SubRip (SRT)",
            Self::Ass => "ASS/SSA",
            Self::Dvb => "DVB Subtitle",
            Self::Hdmv => "HDMV/PGS",
            Self::Webvtt => "WebVTT",
            Self::Other(name) => name.as_str(),
        }
    }

    /// Returns `true` if this is a text-based subtitle codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::SubtitleCodec;
    ///
    /// assert!(SubtitleCodec::Srt.is_text_based());
    /// assert!(SubtitleCodec::Ass.is_text_based());
    /// assert!(!SubtitleCodec::Dvb.is_text_based());
    /// ```
    #[must_use]
    pub fn is_text_based(&self) -> bool {
        matches!(self, Self::Srt | Self::Ass | Self::Webvtt)
    }

    /// Returns `true` if this is a bitmap-based subtitle codec.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::SubtitleCodec;
    ///
    /// assert!(SubtitleCodec::Dvb.is_bitmap_based());
    /// assert!(SubtitleCodec::Hdmv.is_bitmap_based());
    /// assert!(!SubtitleCodec::Srt.is_bitmap_based());
    /// ```
    #[must_use]
    pub fn is_bitmap_based(&self) -> bool {
        matches!(self, Self::Dvb | Self::Hdmv)
    }

    /// Returns `true` if the codec is unrecognized.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::codec::SubtitleCodec;
    ///
    /// assert!(SubtitleCodec::Other("dvd_subtitle".to_string()).is_unknown());
    /// assert!(!SubtitleCodec::Srt.is_unknown());
    /// ```
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Other(_))
    }
}

impl fmt::Display for SubtitleCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod video_codec_tests {
        use super::*;

        #[test]
        fn test_names() {
            assert_eq!(VideoCodec::H264.name(), "h264");
            assert_eq!(VideoCodec::H265.name(), "hevc");
            assert_eq!(VideoCodec::Vp8.name(), "vp8");
            assert_eq!(VideoCodec::Vp9.name(), "vp9");
            assert_eq!(VideoCodec::Av1.name(), "av1");
            assert_eq!(VideoCodec::ProRes.name(), "prores");
            assert_eq!(VideoCodec::DnxHd.name(), "dnxhd");
            assert_eq!(VideoCodec::Mpeg4.name(), "mpeg4");
            assert_eq!(VideoCodec::Mpeg2.name(), "mpeg2video");
            assert_eq!(VideoCodec::Mjpeg.name(), "mjpeg");
            assert_eq!(VideoCodec::Unknown.name(), "unknown");
        }

        #[test]
        fn test_display_names() {
            assert_eq!(VideoCodec::H264.display_name(), "H.264/AVC");
            assert_eq!(VideoCodec::H265.display_name(), "H.265/HEVC");
            assert_eq!(VideoCodec::ProRes.display_name(), "Apple ProRes");
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", VideoCodec::H264), "H.264/AVC");
            assert_eq!(format!("{}", VideoCodec::Av1), "AV1");
        }

        #[test]
        fn test_default() {
            assert_eq!(VideoCodec::default(), VideoCodec::H264);
        }

        #[test]
        fn test_codec_families() {
            assert!(VideoCodec::H264.is_h264_family());
            assert!(!VideoCodec::H265.is_h264_family());

            assert!(VideoCodec::H265.is_h265_family());
            assert!(!VideoCodec::H264.is_h265_family());

            assert!(VideoCodec::Vp8.is_vp_family());
            assert!(VideoCodec::Vp9.is_vp_family());
            assert!(!VideoCodec::H264.is_vp_family());
        }

        #[test]
        fn test_is_professional() {
            assert!(VideoCodec::ProRes.is_professional());
            assert!(VideoCodec::DnxHd.is_professional());
            assert!(!VideoCodec::H264.is_professional());
            assert!(!VideoCodec::Unknown.is_professional());
        }

        #[test]
        fn av1svt_name_should_return_av1() {
            assert_eq!(VideoCodec::Av1Svt.name(), "av1");
        }

        #[test]
        fn av1svt_display_name_should_return_av1_svt() {
            assert_eq!(VideoCodec::Av1Svt.display_name(), "AV1 (SVT)");
        }

        #[test]
        fn dnxhd_should_have_correct_name_and_display_name() {
            assert_eq!(VideoCodec::DnxHd.name(), "dnxhd");
            assert_eq!(VideoCodec::DnxHd.display_name(), "Avid DNxHD/DNxHR");
        }

        #[test]
        fn dnxhd_should_be_professional() {
            assert!(VideoCodec::DnxHd.is_professional());
            assert!(!VideoCodec::DnxHd.is_h264_family());
            assert!(!VideoCodec::DnxHd.is_h265_family());
            assert!(!VideoCodec::DnxHd.is_vp_family());
            assert!(!VideoCodec::DnxHd.is_unknown());
        }

        #[test]
        fn dnxhd_should_not_have_hardware_support() {
            assert!(!VideoCodec::DnxHd.has_hardware_support());
        }

        #[test]
        fn test_hardware_support() {
            assert!(VideoCodec::H264.has_hardware_support());
            assert!(VideoCodec::H265.has_hardware_support());
            assert!(VideoCodec::Vp9.has_hardware_support());
            assert!(VideoCodec::Av1.has_hardware_support());
            assert!(!VideoCodec::ProRes.has_hardware_support());
            assert!(!VideoCodec::Mjpeg.has_hardware_support());
        }

        #[test]
        fn test_is_unknown() {
            assert!(VideoCodec::Unknown.is_unknown());
            assert!(!VideoCodec::H264.is_unknown());
        }

        #[test]
        fn test_debug() {
            assert_eq!(format!("{:?}", VideoCodec::H264), "H264");
            assert_eq!(format!("{:?}", VideoCodec::H265), "H265");
        }

        #[test]
        fn test_equality_and_hash() {
            use std::collections::HashSet;

            assert_eq!(VideoCodec::H264, VideoCodec::H264);
            assert_ne!(VideoCodec::H264, VideoCodec::H265);

            let mut set = HashSet::new();
            set.insert(VideoCodec::H264);
            set.insert(VideoCodec::H265);
            assert!(set.contains(&VideoCodec::H264));
            assert!(!set.contains(&VideoCodec::Vp9));
        }

        #[test]
        fn test_copy() {
            let codec = VideoCodec::H264;
            let copied = codec;
            assert_eq!(codec, copied);
        }
    }

    mod subtitle_codec_tests {
        use super::*;

        #[test]
        fn name_should_return_short_codec_name() {
            assert_eq!(SubtitleCodec::Srt.name(), "srt");
            assert_eq!(SubtitleCodec::Ass.name(), "ass");
            assert_eq!(SubtitleCodec::Dvb.name(), "dvb_subtitle");
            assert_eq!(SubtitleCodec::Hdmv.name(), "hdmv_pgs_subtitle");
            assert_eq!(SubtitleCodec::Webvtt.name(), "webvtt");
            assert_eq!(
                SubtitleCodec::Other("dvd_subtitle".to_string()).name(),
                "dvd_subtitle"
            );
        }

        #[test]
        fn display_name_should_return_human_readable_name() {
            assert_eq!(SubtitleCodec::Srt.display_name(), "SubRip (SRT)");
            assert_eq!(SubtitleCodec::Ass.display_name(), "ASS/SSA");
            assert_eq!(SubtitleCodec::Dvb.display_name(), "DVB Subtitle");
            assert_eq!(SubtitleCodec::Hdmv.display_name(), "HDMV/PGS");
            assert_eq!(SubtitleCodec::Webvtt.display_name(), "WebVTT");
        }

        #[test]
        fn display_should_use_display_name() {
            assert_eq!(format!("{}", SubtitleCodec::Srt), "SubRip (SRT)");
            assert_eq!(format!("{}", SubtitleCodec::Hdmv), "HDMV/PGS");
        }

        #[test]
        fn is_text_based_should_return_true_for_text_codecs() {
            assert!(SubtitleCodec::Srt.is_text_based());
            assert!(SubtitleCodec::Ass.is_text_based());
            assert!(SubtitleCodec::Webvtt.is_text_based());
            assert!(!SubtitleCodec::Dvb.is_text_based());
            assert!(!SubtitleCodec::Hdmv.is_text_based());
        }

        #[test]
        fn is_bitmap_based_should_return_true_for_bitmap_codecs() {
            assert!(SubtitleCodec::Dvb.is_bitmap_based());
            assert!(SubtitleCodec::Hdmv.is_bitmap_based());
            assert!(!SubtitleCodec::Srt.is_bitmap_based());
            assert!(!SubtitleCodec::Ass.is_bitmap_based());
            assert!(!SubtitleCodec::Webvtt.is_bitmap_based());
        }

        #[test]
        fn is_unknown_should_return_true_only_for_other_variant() {
            assert!(SubtitleCodec::Other("dvd_subtitle".to_string()).is_unknown());
            assert!(!SubtitleCodec::Srt.is_unknown());
            assert!(!SubtitleCodec::Dvb.is_unknown());
        }

        #[test]
        fn equality_should_compare_by_value() {
            assert_eq!(SubtitleCodec::Srt, SubtitleCodec::Srt);
            assert_ne!(SubtitleCodec::Srt, SubtitleCodec::Ass);
            assert_eq!(
                SubtitleCodec::Other("foo".to_string()),
                SubtitleCodec::Other("foo".to_string())
            );
            assert_ne!(
                SubtitleCodec::Other("foo".to_string()),
                SubtitleCodec::Other("bar".to_string())
            );
        }

        #[test]
        fn clone_should_produce_equal_value() {
            let codec = SubtitleCodec::Other("test".to_string());
            let cloned = codec.clone();
            assert_eq!(codec, cloned);
        }
    }

    mod audio_codec_tests {
        use super::*;

        #[test]
        fn test_names() {
            assert_eq!(AudioCodec::Aac.name(), "aac");
            assert_eq!(AudioCodec::Mp3.name(), "mp3");
            assert_eq!(AudioCodec::Opus.name(), "opus");
            assert_eq!(AudioCodec::Flac.name(), "flac");
            assert_eq!(AudioCodec::Pcm.name(), "pcm");
            assert_eq!(AudioCodec::Vorbis.name(), "vorbis");
            assert_eq!(AudioCodec::Ac3.name(), "ac3");
            assert_eq!(AudioCodec::Eac3.name(), "eac3");
            assert_eq!(AudioCodec::Dts.name(), "dts");
            assert_eq!(AudioCodec::Alac.name(), "alac");
            assert_eq!(AudioCodec::Unknown.name(), "unknown");
        }

        #[test]
        fn test_display_names() {
            assert_eq!(AudioCodec::Aac.display_name(), "AAC");
            assert_eq!(AudioCodec::Flac.display_name(), "FLAC");
            assert_eq!(AudioCodec::Ac3.display_name(), "Dolby Digital (AC-3)");
        }

        #[test]
        fn test_display() {
            assert_eq!(format!("{}", AudioCodec::Aac), "AAC");
            assert_eq!(format!("{}", AudioCodec::Opus), "Opus");
        }

        #[test]
        fn test_default() {
            assert_eq!(AudioCodec::default(), AudioCodec::Aac);
        }

        #[test]
        fn test_lossy_lossless() {
            // Lossy codecs
            assert!(AudioCodec::Aac.is_lossy());
            assert!(AudioCodec::Mp3.is_lossy());
            assert!(AudioCodec::Opus.is_lossy());
            assert!(AudioCodec::Vorbis.is_lossy());
            assert!(AudioCodec::Ac3.is_lossy());
            assert!(AudioCodec::Eac3.is_lossy());
            assert!(AudioCodec::Dts.is_lossy());

            // Lossless codecs
            assert!(AudioCodec::Flac.is_lossless());
            assert!(AudioCodec::Pcm.is_lossless());
            assert!(AudioCodec::Alac.is_lossless());

            // Mutual exclusion
            assert!(!AudioCodec::Aac.is_lossless());
            assert!(!AudioCodec::Flac.is_lossy());
        }

        #[test]
        fn test_surround() {
            assert!(AudioCodec::Ac3.is_surround());
            assert!(AudioCodec::Eac3.is_surround());
            assert!(AudioCodec::Dts.is_surround());
            assert!(!AudioCodec::Aac.is_surround());
            assert!(!AudioCodec::Flac.is_surround());
        }

        #[test]
        fn pcm16_name_should_return_pcm_s16le() {
            assert_eq!(AudioCodec::Pcm16.name(), "pcm_s16le");
        }

        #[test]
        fn pcm24_name_should_return_pcm_s24le() {
            assert_eq!(AudioCodec::Pcm24.name(), "pcm_s24le");
        }

        #[test]
        fn pcm16_should_be_lossless() {
            assert!(AudioCodec::Pcm16.is_lossless());
            assert!(!AudioCodec::Pcm16.is_lossy());
        }

        #[test]
        fn pcm24_should_be_lossless() {
            assert!(AudioCodec::Pcm24.is_lossless());
            assert!(!AudioCodec::Pcm24.is_lossy());
        }

        #[test]
        fn test_is_unknown() {
            assert!(AudioCodec::Unknown.is_unknown());
            assert!(!AudioCodec::Aac.is_unknown());
        }

        #[test]
        fn test_debug() {
            assert_eq!(format!("{:?}", AudioCodec::Aac), "Aac");
            assert_eq!(format!("{:?}", AudioCodec::Flac), "Flac");
        }

        #[test]
        fn test_equality_and_hash() {
            use std::collections::HashSet;

            assert_eq!(AudioCodec::Aac, AudioCodec::Aac);
            assert_ne!(AudioCodec::Aac, AudioCodec::Mp3);

            let mut set = HashSet::new();
            set.insert(AudioCodec::Aac);
            set.insert(AudioCodec::Flac);
            assert!(set.contains(&AudioCodec::Aac));
            assert!(!set.contains(&AudioCodec::Opus));
        }

        #[test]
        fn test_copy() {
            let codec = AudioCodec::Aac;
            let copied = codec;
            assert_eq!(codec, copied);
        }
    }
}
