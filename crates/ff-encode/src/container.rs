//! Container format definitions.

/// Container format for output file.
///
/// The container format is usually auto-detected from the file extension,
/// but can be explicitly specified if needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Container {
    /// MP4 / `QuickTime`
    Mp4,

    /// `WebM`
    WebM,

    /// Matroska
    Mkv,

    /// AVI
    Avi,

    /// MOV
    Mov,

    /// FLAC (lossless audio container)
    Flac,

    /// OGG (audio container for Vorbis/Opus)
    Ogg,
}

impl Container {
    /// Get `FFmpeg` format name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::WebM => "webm",
            Self::Mkv => "matroska",
            Self::Avi => "avi",
            Self::Mov => "mov",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
        }
    }

    /// Get default file extension.
    #[must_use]
    pub const fn default_extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::WebM => "webm",
            Self::Mkv => "mkv",
            Self::Avi => "avi",
            Self::Mov => "mov",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_as_str() {
        assert_eq!(Container::Mp4.as_str(), "mp4");
        assert_eq!(Container::WebM.as_str(), "webm");
        assert_eq!(Container::Mkv.as_str(), "matroska");
    }

    #[test]
    fn test_container_extension() {
        assert_eq!(Container::Mp4.default_extension(), "mp4");
        assert_eq!(Container::WebM.default_extension(), "webm");
        assert_eq!(Container::Mkv.default_extension(), "mkv");
        assert_eq!(Container::Flac.default_extension(), "flac");
        assert_eq!(Container::Ogg.default_extension(), "ogg");
    }

    #[test]
    fn flac_as_str_should_return_flac() {
        assert_eq!(Container::Flac.as_str(), "flac");
    }

    #[test]
    fn ogg_as_str_should_return_ogg() {
        assert_eq!(Container::Ogg.as_str(), "ogg");
    }
}
