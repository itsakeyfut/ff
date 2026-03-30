//! Frame-push SRT output.
//!
//! [`SrtOutput`] receives pre-decoded [`ff_format::VideoFrame`] / [`ff_format::AudioFrame`] values
//! from the caller, encodes them with H.264/AAC, and pushes the MPEG-TS
//! stream to an SRT destination using `FFmpeg`'s built-in SRT support.
//!
//! This module is only available when the `srt` feature flag is enabled.
//!
//! # Example
//!
//! ```ignore
//! use ff_stream::{SrtOutput, StreamOutput};
//!
//! let mut out = SrtOutput::new("srt://192.168.1.100:9000")
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

use ff_format::{AudioCodec, VideoCodec};

use crate::error::StreamError;
use crate::srt_output_inner::SrtInner;

/// Live SRT output: encodes frames as MPEG-TS and pushes them to an SRT
/// destination.
///
/// Build with [`SrtOutput::new`], chain setter methods, then call
/// [`build`](Self::build) to open the `FFmpeg` context and establish the SRT
/// connection. After `build()`:
///
/// - `push_video` and `push_audio`
///   encode and transmit frames in real time.
/// - [`crate::StreamOutput::finish`] flushes all encoders, writes the MPEG-TS
///   end-of-stream, and closes the SRT connection.
///
/// The SRT transport uses MPEG-TS as the container (H.264 video + AAC audio).
/// [`build`](Self::build) returns [`StreamError::UnsupportedCodec`] for any
/// other codec selection.
///
/// [`build`](Self::build) also performs a runtime check and returns
/// [`StreamError::ProtocolUnavailable`] when the linked `FFmpeg` library was
/// built without libsrt support.
pub struct SrtOutput {
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
    inner: Option<SrtInner>,
    finished: bool,
}

impl SrtOutput {
    /// Create a new builder that streams to the given SRT URL.
    ///
    /// The URL must begin with `srt://`; [`build`](Self::build) returns
    /// [`StreamError::InvalidConfig`] otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ff_stream::SrtOutput;
    ///
    /// let out = SrtOutput::new("srt://192.168.1.100:9000");
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

    /// Open the `FFmpeg` MPEG-TS context and establish the SRT connection.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::ProtocolUnavailable`] when the linked `FFmpeg`
    /// was built without libsrt support.
    ///
    /// Returns [`StreamError::InvalidConfig`] when:
    /// - The URL does not start with `srt://`.
    /// - [`video`](Self::video) was not called before `build`.
    ///
    /// Returns [`StreamError::UnsupportedCodec`] when:
    /// - The video codec is not [`VideoCodec::H264`].
    /// - The audio codec is not [`AudioCodec::Aac`].
    ///
    /// Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation fails
    /// (including network connection errors).
    pub fn build(mut self) -> Result<Self, StreamError> {
        if !ff_sys::avformat::srt_available() {
            return Err(StreamError::ProtocolUnavailable {
                reason: "FFmpeg was built without libsrt; recompile FFmpeg with --enable-libsrt"
                    .into(),
            });
        }

        if !self.url.starts_with("srt://") {
            return Err(StreamError::InvalidConfig {
                reason: "SrtOutput URL must start with srt://".into(),
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
                reason: "SRT/MPEG-TS output requires H.264 video".into(),
            });
        }

        if self.audio_codec != AudioCodec::Aac {
            return Err(StreamError::UnsupportedCodec {
                codec: format!("{:?}", self.audio_codec),
                reason: "SRT/MPEG-TS output requires AAC audio".into(),
            });
        }

        #[allow(clippy::cast_possible_truncation)]
        let fps_int = fps.round().max(1.0) as i32;

        let inner = SrtInner::open(
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

impl_live_stream_setters!(SrtOutput, required_audio);
impl_frame_push_stream_output!(SrtOutput);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_without_srt_scheme_should_return_invalid_config() {
        // Skip if libsrt is not available (ProtocolUnavailable would be returned first).
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("rtmp://example.com/live")
            .video(1280, 720, 30.0)
            .build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_without_video_should_return_invalid_config() {
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000").build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_non_h264_video_codec_should_return_unsupported_codec() {
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000")
            .video(1280, 720, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(matches!(result, Err(StreamError::UnsupportedCodec { .. })));
    }

    #[test]
    fn build_with_non_aac_audio_codec_should_return_unsupported_codec() {
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000")
            .video(1280, 720, 30.0)
            .audio_codec(AudioCodec::Mp3)
            .build();
        assert!(matches!(result, Err(StreamError::UnsupportedCodec { .. })));
    }

    #[test]
    fn build_without_libsrt_should_return_protocol_unavailable() {
        if ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt is available; cannot test ProtocolUnavailable path");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000")
            .video(1280, 720, 30.0)
            .build();
        assert!(matches!(
            result,
            Err(StreamError::ProtocolUnavailable { .. })
        ));
    }

    #[test]
    fn video_bitrate_default_should_be_four_megabits() {
        let out = SrtOutput::new("srt://127.0.0.1:9000");
        assert_eq!(out.video_bitrate, 4_000_000);
    }

    #[test]
    fn audio_defaults_should_be_44100hz_stereo() {
        let out = SrtOutput::new("srt://127.0.0.1:9000");
        assert_eq!(out.sample_rate, 44100);
        assert_eq!(out.channels, 2);
    }
}
