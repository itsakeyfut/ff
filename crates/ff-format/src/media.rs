//! Media container information.
//!
//! This module provides the [`MediaInfo`] struct for representing metadata about
//! a media file as a whole, including all its video and audio streams.
//!
//! # Examples
//!
//! ```
//! use ff_format::media::{MediaInfo, MediaInfoBuilder};
//! use ff_format::stream::{VideoStreamInfo, AudioStreamInfo};
//! use ff_format::{PixelFormat, SampleFormat, Rational};
//! use ff_format::codec::{VideoCodec, AudioCodec};
//! use std::time::Duration;
//! use std::path::PathBuf;
//!
//! // Create video stream info
//! let video = VideoStreamInfo::builder()
//!     .index(0)
//!     .codec(VideoCodec::H264)
//!     .width(1920)
//!     .height(1080)
//!     .frame_rate(Rational::new(30, 1))
//!     .pixel_format(PixelFormat::Yuv420p)
//!     .build();
//!
//! // Create audio stream info
//! let audio = AudioStreamInfo::builder()
//!     .index(1)
//!     .codec(AudioCodec::Aac)
//!     .sample_rate(48000)
//!     .channels(2)
//!     .sample_format(SampleFormat::F32)
//!     .build();
//!
//! // Create media info
//! let media = MediaInfo::builder()
//!     .path("/path/to/video.mp4")
//!     .format("mp4")
//!     .format_long_name("QuickTime / MOV")
//!     .duration(Duration::from_secs(120))
//!     .file_size(1_000_000)
//!     .bitrate(8_000_000)
//!     .video_stream(video)
//!     .audio_stream(audio)
//!     .metadata("title", "Sample Video")
//!     .metadata("artist", "Test Artist")
//!     .build();
//!
//! assert!(media.has_video());
//! assert!(media.has_audio());
//! assert_eq!(media.resolution(), Some((1920, 1080)));
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::chapter::ChapterInfo;
use crate::stream::{AudioStreamInfo, SubtitleStreamInfo, VideoStreamInfo};

/// Information about a media file.
///
/// This struct contains all metadata about a media container, including
/// format information, duration, file size, and all contained streams.
///
/// # Construction
///
/// Use [`MediaInfo::builder()`] for fluent construction:
///
/// ```
/// use ff_format::media::MediaInfo;
/// use std::time::Duration;
///
/// let info = MediaInfo::builder()
///     .path("/path/to/video.mp4")
///     .format("mp4")
///     .duration(Duration::from_secs(120))
///     .file_size(1_000_000)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct MediaInfo {
    /// File path
    path: PathBuf,
    /// Container format name (e.g., "mp4", "mkv", "avi")
    format: String,
    /// Long format name from the container format description
    format_long_name: Option<String>,
    /// Total duration
    duration: Duration,
    /// File size in bytes
    file_size: u64,
    /// Overall bitrate in bits per second
    bitrate: Option<u64>,
    /// Video streams in the file
    video_streams: Vec<VideoStreamInfo>,
    /// Audio streams in the file
    audio_streams: Vec<AudioStreamInfo>,
    /// Subtitle streams in the file
    subtitle_streams: Vec<SubtitleStreamInfo>,
    /// Chapter markers in the file
    chapters: Vec<ChapterInfo>,
    /// File metadata (title, artist, etc.)
    metadata: HashMap<String, String>,
}

impl MediaInfo {
    /// Creates a new builder for constructing `MediaInfo`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::media::MediaInfo;
    /// use std::time::Duration;
    ///
    /// let info = MediaInfo::builder()
    ///     .path("/path/to/video.mp4")
    ///     .format("mp4")
    ///     .duration(Duration::from_secs(120))
    ///     .file_size(1_000_000)
    ///     .build();
    /// ```
    #[must_use]
    pub fn builder() -> MediaInfoBuilder {
        MediaInfoBuilder::default()
    }

    /// Returns the file path.
    #[must_use]
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the container format name.
    #[must_use]
    #[inline]
    pub fn format(&self) -> &str {
        &self.format
    }

    /// Returns the long format name, if available.
    #[must_use]
    #[inline]
    pub fn format_long_name(&self) -> Option<&str> {
        self.format_long_name.as_deref()
    }

    /// Returns the total duration.
    #[must_use]
    #[inline]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the file size in bytes.
    #[must_use]
    #[inline]
    pub const fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Returns the overall bitrate in bits per second, if known.
    #[must_use]
    #[inline]
    pub const fn bitrate(&self) -> Option<u64> {
        self.bitrate
    }

    /// Returns all video streams in the file.
    #[must_use]
    #[inline]
    pub fn video_streams(&self) -> &[VideoStreamInfo] {
        &self.video_streams
    }

    /// Returns all audio streams in the file.
    #[must_use]
    #[inline]
    pub fn audio_streams(&self) -> &[AudioStreamInfo] {
        &self.audio_streams
    }

    /// Returns all subtitle streams in the file.
    #[must_use]
    #[inline]
    pub fn subtitle_streams(&self) -> &[SubtitleStreamInfo] {
        &self.subtitle_streams
    }

    /// Returns all chapters in the file.
    #[must_use]
    #[inline]
    pub fn chapters(&self) -> &[ChapterInfo] {
        &self.chapters
    }

    /// Returns `true` if the file contains at least one chapter marker.
    #[must_use]
    #[inline]
    pub fn has_chapters(&self) -> bool {
        !self.chapters.is_empty()
    }

    /// Returns the number of chapters.
    #[must_use]
    #[inline]
    pub fn chapter_count(&self) -> usize {
        self.chapters.len()
    }

    /// Returns the file metadata.
    #[must_use]
    #[inline]
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Returns a specific metadata value by key.
    #[must_use]
    #[inline]
    pub fn metadata_value(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    // === Stream Query Methods ===

    /// Returns `true` if the file contains at least one video stream.
    #[must_use]
    #[inline]
    pub fn has_video(&self) -> bool {
        !self.video_streams.is_empty()
    }

    /// Returns `true` if the file contains at least one audio stream.
    #[must_use]
    #[inline]
    pub fn has_audio(&self) -> bool {
        !self.audio_streams.is_empty()
    }

    /// Returns `true` if the file contains at least one subtitle stream.
    #[must_use]
    #[inline]
    pub fn has_subtitles(&self) -> bool {
        !self.subtitle_streams.is_empty()
    }

    /// Returns the number of video streams.
    #[must_use]
    #[inline]
    pub fn video_stream_count(&self) -> usize {
        self.video_streams.len()
    }

    /// Returns the number of audio streams.
    #[must_use]
    #[inline]
    pub fn audio_stream_count(&self) -> usize {
        self.audio_streams.len()
    }

    /// Returns the number of subtitle streams.
    #[must_use]
    #[inline]
    pub fn subtitle_stream_count(&self) -> usize {
        self.subtitle_streams.len()
    }

    /// Returns the total number of streams (video + audio + subtitle).
    #[must_use]
    #[inline]
    pub fn stream_count(&self) -> usize {
        self.video_streams.len() + self.audio_streams.len() + self.subtitle_streams.len()
    }

    // === Primary Stream Selection ===

    /// Returns the primary video stream.
    ///
    /// The primary video stream is the first video stream in the file.
    /// Returns `None` if there are no video streams.
    #[must_use]
    #[inline]
    pub fn primary_video(&self) -> Option<&VideoStreamInfo> {
        self.video_streams.first()
    }

    /// Returns the primary audio stream.
    ///
    /// The primary audio stream is the first audio stream in the file.
    /// Returns `None` if there are no audio streams.
    #[must_use]
    #[inline]
    pub fn primary_audio(&self) -> Option<&AudioStreamInfo> {
        self.audio_streams.first()
    }

    /// Returns a video stream by index within the video streams list.
    #[must_use]
    #[inline]
    pub fn video_stream(&self, index: usize) -> Option<&VideoStreamInfo> {
        self.video_streams.get(index)
    }

    /// Returns an audio stream by index within the audio streams list.
    #[must_use]
    #[inline]
    pub fn audio_stream(&self, index: usize) -> Option<&AudioStreamInfo> {
        self.audio_streams.get(index)
    }

    /// Returns a subtitle stream by index within the subtitle streams list.
    #[must_use]
    #[inline]
    pub fn subtitle_stream(&self, index: usize) -> Option<&SubtitleStreamInfo> {
        self.subtitle_streams.get(index)
    }

    // === Convenience Methods ===

    /// Returns the resolution of the primary video stream.
    ///
    /// Returns `None` if there are no video streams.
    #[must_use]
    #[inline]
    pub fn resolution(&self) -> Option<(u32, u32)> {
        self.primary_video().map(|v| (v.width(), v.height()))
    }

    /// Returns the frame rate of the primary video stream.
    ///
    /// Returns `None` if there are no video streams.
    #[must_use]
    #[inline]
    pub fn frame_rate(&self) -> Option<f64> {
        self.primary_video().map(VideoStreamInfo::fps)
    }

    /// Returns the sample rate of the primary audio stream.
    ///
    /// Returns `None` if there are no audio streams.
    #[must_use]
    #[inline]
    pub fn sample_rate(&self) -> Option<u32> {
        self.primary_audio().map(AudioStreamInfo::sample_rate)
    }

    /// Returns the channel count of the primary audio stream.
    ///
    /// Returns `None` if there are no audio streams.
    ///
    /// The type is `u32` to match `FFmpeg` and professional audio APIs. For `rodio`
    /// or `cpal` (which require `u16`), cast with `.map(|c| c as u16)` — channel
    /// counts never exceed `u16::MAX` in practice.
    #[must_use]
    #[inline]
    pub fn channels(&self) -> Option<u32> {
        self.primary_audio().map(AudioStreamInfo::channels)
    }

    /// Returns `true` if this is a video-only file (no audio streams).
    #[must_use]
    #[inline]
    pub fn is_video_only(&self) -> bool {
        self.has_video() && !self.has_audio()
    }

    /// Returns `true` if this is an audio-only file (no video streams).
    #[must_use]
    #[inline]
    pub fn is_audio_only(&self) -> bool {
        self.has_audio() && !self.has_video()
    }

    /// Returns the file name (without directory path).
    #[must_use]
    #[inline]
    pub fn file_name(&self) -> Option<&str> {
        self.path.file_name().and_then(|n| n.to_str())
    }

    /// Returns the file extension.
    #[must_use]
    #[inline]
    pub fn extension(&self) -> Option<&str> {
        self.path.extension().and_then(|e| e.to_str())
    }

    // === Common Metadata Accessors ===

    /// Returns the title from metadata.
    ///
    /// This is a convenience method for accessing the "title" metadata key,
    /// which is commonly used by media containers to store the title of the content.
    #[must_use]
    #[inline]
    pub fn title(&self) -> Option<&str> {
        self.metadata_value("title")
    }

    /// Returns the artist from metadata.
    ///
    /// This is a convenience method for accessing the "artist" metadata key.
    #[must_use]
    #[inline]
    pub fn artist(&self) -> Option<&str> {
        self.metadata_value("artist")
    }

    /// Returns the album from metadata.
    ///
    /// This is a convenience method for accessing the "album" metadata key.
    #[must_use]
    #[inline]
    pub fn album(&self) -> Option<&str> {
        self.metadata_value("album")
    }

    /// Returns the creation time from metadata.
    ///
    /// This is a convenience method for accessing the `creation_time` metadata key,
    /// which is commonly used by media containers to store when the file was created.
    /// The format is typically ISO 8601 (e.g., `2024-01-15T10:30:00.000000Z`).
    #[must_use]
    #[inline]
    pub fn creation_time(&self) -> Option<&str> {
        self.metadata_value("creation_time")
    }

    /// Returns the date from metadata.
    ///
    /// This is a convenience method for accessing the "date" metadata key.
    #[must_use]
    #[inline]
    pub fn date(&self) -> Option<&str> {
        self.metadata_value("date")
    }

    /// Returns the comment from metadata.
    ///
    /// This is a convenience method for accessing the "comment" metadata key.
    #[must_use]
    #[inline]
    pub fn comment(&self) -> Option<&str> {
        self.metadata_value("comment")
    }

    /// Returns the encoder from metadata.
    ///
    /// This is a convenience method for accessing the "encoder" metadata key,
    /// which stores information about the software used to create the file.
    #[must_use]
    #[inline]
    pub fn encoder(&self) -> Option<&str> {
        self.metadata_value("encoder")
    }
}

impl Default for MediaInfo {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            format: String::new(),
            format_long_name: None,
            duration: Duration::ZERO,
            file_size: 0,
            bitrate: None,
            video_streams: Vec::new(),
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
            chapters: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Builder for constructing [`MediaInfo`].
///
/// # Examples
///
/// ```
/// use ff_format::media::MediaInfo;
/// use std::time::Duration;
///
/// let info = MediaInfo::builder()
///     .path("/path/to/video.mp4")
///     .format("mp4")
///     .format_long_name("QuickTime / MOV")
///     .duration(Duration::from_secs(120))
///     .file_size(1_000_000)
///     .bitrate(8_000_000)
///     .metadata("title", "Sample Video")
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct MediaInfoBuilder {
    path: PathBuf,
    format: String,
    format_long_name: Option<String>,
    duration: Duration,
    file_size: u64,
    bitrate: Option<u64>,
    video_streams: Vec<VideoStreamInfo>,
    audio_streams: Vec<AudioStreamInfo>,
    subtitle_streams: Vec<SubtitleStreamInfo>,
    chapters: Vec<ChapterInfo>,
    metadata: HashMap<String, String>,
}

impl MediaInfoBuilder {
    /// Sets the file path.
    #[must_use]
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets the container format name.
    #[must_use]
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    /// Sets the long format name.
    #[must_use]
    pub fn format_long_name(mut self, name: impl Into<String>) -> Self {
        self.format_long_name = Some(name.into());
        self
    }

    /// Sets the total duration.
    #[must_use]
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the file size in bytes.
    #[must_use]
    pub fn file_size(mut self, size: u64) -> Self {
        self.file_size = size;
        self
    }

    /// Sets the overall bitrate in bits per second.
    #[must_use]
    pub fn bitrate(mut self, bitrate: u64) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Adds a video stream.
    #[must_use]
    pub fn video_stream(mut self, stream: VideoStreamInfo) -> Self {
        self.video_streams.push(stream);
        self
    }

    /// Sets all video streams at once, replacing any existing streams.
    #[must_use]
    pub fn video_streams(mut self, streams: Vec<VideoStreamInfo>) -> Self {
        self.video_streams = streams;
        self
    }

    /// Adds an audio stream.
    #[must_use]
    pub fn audio_stream(mut self, stream: AudioStreamInfo) -> Self {
        self.audio_streams.push(stream);
        self
    }

    /// Sets all audio streams at once, replacing any existing streams.
    #[must_use]
    pub fn audio_streams(mut self, streams: Vec<AudioStreamInfo>) -> Self {
        self.audio_streams = streams;
        self
    }

    /// Adds a subtitle stream.
    #[must_use]
    pub fn subtitle_stream(mut self, stream: SubtitleStreamInfo) -> Self {
        self.subtitle_streams.push(stream);
        self
    }

    /// Sets all subtitle streams at once, replacing any existing streams.
    #[must_use]
    pub fn subtitle_streams(mut self, streams: Vec<SubtitleStreamInfo>) -> Self {
        self.subtitle_streams = streams;
        self
    }

    /// Adds a chapter.
    #[must_use]
    pub fn chapter(mut self, chapter: ChapterInfo) -> Self {
        self.chapters.push(chapter);
        self
    }

    /// Sets all chapters at once, replacing any existing chapters.
    #[must_use]
    pub fn chapters(mut self, chapters: Vec<ChapterInfo>) -> Self {
        self.chapters = chapters;
        self
    }

    /// Adds a metadata key-value pair.
    #[must_use]
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Sets all metadata at once, replacing any existing metadata.
    #[must_use]
    pub fn metadata_map(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Builds the [`MediaInfo`].
    #[must_use]
    pub fn build(self) -> MediaInfo {
        MediaInfo {
            path: self.path,
            format: self.format,
            format_long_name: self.format_long_name,
            duration: self.duration,
            file_size: self.file_size,
            bitrate: self.bitrate,
            video_streams: self.video_streams,
            audio_streams: self.audio_streams,
            subtitle_streams: self.subtitle_streams,
            chapters: self.chapters,
            metadata: self.metadata,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{AudioCodec, SubtitleCodec, VideoCodec};
    use crate::time::Rational;
    use crate::{PixelFormat, SampleFormat};

    fn sample_video_stream() -> VideoStreamInfo {
        VideoStreamInfo::builder()
            .index(0)
            .codec(VideoCodec::H264)
            .codec_name("h264")
            .width(1920)
            .height(1080)
            .frame_rate(Rational::new(30, 1))
            .pixel_format(PixelFormat::Yuv420p)
            .duration(Duration::from_secs(120))
            .build()
    }

    fn sample_audio_stream() -> AudioStreamInfo {
        AudioStreamInfo::builder()
            .index(1)
            .codec(AudioCodec::Aac)
            .codec_name("aac")
            .sample_rate(48000)
            .channels(2)
            .sample_format(SampleFormat::F32)
            .duration(Duration::from_secs(120))
            .build()
    }

    fn sample_subtitle_stream() -> SubtitleStreamInfo {
        SubtitleStreamInfo::builder()
            .index(2)
            .codec(SubtitleCodec::Srt)
            .codec_name("srt")
            .language("eng")
            .build()
    }

    mod media_info_tests {
        use super::*;

        #[test]
        fn test_builder_basic() {
            let info = MediaInfo::builder()
                .path("/path/to/video.mp4")
                .format("mp4")
                .duration(Duration::from_secs(120))
                .file_size(1_000_000)
                .build();

            assert_eq!(info.path(), Path::new("/path/to/video.mp4"));
            assert_eq!(info.format(), "mp4");
            assert_eq!(info.duration(), Duration::from_secs(120));
            assert_eq!(info.file_size(), 1_000_000);
            assert!(info.format_long_name().is_none());
            assert!(info.bitrate().is_none());
        }

        #[test]
        fn test_builder_full() {
            let video = sample_video_stream();
            let audio = sample_audio_stream();

            let info = MediaInfo::builder()
                .path("/path/to/video.mp4")
                .format("mp4")
                .format_long_name("QuickTime / MOV")
                .duration(Duration::from_secs(120))
                .file_size(150_000_000)
                .bitrate(10_000_000)
                .video_stream(video)
                .audio_stream(audio)
                .metadata("title", "Test Video")
                .metadata("artist", "Test Artist")
                .build();

            assert_eq!(info.format_long_name(), Some("QuickTime / MOV"));
            assert_eq!(info.bitrate(), Some(10_000_000));
            assert_eq!(info.video_stream_count(), 1);
            assert_eq!(info.audio_stream_count(), 1);
            assert_eq!(info.metadata_value("title"), Some("Test Video"));
            assert_eq!(info.metadata_value("artist"), Some("Test Artist"));
            assert!(info.metadata_value("nonexistent").is_none());
        }

        #[test]
        fn test_default() {
            let info = MediaInfo::default();
            assert_eq!(info.path(), Path::new(""));
            assert_eq!(info.format(), "");
            assert_eq!(info.duration(), Duration::ZERO);
            assert_eq!(info.file_size(), 0);
            assert!(!info.has_video());
            assert!(!info.has_audio());
        }

        #[test]
        fn test_has_streams() {
            // No streams
            let empty = MediaInfo::default();
            assert!(!empty.has_video());
            assert!(!empty.has_audio());

            // Video only
            let video_only = MediaInfo::builder()
                .video_stream(sample_video_stream())
                .build();
            assert!(video_only.has_video());
            assert!(!video_only.has_audio());
            assert!(video_only.is_video_only());
            assert!(!video_only.is_audio_only());

            // Audio only
            let audio_only = MediaInfo::builder()
                .audio_stream(sample_audio_stream())
                .build();
            assert!(!audio_only.has_video());
            assert!(audio_only.has_audio());
            assert!(!audio_only.is_video_only());
            assert!(audio_only.is_audio_only());

            // Both
            let both = MediaInfo::builder()
                .video_stream(sample_video_stream())
                .audio_stream(sample_audio_stream())
                .build();
            assert!(both.has_video());
            assert!(both.has_audio());
            assert!(!both.is_video_only());
            assert!(!both.is_audio_only());
        }

        #[test]
        fn test_primary_streams() {
            let video1 = VideoStreamInfo::builder()
                .index(0)
                .width(1920)
                .height(1080)
                .build();
            let video2 = VideoStreamInfo::builder()
                .index(2)
                .width(1280)
                .height(720)
                .build();
            let audio1 = AudioStreamInfo::builder()
                .index(1)
                .sample_rate(48000)
                .build();
            let audio2 = AudioStreamInfo::builder()
                .index(3)
                .sample_rate(44100)
                .build();

            let info = MediaInfo::builder()
                .video_stream(video1)
                .video_stream(video2)
                .audio_stream(audio1)
                .audio_stream(audio2)
                .build();

            // Primary should be first
            let primary_video = info.primary_video().unwrap();
            assert_eq!(primary_video.width(), 1920);
            assert_eq!(primary_video.index(), 0);

            let primary_audio = info.primary_audio().unwrap();
            assert_eq!(primary_audio.sample_rate(), 48000);
            assert_eq!(primary_audio.index(), 1);
        }

        #[test]
        fn test_stream_access_by_index() {
            let video1 = VideoStreamInfo::builder().width(1920).build();
            let video2 = VideoStreamInfo::builder().width(1280).build();
            let audio1 = AudioStreamInfo::builder().sample_rate(48000).build();

            let info = MediaInfo::builder()
                .video_stream(video1)
                .video_stream(video2)
                .audio_stream(audio1)
                .build();

            assert_eq!(info.video_stream(0).unwrap().width(), 1920);
            assert_eq!(info.video_stream(1).unwrap().width(), 1280);
            assert!(info.video_stream(2).is_none());

            assert_eq!(info.audio_stream(0).unwrap().sample_rate(), 48000);
            assert!(info.audio_stream(1).is_none());
        }

        #[test]
        fn test_resolution_and_frame_rate() {
            let info = MediaInfo::builder()
                .video_stream(sample_video_stream())
                .build();

            assert_eq!(info.resolution(), Some((1920, 1080)));
            assert!((info.frame_rate().unwrap() - 30.0).abs() < 0.001);

            // No video
            let no_video = MediaInfo::default();
            assert!(no_video.resolution().is_none());
            assert!(no_video.frame_rate().is_none());
        }

        #[test]
        fn test_sample_rate_and_channels() {
            let info = MediaInfo::builder()
                .audio_stream(sample_audio_stream())
                .build();

            assert_eq!(info.sample_rate(), Some(48000));
            assert_eq!(info.channels(), Some(2));

            // No audio
            let no_audio = MediaInfo::default();
            assert!(no_audio.sample_rate().is_none());
            assert!(no_audio.channels().is_none());
        }

        #[test]
        fn test_stream_counts() {
            let info = MediaInfo::builder()
                .video_stream(sample_video_stream())
                .video_stream(sample_video_stream())
                .audio_stream(sample_audio_stream())
                .audio_stream(sample_audio_stream())
                .audio_stream(sample_audio_stream())
                .build();

            assert_eq!(info.video_stream_count(), 2);
            assert_eq!(info.audio_stream_count(), 3);
            assert_eq!(info.stream_count(), 5);
        }

        #[test]
        fn has_subtitles_should_return_true_when_subtitle_streams_present() {
            let no_subs = MediaInfo::default();
            assert!(!no_subs.has_subtitles());
            assert_eq!(no_subs.subtitle_stream_count(), 0);

            let with_subs = MediaInfo::builder()
                .subtitle_stream(sample_subtitle_stream())
                .subtitle_stream(sample_subtitle_stream())
                .build();
            assert!(with_subs.has_subtitles());
            assert_eq!(with_subs.subtitle_stream_count(), 2);
        }

        #[test]
        fn subtitle_stream_count_should_be_included_in_stream_count() {
            let info = MediaInfo::builder()
                .video_stream(sample_video_stream())
                .audio_stream(sample_audio_stream())
                .subtitle_stream(sample_subtitle_stream())
                .build();
            assert_eq!(info.stream_count(), 3);
        }

        #[test]
        fn subtitle_stream_by_index_should_return_correct_stream() {
            let sub1 = SubtitleStreamInfo::builder()
                .index(2)
                .codec(SubtitleCodec::Srt)
                .language("eng")
                .build();
            let sub2 = SubtitleStreamInfo::builder()
                .index(3)
                .codec(SubtitleCodec::Ass)
                .language("jpn")
                .build();

            let info = MediaInfo::builder()
                .subtitle_stream(sub1)
                .subtitle_stream(sub2)
                .build();

            assert_eq!(info.subtitle_stream(0).unwrap().language(), Some("eng"));
            assert_eq!(info.subtitle_stream(1).unwrap().language(), Some("jpn"));
            assert!(info.subtitle_stream(2).is_none());
        }

        #[test]
        fn test_file_name_and_extension() {
            let info = MediaInfo::builder().path("/path/to/my_video.mp4").build();

            assert_eq!(info.file_name(), Some("my_video.mp4"));
            assert_eq!(info.extension(), Some("mp4"));

            // Empty path
            let empty = MediaInfo::default();
            assert!(empty.file_name().is_none());
            assert!(empty.extension().is_none());
        }

        #[test]
        fn test_metadata_operations() {
            let mut map = HashMap::new();
            map.insert("key1".to_string(), "value1".to_string());
            map.insert("key2".to_string(), "value2".to_string());

            let info = MediaInfo::builder()
                .metadata_map(map)
                .metadata("key3", "value3")
                .build();

            assert_eq!(info.metadata().len(), 3);
            assert_eq!(info.metadata_value("key1"), Some("value1"));
            assert_eq!(info.metadata_value("key2"), Some("value2"));
            assert_eq!(info.metadata_value("key3"), Some("value3"));
        }

        #[test]
        fn test_clone() {
            let info = MediaInfo::builder()
                .path("/path/to/video.mp4")
                .format("mp4")
                .format_long_name("QuickTime / MOV")
                .duration(Duration::from_secs(120))
                .file_size(1_000_000)
                .video_stream(sample_video_stream())
                .audio_stream(sample_audio_stream())
                .metadata("title", "Test")
                .build();

            let cloned = info.clone();
            assert_eq!(info.path(), cloned.path());
            assert_eq!(info.format(), cloned.format());
            assert_eq!(info.format_long_name(), cloned.format_long_name());
            assert_eq!(info.duration(), cloned.duration());
            assert_eq!(info.file_size(), cloned.file_size());
            assert_eq!(info.video_stream_count(), cloned.video_stream_count());
            assert_eq!(info.audio_stream_count(), cloned.audio_stream_count());
            assert_eq!(info.metadata_value("title"), cloned.metadata_value("title"));
        }

        #[test]
        fn test_debug() {
            let info = MediaInfo::builder()
                .path("/path/to/video.mp4")
                .format("mp4")
                .duration(Duration::from_secs(120))
                .file_size(1_000_000)
                .build();

            let debug = format!("{info:?}");
            assert!(debug.contains("MediaInfo"));
            assert!(debug.contains("mp4"));
        }

        #[test]
        fn test_video_streams_setter() {
            let streams = vec![sample_video_stream(), sample_video_stream()];

            let info = MediaInfo::builder().video_streams(streams).build();

            assert_eq!(info.video_stream_count(), 2);
        }

        #[test]
        fn test_audio_streams_setter() {
            let streams = vec![
                sample_audio_stream(),
                sample_audio_stream(),
                sample_audio_stream(),
            ];

            let info = MediaInfo::builder().audio_streams(streams).build();

            assert_eq!(info.audio_stream_count(), 3);
        }
    }

    mod media_info_builder_tests {
        use super::*;

        #[test]
        fn test_builder_default() {
            let builder = MediaInfoBuilder::default();
            let info = builder.build();
            assert_eq!(info.path(), Path::new(""));
            assert_eq!(info.format(), "");
            assert_eq!(info.duration(), Duration::ZERO);
        }

        #[test]
        fn test_builder_clone() {
            let builder = MediaInfo::builder()
                .path("/path/to/video.mp4")
                .format("mp4")
                .duration(Duration::from_secs(120));

            let cloned = builder.clone();
            let info1 = builder.build();
            let info2 = cloned.build();

            assert_eq!(info1.path(), info2.path());
            assert_eq!(info1.format(), info2.format());
            assert_eq!(info1.duration(), info2.duration());
        }

        #[test]
        fn test_builder_debug() {
            let builder = MediaInfo::builder()
                .path("/path/to/video.mp4")
                .format("mp4");

            let debug = format!("{builder:?}");
            assert!(debug.contains("MediaInfoBuilder"));
        }
    }

    mod metadata_convenience_tests {
        use super::*;

        #[test]
        fn test_title() {
            let info = MediaInfo::builder()
                .metadata("title", "Sample Video Title")
                .build();

            assert_eq!(info.title(), Some("Sample Video Title"));
        }

        #[test]
        fn test_title_missing() {
            let info = MediaInfo::default();
            assert!(info.title().is_none());
        }

        #[test]
        fn test_artist() {
            let info = MediaInfo::builder()
                .metadata("artist", "Test Artist")
                .build();

            assert_eq!(info.artist(), Some("Test Artist"));
        }

        #[test]
        fn test_album() {
            let info = MediaInfo::builder().metadata("album", "Test Album").build();

            assert_eq!(info.album(), Some("Test Album"));
        }

        #[test]
        fn test_creation_time() {
            let info = MediaInfo::builder()
                .metadata("creation_time", "2024-01-15T10:30:00.000000Z")
                .build();

            assert_eq!(info.creation_time(), Some("2024-01-15T10:30:00.000000Z"));
        }

        #[test]
        fn test_date() {
            let info = MediaInfo::builder().metadata("date", "2024-01-15").build();

            assert_eq!(info.date(), Some("2024-01-15"));
        }

        #[test]
        fn test_comment() {
            let info = MediaInfo::builder()
                .metadata("comment", "This is a test comment")
                .build();

            assert_eq!(info.comment(), Some("This is a test comment"));
        }

        #[test]
        fn test_encoder() {
            let info = MediaInfo::builder()
                .metadata("encoder", "Lavf58.76.100")
                .build();

            assert_eq!(info.encoder(), Some("Lavf58.76.100"));
        }

        #[test]
        fn test_multiple_metadata_fields() {
            let info = MediaInfo::builder()
                .metadata("title", "My Video")
                .metadata("artist", "John Doe")
                .metadata("album", "My Collection")
                .metadata("date", "2024")
                .metadata("comment", "A great video")
                .metadata("encoder", "FFmpeg")
                .metadata("custom_field", "custom_value")
                .build();

            assert_eq!(info.title(), Some("My Video"));
            assert_eq!(info.artist(), Some("John Doe"));
            assert_eq!(info.album(), Some("My Collection"));
            assert_eq!(info.date(), Some("2024"));
            assert_eq!(info.comment(), Some("A great video"));
            assert_eq!(info.encoder(), Some("FFmpeg"));
            assert_eq!(info.metadata_value("custom_field"), Some("custom_value"));
            assert_eq!(info.metadata().len(), 7);
        }
    }
}
