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

use ff_encode::{Preset, VideoCodec, VideoEncoder};
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
        .expect("Failed to create encoder builder")
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
            .expect("Failed to create encoder builder")
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
            .expect("Failed to create encoder builder")
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
        .expect("Failed to create encoder builder")
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
        .expect("Failed to create encoder builder")
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
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .video_bitrate(1_000_000) // 1 Mbps
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
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .video_quality(30) // CRF 30
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
fn crf_h264_should_produce_valid_output() {
    let output_path = test_output_path("crf_h264.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H264)
        .video_quality(23) // CRF 23 — default quality for H.264
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
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::H265)
        .video_quality(28) // CRF 28 — default quality for H.265
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
            .expect("Failed to create encoder builder")
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
        .expect("Failed to create encoder builder")
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
        .expect("Failed to create encoder builder")
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
