//! Integration tests for `SrtOutput` (feature = "srt").
//!
//! Tests that validate builder configuration without requiring a live SRT server.
//! When the linked `FFmpeg` build does not include libsrt, all connection tests are
//! skipped via the `ProtocolUnavailable` early-return path.

#[cfg(feature = "srt")]
mod srt_tests {
    use ff_format::{AudioCodec, VideoCodec};
    use ff_stream::{SrtOutput, StreamError};

    #[test]
    fn srt_build_without_srt_scheme_should_return_invalid_config() {
        // Skip if libsrt is not available (ProtocolUnavailable would be returned first).
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("rtmp://example.com/live")
            .video(1280, 720, 30.0)
            .build();
        assert!(
            matches!(result, Err(StreamError::InvalidConfig { .. })),
            "expected InvalidConfig for non-srt URL"
        );
    }

    #[test]
    fn srt_build_without_video_should_return_invalid_config() {
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000").build();
        assert!(
            matches!(result, Err(StreamError::InvalidConfig { .. })),
            "expected InvalidConfig when video() not called"
        );
    }

    #[test]
    fn srt_build_with_non_h264_video_codec_should_return_unsupported_codec() {
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000")
            .video(1280, 720, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(
            matches!(result, Err(StreamError::UnsupportedCodec { .. })),
            "expected UnsupportedCodec for VP9"
        );
    }

    #[test]
    fn srt_build_with_non_aac_audio_codec_should_return_unsupported_codec() {
        if !ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt not available in linked FFmpeg");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000")
            .video(1280, 720, 30.0)
            .audio_codec(AudioCodec::Mp3)
            .build();
        assert!(
            matches!(result, Err(StreamError::UnsupportedCodec { .. })),
            "expected UnsupportedCodec for MP3"
        );
    }

    #[test]
    fn srt_build_without_libsrt_should_return_protocol_unavailable() {
        if ff_sys::avformat::srt_available() {
            println!("Skipping: libsrt is available; cannot test ProtocolUnavailable path");
            return;
        }
        let result = SrtOutput::new("srt://127.0.0.1:9000")
            .video(1280, 720, 30.0)
            .build();
        assert!(
            matches!(result, Err(StreamError::ProtocolUnavailable { .. })),
            "expected ProtocolUnavailable when libsrt is absent"
        );
    }
}

// Ensure the file compiles even without the srt feature.
#[cfg(not(feature = "srt"))]
#[test]
fn srt_feature_not_enabled_is_expected() {
    println!("srt feature is not enabled; SrtOutput integration tests are skipped");
}
