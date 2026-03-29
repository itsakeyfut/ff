//! Error handling tests for video encoder.
//!
//! These tests verify that the encoder properly handles error conditions:
//! - Invalid file paths
//! - Invalid configurations
//! - Encoding without initialization
//! - Double finish calls

// Tests are allowed to use unwrap() for simplicity
#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::{BitrateMode, EncodeError, VideoCodec, VideoEncoder};
use fixtures::{create_black_frame, test_output_path};

// ============================================================================
// Configuration Error Tests
// ============================================================================

#[test]
fn test_invalid_output_path() {
    // create() is now infallible; validation happens at build() time.
    let build_result = VideoEncoder::create("/invalid/path\0.mp4")
        .video(640, 480, 30.0)
        .build();
    // Build might fail due to invalid path (null byte in CString, or missing dir).
    if build_result.is_ok() {
        println!("Path with null byte was accepted (system-dependent)");
    }
}

#[test]
fn test_encoder_without_video_config() {
    let output_path = test_output_path("test_no_video.mp4");

    // Build without calling .video() — must fail validation.
    let result = VideoEncoder::create(&output_path).build();

    assert!(
        result.is_err(),
        "Should fail when no video or audio stream is configured"
    );

    if let Err(e) = result {
        match e {
            EncodeError::InvalidConfig { reason } => {
                assert!(reason.contains("stream") || reason.contains("configured"));
            }
            e => panic!("Expected InvalidConfig error, got: {:?}", e),
        }
    }
}

#[test]
fn test_zero_dimensions() {
    let output_path = test_output_path("test_zero_dims.mp4");

    let result = VideoEncoder::create(&output_path)
        .video(0, 480, 30.0)
        .build();
    assert!(result.is_err(), "Should fail with zero width");

    let result = VideoEncoder::create(&output_path)
        .video(640, 0, 30.0)
        .build();
    assert!(result.is_err(), "Should fail with zero height");
}

#[test]
fn test_zero_framerate() {
    let output_path = test_output_path("test_zero_fps.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 0.0)
        .build();
    assert!(result.is_err(), "Should fail with zero framerate");
}

#[test]
fn test_negative_framerate() {
    let output_path = test_output_path("test_negative_fps.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, -30.0)
        .build();
    assert!(result.is_err(), "Should fail with negative framerate");
}

// ============================================================================
// Runtime Error Tests
// ============================================================================

#[test]
fn test_finish_without_frames() {
    let output_path = test_output_path("test_finish_no_frames.mp4");

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(encoder) => {
            let finish_result = encoder.finish();
            assert!(
                finish_result.is_ok(),
                "Should be able to finish encoder even without frames"
            );
        }
        Err(e) => {
            println!(
                "Encoder creation failed (no suitable codec available): {}",
                e
            );
        }
    }
}

#[test]
fn test_push_video_with_wrong_dimensions() {
    let output_path = test_output_path("test_wrong_dimensions.mp4");

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(mut encoder) => {
            let frame = create_black_frame(320, 240);
            let push_result = encoder.push_video(&frame);
            match push_result {
                Ok(_) => println!("Frame accepted with automatic scaling"),
                Err(e) => println!("Frame rejected as expected: {}", e),
            }
            let _ = encoder.finish();
        }
        Err(e) => {
            println!(
                "Encoder creation failed (no suitable codec available): {}",
                e
            );
        }
    }
}

// ============================================================================
// Codec Error Tests
// ============================================================================

#[test]
fn test_unsupported_codec_fallback() {
    let output_path = test_output_path("test_codec_fallback.mp4");

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(encoder) => {
            let codec = encoder.actual_video_codec();
            println!("Selected codec: {}", codec);
            assert!(!codec.is_empty());
        }
        Err(e) => {
            println!("Codec selection failed: {}", e);
        }
    }
}

// ============================================================================
// File System Error Tests
// ============================================================================

#[test]
fn test_readonly_directory() {
    #[cfg(unix)]
    {
        let result = VideoEncoder::create("/test_readonly.mp4")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        assert!(
            result.is_err(),
            "Should fail when trying to write to read-only directory"
        );
    }

    #[cfg(windows)]
    {
        let result = VideoEncoder::create("C:\\test_readonly.mp4")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();
        match result {
            Ok(encoder) => {
                let _ = encoder.finish();
                let _ = std::fs::remove_file("C:\\test_readonly.mp4");
            }
            Err(e) => {
                println!("Expected error on read-only location: {}", e);
            }
        }
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_very_large_dimensions() {
    let output_path = test_output_path("test_large_dims.webm");

    let result = VideoEncoder::create(&output_path)
        .video(16384, 16384, 30.0)
        .video_codec(VideoCodec::Vp9)
        .build();

    match result {
        Ok(mut encoder) => {
            let frame_result = create_black_frame(16384, 16384);
            let push_result = encoder.push_video(&frame_result);
            match push_result {
                Ok(_) => {
                    println!("Surprisingly, 16K encoding works!");
                    let _ = encoder.finish();
                }
                Err(e) => {
                    println!("Frame push failed as expected: {}", e);
                }
            }
        }
        Err(e) => {
            println!(
                "Encoder creation failed as expected for large dimensions: {}",
                e
            );
        }
    }
}

#[test]
fn test_very_high_framerate() {
    let output_path = test_output_path("test_high_fps.webm");

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 1000.0)
        .video_codec(VideoCodec::Vp9)
        .build();

    match result {
        Ok(mut encoder) => {
            println!("1000fps encoder created successfully");
            for _ in 0..10 {
                let frame = create_black_frame(640, 480);
                encoder.push_video(&frame).expect("Failed to push frame");
            }
            encoder.finish().expect("Failed to finish encoding");
        }
        Err(e) => {
            println!("High framerate rejected: {}", e);
        }
    }
}

// ============================================================================
// Dimension Validation Tests (issue #283)
// ============================================================================

#[test]
fn width_zero_should_return_invalid_dimensions() {
    let output_path = test_output_path("dim_w0.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(0, 480, 30.0)
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidDimensions { .. })),
        "expected InvalidDimensions for width=0"
    );
}

#[test]
fn width_one_should_return_invalid_dimensions() {
    let output_path = test_output_path("dim_w1.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(1, 480, 30.0)
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidDimensions { .. })),
        "expected InvalidDimensions for width=1"
    );
}

#[test]
fn width_above_maximum_should_return_invalid_dimensions() {
    let output_path = test_output_path("dim_w_max.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(32769, 480, 30.0)
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidDimensions { .. })),
        "expected InvalidDimensions for width=32769"
    );
}

#[test]
fn height_one_should_return_invalid_dimensions() {
    let output_path = test_output_path("dim_h1.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 1, 30.0)
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidDimensions { .. })),
        "expected InvalidDimensions for height=1"
    );
}

#[test]
fn height_above_maximum_should_return_invalid_dimensions() {
    let output_path = test_output_path("dim_h_max.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 32769, 30.0)
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidDimensions { .. })),
        "expected InvalidDimensions for height=32769"
    );
}

#[test]
fn minimum_valid_dimensions_should_build_without_dimension_error() {
    let output_path = test_output_path("dim_min_valid.mp4");
    let result = VideoEncoder::create(&output_path).video(2, 2, 30.0).build();
    // Build may fail for other reasons (codec, file system), but not InvalidDimensions.
    assert!(
        !matches!(result, Err(EncodeError::InvalidDimensions { .. })),
        "expected no InvalidDimensions for width=2 height=2"
    );
}

// ============================================================================
// Bitrate Validation Tests (issue #283)
// ============================================================================

#[test]
fn cbr_bitrate_above_800mbps_should_return_invalid_bitrate() {
    let output_path = test_output_path("bitrate_cbr_max.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .bitrate_mode(BitrateMode::Cbr(900_000_000))
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidBitrate { .. })),
        "expected InvalidBitrate for cbr=900_000_000"
    );
}

#[test]
fn vbr_max_above_800mbps_should_return_invalid_bitrate() {
    let output_path = test_output_path("bitrate_vbr_max.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .bitrate_mode(BitrateMode::Vbr {
            target: 400_000_000,
            max: 900_000_000,
        })
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidBitrate { .. })),
        "expected InvalidBitrate for vbr max=900_000_000"
    );
}

#[test]
fn cbr_bitrate_at_800mbps_boundary_should_not_return_invalid_bitrate() {
    let output_path = test_output_path("bitrate_cbr_boundary.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .bitrate_mode(BitrateMode::Cbr(800_000_000))
        .build();
    assert!(
        !matches!(result, Err(EncodeError::InvalidBitrate { .. })),
        "expected no InvalidBitrate for cbr=800_000_000"
    );
}

// ============================================================================
// FPS Upper Bound Tests (issue #283)
// ============================================================================

#[test]
fn fps_above_1000_should_return_invalid_config() {
    let output_path = test_output_path("fps_max.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 1001.0)
        .build();
    assert!(
        matches!(result, Err(EncodeError::InvalidConfig { .. })),
        "expected InvalidConfig for fps=1001.0"
    );
}

#[test]
fn fps_at_1000_boundary_should_not_return_fps_error() {
    let output_path = test_output_path("fps_boundary.mp4");
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 1000.0)
        .build();
    // Build may fail for other reasons (codec, file system), but not the fps cap.
    let is_fps_error = matches!(&result, Err(EncodeError::InvalidConfig { reason })
        if reason.contains("fps") && reason.contains("maximum"));
    assert!(!is_fps_error, "expected no fps cap error for fps=1000.0");
}
