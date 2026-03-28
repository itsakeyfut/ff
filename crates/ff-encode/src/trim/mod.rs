//! Stream-copy trimming — cut a media file to a time range without re-encoding.

mod trim_inner;

use std::path::PathBuf;

use crate::error::EncodeError;

/// Trim a media file to a time range using stream copy (no re-encode).
///
/// Uses [`avformat_seek_file`] to seek to the start point, then copies packets
/// until the presentation timestamp exceeds the end point.  All streams
/// (video, audio, subtitles) are copied verbatim from the input.
///
/// # Example
///
/// ```ignore
/// use ff_encode::StreamCopyTrimmer;
///
/// StreamCopyTrimmer::new("input.mp4", 2.0, 7.0, "output.mp4")
///     .run()?;
/// ```
///
/// [`avformat_seek_file`]: https://ffmpeg.org/doxygen/trunk/group__lavf__decoding.html
pub struct StreamCopyTrimmer {
    input: PathBuf,
    output: PathBuf,
    start_sec: f64,
    end_sec: f64,
}

impl StreamCopyTrimmer {
    /// Create a new `StreamCopyTrimmer`.
    ///
    /// `start_sec` and `end_sec` are absolute timestamps in seconds measured
    /// from the start of the source file.  [`run`](Self::run) returns
    /// [`EncodeError::InvalidConfig`] if `start_sec >= end_sec`.
    pub fn new(
        input: impl Into<PathBuf>,
        start_sec: f64,
        end_sec: f64,
        output: impl Into<PathBuf>,
    ) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            start_sec,
            end_sec,
        }
    }

    /// Execute the trim operation.
    ///
    /// # Errors
    ///
    /// - [`EncodeError::InvalidConfig`] if `start_sec >= end_sec`.
    /// - [`EncodeError::Ffmpeg`] if any FFmpeg API call fails.
    pub fn run(self) -> Result<(), EncodeError> {
        if self.start_sec >= self.end_sec {
            return Err(EncodeError::InvalidConfig {
                reason: format!(
                    "start_sec ({}) must be less than end_sec ({})",
                    self.start_sec, self.end_sec
                ),
            });
        }
        log::debug!(
            "stream copy trim start input={} output={} start_sec={} end_sec={}",
            self.input.display(),
            self.output.display(),
            self.start_sec,
            self.end_sec,
        );
        trim_inner::run_trim(&self.input, &self.output, self.start_sec, self.end_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_copy_trimmer_should_reject_start_greater_than_end() {
        let result = StreamCopyTrimmer::new("input.mp4", 7.0, 2.0, "output.mp4").run();
        assert!(
            matches!(result, Err(EncodeError::InvalidConfig { .. })),
            "expected InvalidConfig for start > end, got {result:?}"
        );
    }

    #[test]
    fn stream_copy_trimmer_should_reject_equal_start_and_end() {
        let result = StreamCopyTrimmer::new("input.mp4", 5.0, 5.0, "output.mp4").run();
        assert!(
            matches!(result, Err(EncodeError::InvalidConfig { .. })),
            "expected InvalidConfig for start == end, got {result:?}"
        );
    }
}
