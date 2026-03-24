//! Subtitle stream info and builder.

use std::time::Duration;

use crate::codec::SubtitleCodec;

/// Information about a subtitle stream within a media file.
///
/// This struct contains all metadata needed to identify and categorize
/// a subtitle stream, including codec, language, and forced flag.
///
/// # Construction
///
/// Use [`SubtitleStreamInfo::builder()`] for fluent construction:
///
/// ```
/// use ff_format::stream::SubtitleStreamInfo;
/// use ff_format::codec::SubtitleCodec;
///
/// let info = SubtitleStreamInfo::builder()
///     .index(2)
///     .codec(SubtitleCodec::Srt)
///     .codec_name("srt")
///     .language("eng")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct SubtitleStreamInfo {
    /// Stream index within the container
    index: u32,
    /// Subtitle codec
    codec: SubtitleCodec,
    /// Codec name as reported by the demuxer
    codec_name: String,
    /// Language code (e.g., "eng", "jpn")
    language: Option<String>,
    /// Stream title (e.g., "English (Forced)")
    title: Option<String>,
    /// Stream duration (if known)
    duration: Option<Duration>,
    /// Whether this is a forced subtitle track
    forced: bool,
}

impl SubtitleStreamInfo {
    /// Creates a new builder for constructing `SubtitleStreamInfo`.
    #[must_use]
    pub fn builder() -> SubtitleStreamInfoBuilder {
        SubtitleStreamInfoBuilder::default()
    }

    /// Returns the stream index within the container.
    #[must_use]
    #[inline]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Returns the subtitle codec.
    #[must_use]
    #[inline]
    pub fn codec(&self) -> &SubtitleCodec {
        &self.codec
    }

    /// Returns the codec name as reported by the demuxer.
    #[must_use]
    #[inline]
    pub fn codec_name(&self) -> &str {
        &self.codec_name
    }

    /// Returns the language code, if specified.
    #[must_use]
    #[inline]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Returns the stream title, if specified.
    #[must_use]
    #[inline]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the stream duration, if known.
    #[must_use]
    #[inline]
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Returns `true` if this is a forced subtitle track.
    #[must_use]
    #[inline]
    pub const fn is_forced(&self) -> bool {
        self.forced
    }

    /// Returns `true` if the codec is text-based.
    #[must_use]
    #[inline]
    pub fn is_text_based(&self) -> bool {
        self.codec.is_text_based()
    }
}

/// Builder for constructing `SubtitleStreamInfo`.
#[derive(Debug, Clone)]
pub struct SubtitleStreamInfoBuilder {
    index: u32,
    codec: SubtitleCodec,
    codec_name: String,
    language: Option<String>,
    title: Option<String>,
    duration: Option<Duration>,
    forced: bool,
}

impl Default for SubtitleStreamInfoBuilder {
    fn default() -> Self {
        Self {
            index: 0,
            codec: SubtitleCodec::Other(String::new()),
            codec_name: String::new(),
            language: None,
            title: None,
            duration: None,
            forced: false,
        }
    }
}

impl SubtitleStreamInfoBuilder {
    /// Sets the stream index.
    #[must_use]
    pub fn index(mut self, index: u32) -> Self {
        self.index = index;
        self
    }

    /// Sets the subtitle codec.
    #[must_use]
    pub fn codec(mut self, codec: SubtitleCodec) -> Self {
        self.codec = codec;
        self
    }

    /// Sets the codec name string.
    #[must_use]
    pub fn codec_name(mut self, name: impl Into<String>) -> Self {
        self.codec_name = name.into();
        self
    }

    /// Sets the language code.
    #[must_use]
    pub fn language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Sets the stream title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the stream duration.
    #[must_use]
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Sets the forced flag.
    #[must_use]
    pub fn forced(mut self, forced: bool) -> Self {
        self.forced = forced;
        self
    }

    /// Builds the `SubtitleStreamInfo`.
    #[must_use]
    pub fn build(self) -> SubtitleStreamInfo {
        SubtitleStreamInfo {
            index: self.index,
            codec: self.codec,
            codec_name: self.codec_name,
            language: self.language,
            title: self.title,
            duration: self.duration,
            forced: self.forced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_should_store_all_fields() {
        let info = SubtitleStreamInfo::builder()
            .index(2)
            .codec(SubtitleCodec::Srt)
            .codec_name("srt")
            .language("eng")
            .title("English")
            .duration(Duration::from_secs(120))
            .forced(true)
            .build();

        assert_eq!(info.index(), 2);
        assert_eq!(info.codec(), &SubtitleCodec::Srt);
        assert_eq!(info.codec_name(), "srt");
        assert_eq!(info.language(), Some("eng"));
        assert_eq!(info.title(), Some("English"));
        assert_eq!(info.duration(), Some(Duration::from_secs(120)));
        assert!(info.is_forced());
    }

    #[test]
    fn is_forced_should_default_to_false() {
        let info = SubtitleStreamInfo::builder()
            .codec(SubtitleCodec::Ass)
            .build();
        assert!(!info.is_forced());
    }

    #[test]
    fn is_text_based_should_delegate_to_codec() {
        let text = SubtitleStreamInfo::builder()
            .codec(SubtitleCodec::Srt)
            .build();
        assert!(text.is_text_based());

        let bitmap = SubtitleStreamInfo::builder()
            .codec(SubtitleCodec::Hdmv)
            .build();
        assert!(!bitmap.is_text_based());
    }

    #[test]
    fn optional_fields_should_default_to_none() {
        let info = SubtitleStreamInfo::builder().build();
        assert!(info.language().is_none());
        assert!(info.title().is_none());
        assert!(info.duration().is_none());
    }
}
