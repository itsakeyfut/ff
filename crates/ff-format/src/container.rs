//! Container-level media information.
//!
//! [`ContainerInfo`] exposes the fields that belong to the media container as a
//! whole (format name, overall bitrate, total stream count) rather than to any
//! individual stream. Both [`crate::stream::VideoStreamInfo`] and
//! [`crate::stream::AudioStreamInfo`] cover per-stream data; this type fills the
//! gap with the container-level layer.

/// Container-level metadata extracted from `AVFormatContext`.
///
/// Obtain an instance via `VideoDecoder::container_info` or
/// `AudioDecoder::container_info` from `ff-decode`.
///
/// # Examples
///
/// ```
/// use ff_format::ContainerInfo;
///
/// let info = ContainerInfo::builder()
///     .format_name("mov,mp4,m4a,3gp,3g2,mj2")
///     .bit_rate(2_048_000)
///     .nb_streams(2)
///     .build();
///
/// assert_eq!(info.format_name(), "mov,mp4,m4a,3gp,3g2,mj2");
/// assert_eq!(info.bit_rate(), Some(2_048_000));
/// assert_eq!(info.nb_streams(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerInfo {
    format_name: String,
    bit_rate: Option<u64>,
    nb_streams: u32,
}

impl ContainerInfo {
    /// Returns a builder for constructing a [`ContainerInfo`].
    #[must_use]
    pub fn builder() -> ContainerInfoBuilder {
        ContainerInfoBuilder::default()
    }

    /// Short format name as reported by `AVInputFormat::name`
    /// (e.g. `"mov,mp4,m4a,3gp,3g2,mj2"`, `"matroska,webm"`, `"mp3"`).
    ///
    /// Returns an empty string when the format context has no associated
    /// input format (e.g. raw streams or certain network sources).
    #[must_use]
    pub fn format_name(&self) -> &str {
        &self.format_name
    }

    /// Container-level bitrate in bits per second, or `None` when the
    /// container does not report one (live streams, raw formats, etc.).
    #[must_use]
    pub fn bit_rate(&self) -> Option<u64> {
        self.bit_rate
    }

    /// Total number of streams in the container (video + audio + subtitle + …).
    #[must_use]
    pub fn nb_streams(&self) -> u32 {
        self.nb_streams
    }
}

/// Builder for [`ContainerInfo`].
#[derive(Debug, Default)]
pub struct ContainerInfoBuilder {
    format_name: String,
    bit_rate: Option<u64>,
    nb_streams: u32,
}

impl ContainerInfoBuilder {
    /// Sets the short format name (e.g. `"mp3"`, `"mov,mp4,m4a,3gp,3g2,mj2"`).
    #[must_use]
    pub fn format_name(mut self, name: impl Into<String>) -> Self {
        self.format_name = name.into();
        self
    }

    /// Sets the container-level bitrate in bits per second.
    #[must_use]
    pub fn bit_rate(mut self, br: u64) -> Self {
        self.bit_rate = Some(br);
        self
    }

    /// Sets the total number of streams.
    #[must_use]
    pub fn nb_streams(mut self, n: u32) -> Self {
        self.nb_streams = n;
        self
    }

    /// Builds the [`ContainerInfo`].
    #[must_use]
    pub fn build(self) -> ContainerInfo {
        ContainerInfo {
            format_name: self.format_name,
            bit_rate: self.bit_rate,
            nb_streams: self.nb_streams,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_info_builder_sets_fields() {
        let info = ContainerInfo::builder()
            .format_name("mp4")
            .bit_rate(1_000_000)
            .nb_streams(3)
            .build();

        assert_eq!(info.format_name(), "mp4");
        assert_eq!(info.bit_rate(), Some(1_000_000));
        assert_eq!(info.nb_streams(), 3);
    }

    #[test]
    fn bit_rate_none_when_not_set() {
        let info = ContainerInfo::builder().format_name("mp3").build();
        assert_eq!(info.bit_rate(), None);
    }

    #[test]
    fn default_builder_produces_empty_info() {
        let info = ContainerInfo::builder().build();
        assert_eq!(info.format_name(), "");
        assert_eq!(info.bit_rate(), None);
        assert_eq!(info.nb_streams(), 0);
    }
}
