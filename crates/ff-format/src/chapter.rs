//! Chapter information.
//!
//! This module provides the [`ChapterInfo`] struct for representing chapter
//! markers within a media container (e.g., MKV, MP4, M4A).
//!
//! # Examples
//!
//! ```
//! use ff_format::chapter::ChapterInfo;
//! use ff_format::Rational;
//! use std::time::Duration;
//!
//! let chapter = ChapterInfo::builder()
//!     .id(1)
//!     .title("Opening")
//!     .start(Duration::from_secs(0))
//!     .end(Duration::from_secs(120))
//!     .time_base(Rational::new(1, 1000))
//!     .build();
//!
//! assert_eq!(chapter.id(), 1);
//! assert_eq!(chapter.title(), Some("Opening"));
//! assert_eq!(chapter.duration(), Duration::from_secs(120));
//! ```

use std::collections::HashMap;
use std::time::Duration;

use crate::time::Rational;

/// Information about a chapter within a media file.
///
/// Chapters are discrete, named segments within a container (e.g., a chapter in
/// an audiobook or a scene in a movie). Each chapter has a start and end time,
/// and optionally a title and other metadata tags.
///
/// # Construction
///
/// Use [`ChapterInfo::builder()`] for fluent construction:
///
/// ```
/// use ff_format::chapter::ChapterInfo;
/// use std::time::Duration;
///
/// let chapter = ChapterInfo::builder()
///     .id(0)
///     .title("Intro")
///     .start(Duration::ZERO)
///     .end(Duration::from_secs(30))
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ChapterInfo {
    /// Chapter ID as reported by the container (`AVChapter.id`).
    id: i64,
    /// Chapter title from the "title" metadata tag, if present.
    title: Option<String>,
    /// Chapter start time.
    start: Duration,
    /// Chapter end time.
    end: Duration,
    /// Container time base for this chapter.
    ///
    /// Useful when sub-`Duration` precision is required. `None` if the
    /// time base had a zero denominator (invalid/unset).
    time_base: Option<Rational>,
    /// All metadata tags except "title" (which is surfaced via [`ChapterInfo::title`]).
    ///
    /// `None` if the chapter had no metadata dictionary or all tags were filtered out.
    metadata: Option<HashMap<String, String>>,
}

impl ChapterInfo {
    /// Creates a new builder for constructing `ChapterInfo`.
    #[must_use]
    pub fn builder() -> ChapterInfoBuilder {
        ChapterInfoBuilder::default()
    }

    /// Returns the chapter ID.
    #[must_use]
    #[inline]
    pub fn id(&self) -> i64 {
        self.id
    }

    /// Returns the chapter title, if available.
    #[must_use]
    #[inline]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Returns the chapter start time.
    #[must_use]
    #[inline]
    pub fn start(&self) -> Duration {
        self.start
    }

    /// Returns the chapter end time.
    #[must_use]
    #[inline]
    pub fn end(&self) -> Duration {
        self.end
    }

    /// Returns the chapter time base, if available.
    #[must_use]
    #[inline]
    pub fn time_base(&self) -> Option<Rational> {
        self.time_base
    }

    /// Returns the chapter metadata tags (excluding "title"), if any.
    #[must_use]
    #[inline]
    pub fn metadata(&self) -> Option<&HashMap<String, String>> {
        self.metadata.as_ref()
    }

    /// Returns `true` if the chapter has a title.
    #[must_use]
    #[inline]
    pub fn has_title(&self) -> bool {
        self.title.is_some()
    }

    /// Returns the chapter duration (`end − start`).
    ///
    /// Uses saturating subtraction so that malformed chapters where `end < start`
    /// return [`Duration::ZERO`] instead of panicking.
    #[must_use]
    #[inline]
    pub fn duration(&self) -> Duration {
        self.end.saturating_sub(self.start)
    }
}

impl Default for ChapterInfo {
    fn default() -> Self {
        Self {
            id: 0,
            title: None,
            start: Duration::ZERO,
            end: Duration::ZERO,
            time_base: None,
            metadata: None,
        }
    }
}

/// Builder for constructing [`ChapterInfo`].
///
/// # Examples
///
/// ```
/// use ff_format::chapter::ChapterInfo;
/// use ff_format::Rational;
/// use std::time::Duration;
///
/// let chapter = ChapterInfo::builder()
///     .id(2)
///     .title("Act I")
///     .start(Duration::from_secs(120))
///     .end(Duration::from_secs(480))
///     .time_base(Rational::new(1, 1000))
///     .build();
///
/// assert_eq!(chapter.title(), Some("Act I"));
/// assert_eq!(chapter.duration(), Duration::from_secs(360));
/// ```
#[derive(Debug, Clone, Default)]
pub struct ChapterInfoBuilder {
    id: i64,
    title: Option<String>,
    start: Duration,
    end: Duration,
    time_base: Option<Rational>,
    metadata: Option<HashMap<String, String>>,
}

impl ChapterInfoBuilder {
    /// Sets the chapter ID.
    #[must_use]
    pub fn id(mut self, id: i64) -> Self {
        self.id = id;
        self
    }

    /// Sets the chapter title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the chapter start time.
    #[must_use]
    pub fn start(mut self, start: Duration) -> Self {
        self.start = start;
        self
    }

    /// Sets the chapter end time.
    #[must_use]
    pub fn end(mut self, end: Duration) -> Self {
        self.end = end;
        self
    }

    /// Sets the chapter time base.
    #[must_use]
    pub fn time_base(mut self, time_base: Rational) -> Self {
        self.time_base = Some(time_base);
        self
    }

    /// Sets the chapter metadata (tags other than "title").
    #[must_use]
    pub fn metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Builds the [`ChapterInfo`].
    #[must_use]
    pub fn build(self) -> ChapterInfo {
        ChapterInfo {
            id: self.id,
            title: self.title,
            start: self.start,
            end: self.end,
            time_base: self.time_base,
            metadata: self.metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_info_builder_should_set_all_fields() {
        let mut meta = HashMap::new();
        meta.insert("language".to_string(), "eng".to_string());

        let info = ChapterInfo::builder()
            .id(3)
            .title("Intro")
            .start(Duration::from_secs(0))
            .end(Duration::from_secs(60))
            .time_base(Rational::new(1, 1000))
            .metadata(meta)
            .build();

        assert_eq!(info.id(), 3);
        assert_eq!(info.title(), Some("Intro"));
        assert_eq!(info.start(), Duration::ZERO);
        assert_eq!(info.end(), Duration::from_secs(60));
        assert_eq!(info.time_base(), Some(Rational::new(1, 1000)));
        assert_eq!(info.metadata().unwrap()["language"], "eng");
    }

    #[test]
    fn chapter_info_duration_should_return_end_minus_start() {
        let info = ChapterInfo::builder()
            .start(Duration::from_secs(10))
            .end(Duration::from_secs(70))
            .build();

        assert_eq!(info.duration(), Duration::from_secs(60));
    }

    #[test]
    fn chapter_info_duration_should_return_zero_when_end_before_start() {
        let info = ChapterInfo::builder()
            .start(Duration::from_secs(70))
            .end(Duration::from_secs(10))
            .build();

        assert_eq!(info.duration(), Duration::ZERO);
    }

    #[test]
    fn chapter_info_with_no_title_should_return_none() {
        let info = ChapterInfo::builder().id(1).build();

        assert_eq!(info.title(), None);
        assert!(!info.has_title());
    }

    #[test]
    fn chapter_info_with_title_should_have_title() {
        let info = ChapterInfo::builder().title("Chapter One").build();

        assert_eq!(info.title(), Some("Chapter One"));
        assert!(info.has_title());
    }

    #[test]
    fn chapter_info_default_should_have_zero_times() {
        let info = ChapterInfo::default();

        assert_eq!(info.id(), 0);
        assert_eq!(info.start(), Duration::ZERO);
        assert_eq!(info.end(), Duration::ZERO);
        assert!(info.title().is_none());
        assert!(info.time_base().is_none());
        assert!(info.metadata().is_none());
    }

    #[test]
    fn chapter_info_builder_without_metadata_should_return_none() {
        let info = ChapterInfo::builder().id(1).title("Test").build();

        assert!(info.metadata().is_none());
    }

    #[test]
    fn chapter_info_builder_clone_should_produce_equal_instance() {
        let builder = ChapterInfo::builder()
            .id(5)
            .title("Cloned")
            .start(Duration::from_secs(100))
            .end(Duration::from_secs(200));

        let info1 = builder.clone().build();
        let info2 = builder.build();

        assert_eq!(info1.id(), info2.id());
        assert_eq!(info1.title(), info2.title());
        assert_eq!(info1.start(), info2.start());
        assert_eq!(info1.end(), info2.end());
    }
}
