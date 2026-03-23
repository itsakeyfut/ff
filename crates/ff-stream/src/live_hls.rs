//! Frame-push live HLS output.
//!
//! [`LiveHlsOutput`] receives pre-decoded [`VideoFrame`] / [`AudioFrame`] values
//! from the caller, encodes them with H.264/AAC, and muxes them into a sliding-
//! window HLS playlist (`index.m3u8`) backed by `.ts` segment files.
//!
//! # Example
//!
//! ```ignore
//! use ff_stream::{LiveHlsOutput, StreamOutput};
//! use std::time::Duration;
//!
//! let mut out = LiveHlsOutput::new("/var/www/live")
//!     .video(1280, 720, 30.0)
//!     .audio(48000, 2)
//!     .segment_duration(Duration::from_secs(4))
//!     .playlist_size(5)
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

use ff_format::{AudioCodec, AudioFrame, VideoCodec, VideoFrame};

use crate::error::StreamError;
use crate::live_hls_inner::LiveHlsInner;
use crate::output::StreamOutput;

// ============================================================================
// LiveHlsOutput — safe builder + StreamOutput impl
// ============================================================================

/// Live HLS output: receives frames and writes a sliding-window `.m3u8` playlist.
///
/// Build with [`LiveHlsOutput::new`], chain setter methods, then call
/// [`build`](Self::build) to open the `FFmpeg` contexts. After `build()`:
///
/// - [`push_video`](Self::push_video) and [`push_audio`](Self::push_audio) encode and
///   mux frames in real time.
/// - [`StreamOutput::finish`] flushes all encoders and writes the HLS trailer.
///
/// The output directory is created automatically by `build()` if it does not exist.
pub struct LiveHlsOutput {
    output_dir: PathBuf,
    segment_duration: Duration,
    playlist_size: u32,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,
    video_bitrate: u64,
    audio_bitrate: u64,
    video_width: Option<u32>,
    video_height: Option<u32>,
    fps: Option<f64>,
    sample_rate: Option<u32>,
    channels: Option<u32>,
    inner: Option<LiveHlsInner>,
    finished: bool,
}

impl LiveHlsOutput {
    /// Create a new builder that writes HLS output to `output_dir`.
    ///
    /// Accepts any path-like value: `"/var/www/live"`, `Path::new(…)`, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ff_stream::LiveHlsOutput;
    ///
    /// let out = LiveHlsOutput::new("/var/www/live");
    /// ```
    #[must_use]
    pub fn new(output_dir: impl AsRef<Path>) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            segment_duration: Duration::from_secs(6),
            playlist_size: 5,
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

    /// Set the video encoding parameters.
    ///
    /// This method **must** be called before [`build`](Self::build).
    #[must_use]
    pub fn video(mut self, width: u32, height: u32, fps: f64) -> Self {
        self.video_width = Some(width);
        self.video_height = Some(height);
        self.fps = Some(fps);
        self
    }

    /// Enable audio output with the given sample rate and channel count.
    ///
    /// If this method is not called, audio is disabled.
    #[must_use]
    pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
        self.sample_rate = Some(sample_rate);
        self.channels = Some(channels);
        self
    }

    /// Set the target HLS segment duration.
    ///
    /// Default: 6 seconds.
    #[must_use]
    pub fn segment_duration(mut self, duration: Duration) -> Self {
        self.segment_duration = duration;
        self
    }

    /// Set the maximum number of segments kept in the sliding-window playlist.
    ///
    /// Default: 5.
    #[must_use]
    pub fn playlist_size(mut self, size: u32) -> Self {
        self.playlist_size = size;
        self
    }

    /// Set the video codec.
    ///
    /// Default: [`VideoCodec::H264`].
    #[must_use]
    pub fn video_codec(mut self, codec: VideoCodec) -> Self {
        self.video_codec = codec;
        self
    }

    /// Set the audio codec.
    ///
    /// Default: [`AudioCodec::Aac`].
    #[must_use]
    pub fn audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = codec;
        self
    }

    /// Set the video encoder target bit rate in bits/s.
    ///
    /// Default: 2 000 000 (2 Mbit/s).
    #[must_use]
    pub fn video_bitrate(mut self, bitrate: u64) -> Self {
        self.video_bitrate = bitrate;
        self
    }

    /// Set the audio encoder target bit rate in bits/s.
    ///
    /// Default: 128 000 (128 kbit/s).
    #[must_use]
    pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
        self.audio_bitrate = bitrate;
        self
    }

    /// Open all `FFmpeg` contexts and write the HLS header.
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

        let inner = LiveHlsInner::open(
            &output_dir,
            segment_secs,
            self.playlist_size,
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

// ============================================================================
// StreamOutput impl
// ============================================================================

impl StreamOutput for LiveHlsOutput {
    fn push_video(&mut self, frame: &VideoFrame) -> Result<(), StreamError> {
        if self.finished {
            return Err(StreamError::InvalidConfig {
                reason: "push_video called after finish()".into(),
            });
        }
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| StreamError::InvalidConfig {
                reason: "push_video called before build()".into(),
            })?;
        inner.push_video(frame)
    }

    fn push_audio(&mut self, frame: &AudioFrame) -> Result<(), StreamError> {
        if self.finished {
            return Err(StreamError::InvalidConfig {
                reason: "push_audio called after finish()".into(),
            });
        }
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| StreamError::InvalidConfig {
                reason: "push_audio called before build()".into(),
            })?;
        inner.push_audio(frame);
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), StreamError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let inner = self
            .inner
            .take()
            .ok_or_else(|| StreamError::InvalidConfig {
                reason: "finish() called before build()".into(),
            })?;
        inner.flush_and_close();
        Ok(())
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_without_video_should_return_invalid_config() {
        let result = LiveHlsOutput::new("/tmp/live_hls_test_no_video").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_empty_output_dir_should_return_invalid_config() {
        let result = LiveHlsOutput::new("").video(1280, 720, 30.0).build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn segment_duration_default_should_be_six_seconds() {
        let out = LiveHlsOutput::new("/tmp/x");
        assert_eq!(out.segment_duration, Duration::from_secs(6));
    }

    #[test]
    fn playlist_size_default_should_be_five() {
        let out = LiveHlsOutput::new("/tmp/x");
        assert_eq!(out.playlist_size, 5);
    }
}
