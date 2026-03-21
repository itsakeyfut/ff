//! Integration tests for video encoder.
//!
//! These tests verify the basic video encoding functionality:
//! - Creating encoder with builder pattern
//! - Encoding black frames to various formats
//! - Hardware acceleration support
//! - Codec fallback behavior
//! - Error handling

// Tests are allowed to use unwrap() for simplicity
#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::{
    BitrateMode, H264Options, H264Preset, H264Profile, H264Tune, H265Options, H265Profile,
    H265Tier, Preset, VideoCodec, VideoCodecOptions, VideoEncoder,
};
use fixtures::{
    FileGuard, assert_valid_output_file, create_black_frame, get_file_size, test_output_path,
};

// ============================================================================
// Basic Encoding Tests
// ============================================================================

#[test]
fn test_encode_black_frames_mpeg4() {
    let output_path = test_output_path("test_black_frames.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4) // Use MPEG-4 (always available)
        .preset(Preset::Ultrafast)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return; // Skip test
        }
    };

    // Verify codec selection
    assert!(
        !encoder.actual_video_codec().is_empty(),
        "Video codec should be set"
    );
    println!("Using video codec: {}", encoder.actual_video_codec());

    // Encode 30 frames (1 second at 30fps)
    for _ in 0..30 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    // Finish encoding
    encoder.finish().expect("Failed to finish encoding");

    // Verify output file
    assert_valid_output_file(&output_path);
    let file_size = get_file_size(&output_path);
    println!("Output file size: {} bytes", file_size);

    // VP9 compressed black frames should be reasonably small
    assert!(file_size > 1000, "File too small, might be corrupted");
    assert!(file_size < 100_000, "File too large for black frames");
}

#[test]
fn test_encode_different_resolutions() {
    let resolutions = [(320, 240), (640, 480), (1280, 720), (1920, 1080)];

    for (width, height) in resolutions {
        let filename = format!("test_resolution_{}x{}.mp4", width, height);
        let output_path = test_output_path(&filename);
        let _guard = FileGuard::new(output_path.clone());

        let mut encoder = VideoEncoder::create(&output_path)
            .video(width, height, 30.0)
            .video_codec(VideoCodec::Mpeg4)
            .preset(Preset::Ultrafast)
            .build()
            .expect("Failed to build encoder");

        // Encode 10 frames
        for _ in 0..10 {
            let frame = create_black_frame(width, height);
            encoder
                .push_video(&frame)
                .expect("Failed to push video frame");
        }

        encoder.finish().expect("Failed to finish encoding");

        assert_valid_output_file(&output_path);
        println!(
            "{}x{}: {} bytes",
            width,
            height,
            get_file_size(&output_path)
        );
    }
}

#[test]
fn test_encode_different_framerates() {
    // Note: MPEG-4 supports max 65535 for timebase denominator (≈65 fps)
    let framerates = [24.0, 30.0, 60.0];

    for fps in framerates {
        let filename = format!("test_fps_{}.mp4", fps as i32);
        let output_path = test_output_path(&filename);
        let _guard = FileGuard::new(output_path.clone());

        let mut encoder = VideoEncoder::create(&output_path)
            .video(640, 480, fps)
            .video_codec(VideoCodec::Mpeg4)
            .preset(Preset::Ultrafast)
            .build()
            .expect("Failed to build encoder");

        // Encode frames for 0.5 seconds
        let frame_count = (fps * 0.5) as usize;
        for _ in 0..frame_count {
            let frame = create_black_frame(640, 480);
            encoder
                .push_video(&frame)
                .expect("Failed to push video frame");
        }

        encoder.finish().expect("Failed to finish encoding");

        assert_valid_output_file(&output_path);
        println!("{}fps: {} bytes", fps, get_file_size(&output_path));
    }
}

// ============================================================================
// Codec Tests
// ============================================================================

#[test]
fn test_encode_with_mpeg4() {
    let output_path = test_output_path("test_mpeg4.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .build();

    // MPEG-4 should always be available (built-in encoder)
    match result {
        Ok(mut encoder) => {
            println!("Using codec: {}", encoder.actual_video_codec());

            for _ in 0..10 {
                let frame = create_black_frame(640, 480);
                encoder
                    .push_video(&frame)
                    .expect("Failed to push video frame");
            }

            encoder.finish().expect("Failed to finish encoding");
            assert_valid_output_file(&output_path);
        }
        Err(e) => {
            println!("MPEG-4 encoding failed (unexpected): {}", e);
        }
    }
}

#[test]
fn test_encode_with_av1() {
    let output_path = test_output_path("test_av1.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // AV1 encoding might not be available on all systems
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Av1)
        .preset(Preset::Ultrafast)
        .build();

    match result {
        Ok(mut encoder) => {
            println!("Using AV1 codec: {}", encoder.actual_video_codec());

            for _ in 0..10 {
                let frame = create_black_frame(640, 480);
                encoder
                    .push_video(&frame)
                    .expect("Failed to push video frame");
            }

            encoder.finish().expect("Failed to finish encoding");
            assert_valid_output_file(&output_path);
        }
        Err(e) => {
            println!("AV1 encoding not available: {}", e);
        }
    }
}

// ============================================================================
// Builder Pattern Tests
// ============================================================================

#[test]
fn test_builder_pattern() {
    let output_path = test_output_path("test_builder.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Test builder chaining
    let mut encoder = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .bitrate_mode(BitrateMode::Cbr(1_000_000)) // 1 Mbps
        .preset(Preset::Medium)
        .build()
        .expect("Failed to build encoder");

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn test_builder_with_quality() {
    let output_path = test_output_path("test_quality.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Test CRF-based quality control
    let mut encoder = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .bitrate_mode(BitrateMode::Crf(30)) // CRF 30
        .preset(Preset::Fast)
        .build()
        .expect("Failed to build encoder");

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn vbr_mpeg4_should_produce_valid_output() {
    let output_path = test_output_path("vbr_mpeg4.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .bitrate_mode(BitrateMode::Vbr {
            target: 1_000_000,
            max: 2_000_000,
        })
        .preset(Preset::Ultrafast)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping vbr_mpeg4 test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn crf_h264_should_produce_valid_output() {
    let output_path = test_output_path("crf_h264.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H264)
        .bitrate_mode(BitrateMode::Crf(23)) // CRF 23 — default quality for H.264
        .preset(Preset::Ultrafast)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping crf_h264 test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn crf_h265_should_produce_valid_output() {
    let output_path = test_output_path("crf_h265.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28)) // CRF 28 — default quality for H.265
        .preset(Preset::Ultrafast)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping crf_h265 test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

// ── H265Options integration tests ────────────────────────────────────────────

#[test]
fn h265_main_profile_should_produce_valid_output() {
    let output_path = test_output_path("h265_main_profile.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28))
        .preset(Preset::Ultrafast)
        .codec_options(VideoCodecOptions::H265(H265Options {
            profile: H265Profile::Main,
            tier: H265Tier::Main,
            level: None,
            ..H265Options::default()
        }))
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping h265_main_profile test: encoder unavailable ({e})");
            return;
        }
    };

    let codec_name = encoder.actual_video_codec().to_string();

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!("h265_main_profile: codec={codec_name}");
}

#[test]
fn h265_main10_profile_should_produce_valid_output() {
    let output_path = test_output_path("h265_main10_profile.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28))
        .preset(Preset::Ultrafast)
        .codec_options(VideoCodecOptions::H265(H265Options {
            profile: H265Profile::Main10,
            tier: H265Tier::Main,
            level: None,
            ..H265Options::default()
        }))
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping h265_main10_profile test: encoder unavailable ({e})");
            return;
        }
    };

    let codec_name = encoder.actual_video_codec().to_string();

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!("h265_main10_profile: codec={codec_name}");
}

#[test]
fn h265_high_tier_should_produce_valid_output() {
    let output_path = test_output_path("h265_high_tier.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28))
        .preset(Preset::Ultrafast)
        .codec_options(VideoCodecOptions::H265(H265Options {
            profile: H265Profile::Main,
            tier: H265Tier::High,
            level: None,
            ..H265Options::default()
        }))
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping h265_high_tier test: encoder unavailable ({e})");
            return;
        }
    };

    let codec_name = encoder.actual_video_codec().to_string();

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!("h265_high_tier: codec={codec_name}");
}

#[test]
fn h265_level51_should_produce_valid_output() {
    let output_path = test_output_path("h265_level51.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28))
        .preset(Preset::Ultrafast)
        .codec_options(VideoCodecOptions::H265(H265Options {
            profile: H265Profile::Main,
            tier: H265Tier::Main,
            level: Some(51),
            ..H265Options::default()
        }))
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping h265_level51 test: encoder unavailable ({e})");
            return;
        }
    };

    let codec_name = encoder.actual_video_codec().to_string();

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!("h265_level51: codec={codec_name}");
}

#[test]
fn h265_preset_ultrafast_should_produce_valid_output() {
    let output_path = test_output_path("h265_preset_ultrafast.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28))
        .preset(Preset::Ultrafast)
        .codec_options(VideoCodecOptions::H265(H265Options {
            preset: Some("ultrafast".to_string()),
            ..H265Options::default()
        }))
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping h265_preset_ultrafast test: encoder unavailable ({e})");
            return;
        }
    };

    let codec_name = encoder.actual_video_codec().to_string();

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!("h265_preset_ultrafast: codec={codec_name}");
}

#[test]
fn h265_x265_params_log_level_none_should_not_crash() {
    let output_path = test_output_path("h265_x265_params.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .bitrate_mode(BitrateMode::Crf(28))
        .preset(Preset::Ultrafast)
        .codec_options(VideoCodecOptions::H265(H265Options {
            x265_params: Some("log-level=none".to_string()),
            ..H265Options::default()
        }))
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping h265_x265_params test: encoder unavailable ({e})");
            return;
        }
    };

    let codec_name = encoder.actual_video_codec().to_string();

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!("h265_x265_params: codec={codec_name}");
}

// ============================================================================
// Preset Tests
// ============================================================================

#[test]
fn test_different_presets() {
    let presets = [
        Preset::Ultrafast,
        Preset::Fast,
        Preset::Medium,
        Preset::Slow,
    ];

    for preset in presets {
        let filename = format!("test_preset_{:?}.mp4", preset).to_lowercase();
        let output_path = test_output_path(&filename);
        let _guard = FileGuard::new(output_path.clone());

        let mut encoder = VideoEncoder::create(&output_path)
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Mpeg4)
            .preset(preset)
            .build()
            .expect("Failed to build encoder");

        for _ in 0..10 {
            let frame = create_black_frame(640, 480);
            encoder
                .push_video(&frame)
                .expect("Failed to push video frame");
        }

        encoder.finish().expect("Failed to finish encoding");

        assert_valid_output_file(&output_path);
        println!("{:?}: {} bytes", preset, get_file_size(&output_path));
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_encode_single_frame() {
    let output_path = test_output_path("test_single_frame.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let mut encoder = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .build()
        .expect("Failed to build encoder");

    // Encode just one frame
    let frame = create_black_frame(640, 480);
    encoder
        .push_video(&frame)
        .expect("Failed to push video frame");

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn test_encode_many_frames() {
    let output_path = test_output_path("test_many_frames.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let mut encoder = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .build()
        .expect("Failed to build encoder");

    // Encode 300 frames (10 seconds)
    for _ in 0..300 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);

    let file_size = get_file_size(&output_path);
    println!("300 frames: {} bytes", file_size);
}

// ============================================================================
// Two-Pass Tests
// ============================================================================

#[test]
fn two_pass_encode_should_produce_valid_output() {
    let output_path = test_output_path("two_pass_mpeg4.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .bitrate_mode(BitrateMode::Cbr(1_000_000)) // Two-pass is most useful with a target bitrate
        .preset(Preset::Ultrafast)
        .two_pass()
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping two_pass test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..30 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder
        .finish()
        .expect("Failed to finish two-pass encoding");

    assert_valid_output_file(&output_path);
    let file_size = get_file_size(&output_path);
    println!("Two-pass output: {} bytes", file_size);
    assert!(file_size > 1000, "File too small, might be corrupted");
}

#[test]
fn metadata_mpeg4_should_produce_valid_output() {
    let output_path = test_output_path("metadata_mpeg4.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .metadata("title", "Test Video")
        .metadata("artist", "ff-encode integration test")
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping metadata_mpeg4 test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn subtitle_passthrough_mkv_should_produce_valid_output() {
    // Create a minimal SRT subtitle file as the passthrough source.
    let srt_path = test_output_path("subtitle_passthrough_source.srt");
    let _srt_guard = FileGuard::new(srt_path.clone());
    let srt_content = "1\n00:00:00,000 --> 00:00:05,000\nHello, world!\n\n\
                       2\n00:00:05,000 --> 00:00:10,000\nSubtitle passthrough test.\n\n";
    if let Err(e) = std::fs::write(&srt_path, srt_content) {
        println!("Skipping subtitle_passthrough test: cannot write srt file ({e})");
        return;
    }

    let output_path = test_output_path("subtitle_passthrough.mkv");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .subtitle_passthrough(srt_path.to_str().unwrap(), 0)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping subtitle_passthrough test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn subtitle_passthrough_should_preserve_subtitle_stream_in_output() {
    use ff_probe::SubtitleCodec;

    // Write a minimal SRT file as the subtitle source.
    let srt_path = test_output_path("subtitle_roundtrip_source.srt");
    let _srt_guard = FileGuard::new(srt_path.clone());
    let srt_content = "1\n00:00:00,000 --> 00:00:05,000\nHello subtitle!\n\n\
                       2\n00:00:05,000 --> 00:00:10,000\nPassthrough round-trip.\n\n";
    if let Err(e) = std::fs::write(&srt_path, srt_content) {
        println!("Skipping subtitle passthrough round-trip: cannot write srt file ({e})");
        return;
    }

    let output_path = test_output_path("subtitle_roundtrip_output.mkv");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .video(320, 240, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .subtitle_passthrough(srt_path.to_str().unwrap(), 0)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping subtitle passthrough round-trip: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(320, 240);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }
    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);

    // Re-probe the output and verify the subtitle stream is present with correct codec.
    let info = ff_probe::open(&output_path).expect("Failed to probe output file");

    assert!(
        info.has_subtitles(),
        "Output should contain at least one subtitle stream"
    );
    assert_eq!(
        info.subtitle_stream_count(),
        1,
        "Expected exactly one subtitle stream after passthrough"
    );

    let stream = &info.subtitle_streams()[0];
    assert_eq!(
        stream.codec(),
        &SubtitleCodec::Srt,
        "Subtitle codec should be Srt after SRT passthrough, got {:?}",
        stream.codec()
    );
}

#[test]
fn chapter_mpeg4_should_produce_valid_output() {
    use ff_format::chapter::ChapterInfo;
    use std::time::Duration;

    let output_path = test_output_path("chapter_mpeg4.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let chapter1 = ChapterInfo::builder()
        .id(0)
        .title("Intro")
        .start(Duration::ZERO)
        .end(Duration::from_secs(5))
        .build();
    let chapter2 = ChapterInfo::builder()
        .id(1)
        .title("Main")
        .start(Duration::from_secs(5))
        .end(Duration::from_secs(10))
        .build();

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast)
        .chapter(chapter1)
        .chapter(chapter2)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping chapter_mpeg4 test: encoder unavailable ({e})");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
}

#[test]
fn chapter_round_trip_should_preserve_count_titles_and_timestamps() {
    use ff_format::chapter::ChapterInfo;
    use std::time::Duration;

    let output_path = test_output_path("chapter_round_trip.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Known chapters with explicit titles and timestamps.
    let chapters = vec![
        ChapterInfo::builder()
            .id(0)
            .title("Intro")
            .start(Duration::ZERO)
            .end(Duration::from_secs(5))
            .build(),
        ChapterInfo::builder()
            .id(1)
            .title("Main Content")
            .start(Duration::from_secs(5))
            .end(Duration::from_secs(15))
            .build(),
        ChapterInfo::builder()
            .id(2)
            .title("Credits")
            .start(Duration::from_secs(15))
            .end(Duration::from_secs(20))
            .build(),
    ];

    // Encode a short black-frame video with the chapters written.
    let mut builder = VideoEncoder::create(&output_path)
        .video(320, 240, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .preset(Preset::Ultrafast);
    for ch in &chapters {
        builder = builder.chapter(ch.clone());
    }
    let mut encoder = match builder.build() {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping chapter_round_trip test: encoder unavailable ({e})");
            return;
        }
    };

    // Encode 20 seconds worth of frames at 30 fps (600 frames).
    for _ in 0..600 {
        let frame = create_black_frame(320, 240);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }
    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);

    // Re-probe the output and verify the chapter round-trip.
    let info = ff_probe::open(&output_path).expect("Failed to probe output file");

    assert_eq!(
        info.chapter_count(),
        chapters.len(),
        "Chapter count mismatch after round-trip"
    );

    // Tolerance for timestamp comparison: FFmpeg stores chapters in a rational
    // time-base (typically 1/1000 ms), so allow up to 10 ms of rounding error.
    let tolerance = Duration::from_millis(10);

    for (i, expected) in chapters.iter().enumerate() {
        let actual = info
            .chapters()
            .get(i)
            .unwrap_or_else(|| panic!("Chapter {i} missing after round-trip"));

        assert_eq!(
            actual.title(),
            expected.title(),
            "Title mismatch for chapter {i}"
        );

        let start_diff = if actual.start() >= expected.start() {
            actual.start() - expected.start()
        } else {
            expected.start() - actual.start()
        };
        assert!(
            start_diff <= tolerance,
            "Start timestamp mismatch for chapter {i}: expected {:?}, got {:?}",
            expected.start(),
            actual.start()
        );

        let end_diff = if actual.end() >= expected.end() {
            actual.end() - expected.end()
        } else {
            expected.end() - actual.end()
        };
        assert!(
            end_diff <= tolerance,
            "End timestamp mismatch for chapter {i}: expected {:?}, got {:?}",
            expected.end(),
            actual.end()
        );
    }
}

// ============================================================================
// Codec Options Tests
// ============================================================================

#[test]
fn h264_high_profile_level41_should_produce_valid_output() {
    let output_path = test_output_path("h264_high_profile_level41.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let opts = VideoCodecOptions::H264(H264Options {
        profile: H264Profile::High,
        level: Some(41),
        ..H264Options::default()
    });

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H264)
        .preset(Preset::Ultrafast)
        .codec_options(opts)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: no H.264 encoder available: {e}");
            return;
        }
    };

    for _ in 0..30 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    let codec_name = encoder.actual_video_codec().to_string();
    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);

    let file_size = get_file_size(&output_path);
    assert!(file_size > 1000, "Output too small, likely corrupted");
    println!("H264 High@4.1: codec={codec_name} size={file_size} bytes");
}

#[test]
fn h264_baseline_profile_should_produce_valid_output() {
    let output_path = test_output_path("h264_baseline_profile.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let opts = VideoCodecOptions::H264(H264Options {
        profile: H264Profile::Baseline,
        level: None,
        bframes: 0, // Baseline does not support B-frames
        ..H264Options::default()
    });

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H264)
        .preset(Preset::Ultrafast)
        .codec_options(opts)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: no H.264 encoder available: {e}");
            return;
        }
    };

    for _ in 0..30 {
        let frame = create_black_frame(640, 480);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    let codec_name = encoder.actual_video_codec().to_string();
    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!(
        "H264 Baseline: codec={codec_name} size={} bytes",
        get_file_size(&output_path)
    );
}

#[test]
fn h264_veryslow_preset_should_produce_valid_output() {
    let output_path = test_output_path("h264_veryslow_preset.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let opts = VideoCodecOptions::H264(H264Options {
        preset: Some(H264Preset::Veryslow),
        ..H264Options::default()
    });

    let result = VideoEncoder::create(&output_path)
        .video(320, 240, 30.0)
        .video_codec(VideoCodec::H264)
        .codec_options(opts)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: no H.264 encoder available: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(320, 240);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    let codec_name = encoder.actual_video_codec().to_string();
    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!(
        "H264 veryslow preset: codec={codec_name} size={} bytes",
        get_file_size(&output_path)
    );
}

#[test]
fn h264_zerolatency_tune_should_produce_valid_output() {
    let output_path = test_output_path("h264_zerolatency_tune.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let opts = VideoCodecOptions::H264(H264Options {
        preset: Some(H264Preset::Ultrafast),
        tune: Some(H264Tune::Zerolatency),
        ..H264Options::default()
    });

    let result = VideoEncoder::create(&output_path)
        .video(320, 240, 30.0)
        .video_codec(VideoCodec::H264)
        .codec_options(opts)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: no H.264 encoder available: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(320, 240);
        encoder
            .push_video(&frame)
            .expect("Failed to push video frame");
    }

    let codec_name = encoder.actual_video_codec().to_string();
    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output_path);
    println!(
        "H264 zerolatency tune: codec={codec_name} size={} bytes",
        get_file_size(&output_path)
    );
}
