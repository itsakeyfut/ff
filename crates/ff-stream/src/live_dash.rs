//! Frame-push live DASH output.
//!
//! [`LiveDashOutput`] receives pre-decoded [`ff_format::VideoFrame`] / [`ff_format::AudioFrame`] values
//! from the caller, encodes them with H.264/AAC, and muxes them into a DASH
//! manifest (`manifest.mpd`) backed by `.m4s` segment files.
//!
//! # Example
//!
//! ```ignore
//! use ff_stream::{LiveDashOutput, StreamOutput};
//! use std::time::Duration;
//!
//! let mut out = LiveDashOutput::new("/var/www/live")
//!     .video(1280, 720, 30.0)
//!     .audio(48000, 2)
//!     .segment_duration(Duration::from_secs(4))
//!     .build()?;
//!
//! // for each decoded frame:
//! out.push_video(&video_frame)?;
//! out.push_audio(&audio_frame)?;
//!
//! // when done:
//! Box::new(out).finish()?;
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use ff_format::{AudioCodec, VideoCodec};

use crate::error::StreamError;
use crate::live_dash_inner::LiveDashInner;

/// Live DASH output: receives frames and writes a `manifest.mpd` playlist.
///
/// Build with [`LiveDashOutput::new`], chain setter methods, then call
/// [`build`](Self::build) to open the `FFmpeg` contexts. After `build()`:
///
/// - `push_video` and `push_audio` encode and
///   mux frames in real time.
/// - [`crate::StreamOutput::finish`] flushes all encoders and writes the DASH trailer.
///
/// The output directory is created automatically by `build()` if it does not exist.
pub struct LiveDashOutput {
    output_dir: PathBuf,
    segment_duration: Duration,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,
    video_bitrate: u64,
    audio_bitrate: u64,
    video_width: Option<u32>,
    video_height: Option<u32>,
    fps: Option<f64>,
    sample_rate: Option<u32>,
    channels: Option<u32>,
    inner: Option<LiveDashInner>,
    finished: bool,
}

impl LiveDashOutput {
    /// Create a new builder that writes DASH output to `output_dir`.
    ///
    /// Accepts any path-like value: `"/var/www/live"`, `Path::new(…)`, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ff_stream::LiveDashOutput;
    ///
    /// let out = LiveDashOutput::new("/var/www/live");
    /// ```
    #[must_use]
    pub fn new(output_dir: impl AsRef<Path>) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            segment_duration: Duration::from_secs(4),
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
            video_bitrate: 2_000_000,
            audio_bitrate: 128_000,
            video_width: None,
            video_height: None,
            fps: None,
            sample_rate: None,
            channels: None,
            inner: None,
            finished: false,
        }
    }

    /// Set the target DASH segment duration.
    ///
    /// Default: 4 seconds.
    #[must_use]
    pub fn segment_duration(mut self, duration: Duration) -> Self {
        self.segment_duration = duration;
        self
    }

    /// Open all `FFmpeg` contexts and write the DASH header.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::InvalidConfig`] when:
    /// - `output_dir` is empty.
    /// - [`video`](Self::video) was not called before `build`.
    ///
    /// Returns [`StreamError::Io`] when the output directory cannot be created.
    /// Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation fails.
    pub fn build(mut self) -> Result<Self, StreamError> {
        if self.output_dir.as_os_str().is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "output_dir must not be empty".into(),
            });
        }

        let (Some(width), Some(height), Some(fps)) =
            (self.video_width, self.video_height, self.fps)
        else {
            return Err(StreamError::InvalidConfig {
                reason: "video parameters not set; call .video(width, height, fps) before .build()"
                    .into(),
            });
        };

        std::fs::create_dir_all(&self.output_dir)?;

        let output_dir = self
            .output_dir
            .to_str()
            .ok_or_else(|| StreamError::InvalidConfig {
                reason: "output_dir contains non-UTF-8 characters".into(),
            })?
            .to_owned();

        #[allow(clippy::cast_possible_truncation)]
        let fps_int = fps.round().max(1.0) as i32;
        #[allow(clippy::cast_possible_truncation)]
        let segment_secs = self.segment_duration.as_secs().max(1) as u32;

        let audio_params = self.sample_rate.zip(self.channels).map(|(sr, nc)| {
            (
                sr.cast_signed(),
                nc.cast_signed(),
                self.audio_bitrate.cast_signed(),
            )
        });

        let inner = LiveDashInner::open(
            &output_dir,
            segment_secs,
            width.cast_signed(),
            height.cast_signed(),
            fps_int,
            self.video_bitrate,
            audio_params,
        )?;

        self.inner = Some(inner);
        Ok(self)
    }
}

impl_live_stream_setters!(LiveDashOutput, optional_audio);
impl_frame_push_stream_output!(LiveDashOutput);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_without_video_should_return_invalid_config() {
        let result = LiveDashOutput::new("/tmp/live_dash_test_no_video").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_empty_output_dir_should_return_invalid_config() {
        let result = LiveDashOutput::new("").video(1280, 720, 30.0).build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn segment_duration_default_should_be_four_seconds() {
        let out = LiveDashOutput::new("/tmp/x");
        assert_eq!(out.segment_duration, Duration::from_secs(4));
    }
}
