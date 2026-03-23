//! Frame-push RTMP output.
//!
//! [`RtmpOutput`] receives pre-decoded [`VideoFrame`] / [`AudioFrame`] values
//! from the caller, encodes them with H.264/AAC, and pushes the stream to an
//! RTMP ingest endpoint using `FFmpeg`'s built-in RTMP support.
//!
//! # Example
//!
//! ```ignore
//! use ff_stream::{RtmpOutput, StreamOutput};
//!
//! let mut out = RtmpOutput::new("rtmp://ingest.example.com/live/stream_key")
//!     .video(1920, 1080, 30.0)
//!     .audio(44100, 2)
//!     .video_bitrate(4_000_000)
//!     .audio_bitrate(128_000)
//!     .build()?;
//!
//! // for each decoded frame:
//! out.push_video(&video_frame)?;
//! out.push_audio(&audio_frame)?;
//!
//! // when done:
//! Box::new(out).finish()?;
//! ```

use ff_format::{AudioCodec, AudioFrame, VideoCodec, VideoFrame};

use crate::error::StreamError;
use crate::output::StreamOutput;
use crate::rtmp_inner::RtmpInner;

// ============================================================================
// RtmpOutput — safe builder + StreamOutput impl
// ============================================================================

/// Live RTMP output: encodes frames and pushes them to an RTMP ingest endpoint.
///
/// Build with [`RtmpOutput::new`], chain setter methods, then call
/// [`build`](Self::build) to open the `FFmpeg` context and establish the
/// RTMP connection. After `build()`:
///
/// - [`push_video`](Self::push_video) and [`push_audio`](Self::push_audio) encode and
///   transmit frames in real time.
/// - [`StreamOutput::finish`] flushes all encoders, sends the FLV end-of-stream
///   marker, and closes the RTMP connection.
///
/// RTMP/FLV requires H.264 video and AAC audio; [`build`](Self::build) returns
/// [`StreamError::UnsupportedCodec`] for any other codec selection.
pub struct RtmpOutput {
    url: String,
    video_width: Option<u32>,
    video_height: Option<u32>,
    fps: Option<f64>,
    sample_rate: u32,
    channels: u32,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,
    video_bitrate: u64,
    audio_bitrate: u64,
    inner: Option<RtmpInner>,
    finished: bool,
}

impl RtmpOutput {
    /// Create a new builder that streams to the given RTMP URL.
    ///
    /// The URL must begin with `rtmp://`; [`build`](Self::build) returns
    /// [`StreamError::InvalidConfig`] otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ff_stream::RtmpOutput;
    ///
    /// let out = RtmpOutput::new("rtmp://ingest.example.com/live/key");
    /// ```
    #[must_use]
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            video_width: None,
            video_height: None,
            fps: None,
            sample_rate: 44100,
            channels: 2,
            video_codec: VideoCodec::H264,
            audio_codec: AudioCodec::Aac,
            video_bitrate: 4_000_000,
            audio_bitrate: 128_000,
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

    /// Set the audio sample rate and channel count.
    ///
    /// Defaults: 44 100 Hz, 2 channels (stereo).
    #[must_use]
    pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
        self.sample_rate = sample_rate;
        self.channels = channels;
        self
    }

    /// Set the video codec.
    ///
    /// Default: [`VideoCodec::H264`]. Only `H264` is accepted by
    /// [`build`](Self::build); any other value returns
    /// [`StreamError::UnsupportedCodec`].
    #[must_use]
    pub fn video_codec(mut self, codec: VideoCodec) -> Self {
        self.video_codec = codec;
        self
    }

    /// Set the audio codec.
    ///
    /// Default: [`AudioCodec::Aac`]. Only `Aac` is accepted by
    /// [`build`](Self::build); any other value returns
    /// [`StreamError::UnsupportedCodec`].
    #[must_use]
    pub fn audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = codec;
        self
    }

    /// Set the video encoder target bit rate in bits/s.
    ///
    /// Default: 4 000 000 (4 Mbit/s).
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

    /// Open the `FFmpeg` context and establish the RTMP connection.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::InvalidConfig`] when:
    /// - The URL does not start with `rtmp://`.
    /// - [`video`](Self::video) was not called before `build`.
    ///
    /// Returns [`StreamError::UnsupportedCodec`] when:
    /// - The video codec is not [`VideoCodec::H264`].
    /// - The audio codec is not [`AudioCodec::Aac`].
    ///
    /// Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation fails
    /// (including network connection errors).
    pub fn build(mut self) -> Result<Self, StreamError> {
        if !self.url.starts_with("rtmp://") {
            return Err(StreamError::InvalidConfig {
                reason: "RtmpOutput URL must start with rtmp://".into(),
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

        if self.video_codec != VideoCodec::H264 {
            return Err(StreamError::UnsupportedCodec {
                codec: format!("{:?}", self.video_codec),
                reason: "RTMP/FLV requires H.264 video".into(),
            });
        }

        if self.audio_codec != AudioCodec::Aac {
            return Err(StreamError::UnsupportedCodec {
                codec: format!("{:?}", self.audio_codec),
                reason: "RTMP/FLV requires AAC audio".into(),
            });
        }

        #[allow(clippy::cast_possible_truncation)]
        let fps_int = fps.round().max(1.0) as i32;

        let inner = RtmpInner::open(
            &self.url,
            width.cast_signed(),
            height.cast_signed(),
            fps_int,
            self.video_bitrate,
            self.sample_rate.cast_signed(),
            self.channels.cast_signed(),
            self.audio_bitrate.cast_signed(),
        )?;

        self.inner = Some(inner);
        Ok(self)
    }
}

// ============================================================================
// StreamOutput impl
// ============================================================================

impl StreamOutput for RtmpOutput {
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
    fn build_without_rtmp_scheme_should_return_invalid_config() {
        let result = RtmpOutput::new("http://example.com/live")
            .video(1280, 720, 30.0)
            .build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_without_video_should_return_invalid_config() {
        let result = RtmpOutput::new("rtmp://localhost/live/key").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_non_h264_video_codec_should_return_unsupported_codec() {
        let result = RtmpOutput::new("rtmp://localhost/live/key")
            .video(1280, 720, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(matches!(result, Err(StreamError::UnsupportedCodec { .. })));
    }

    #[test]
    fn build_with_non_aac_audio_codec_should_return_unsupported_codec() {
        let result = RtmpOutput::new("rtmp://localhost/live/key")
            .video(1280, 720, 30.0)
            .audio_codec(AudioCodec::Mp3)
            .build();
        assert!(matches!(result, Err(StreamError::UnsupportedCodec { .. })));
    }

    #[test]
    fn video_bitrate_default_should_be_four_megabits() {
        let out = RtmpOutput::new("rtmp://localhost/live/key");
        assert_eq!(out.video_bitrate, 4_000_000);
    }

    #[test]
    fn audio_defaults_should_be_44100hz_stereo() {
        let out = RtmpOutput::new("rtmp://localhost/live/key");
        assert_eq!(out.sample_rate, 44100);
        assert_eq!(out.channels, 2);
    }
}
