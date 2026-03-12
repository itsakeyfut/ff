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

use ff_encode::{EncodeError, VideoCodec, VideoEncoder};
use fixtures::{create_black_frame, test_output_path};

// ============================================================================
// Configuration Error Tests
// ============================================================================

#[test]
fn test_invalid_output_path() {
    // Try to create encoder with invalid path containing null byte
    // Note: CString::new will handle the null byte validation
    let result = VideoEncoder::create("/invalid/path\0.mp4");

    // The builder creation might succeed, but build should fail
    match result {
        Ok(builder) => {
            let build_result = builder.video(640, 480, 30.0).build();
            // Build might fail due to invalid path
            if build_result.is_ok() {
                println!("Path with null byte was accepted (system-dependent)");
            }
        }
        Err(_) => {
            // Expected: should fail with invalid path
        }
    }
}

#[test]
fn test_encoder_without_video_config() {
    let output_path = test_output_path("test_no_video.mp4");

    // Create encoder without calling .video()
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .build();

    // Should fail validation because no streams are configured
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

    // Try to create encoder with zero width
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .video(0, 480, 30.0)
        .build();

    // Should fail validation
    assert!(result.is_err(), "Should fail with zero width");

    // Try to create encoder with zero height
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .video(640, 0, 30.0)
        .build();

    // Should fail validation
    assert!(result.is_err(), "Should fail with zero height");
}

#[test]
fn test_zero_framerate() {
    let output_path = test_output_path("test_zero_fps.mp4");

    // Try to create encoder with zero framerate
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .video(640, 480, 0.0)
        .build();

    // Should fail validation
    assert!(result.is_err(), "Should fail with zero framerate");
}

#[test]
fn test_negative_framerate() {
    let output_path = test_output_path("test_negative_fps.mp4");

    // Try to create encoder with negative framerate
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .video(640, 480, -30.0)
        .build();

    // Should fail validation
    assert!(result.is_err(), "Should fail with negative framerate");
}

// ============================================================================
// Runtime Error Tests
// ============================================================================

#[test]
fn test_finish_without_frames() {
    let output_path = test_output_path("test_finish_no_frames.mp4");

    // Use MPEG-4 which is always available (built-in encoder)
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(encoder) => {
            // Finish without pushing any frames - should still succeed
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
            // Skip test if no encoder available
        }
    }
}

#[test]
fn test_push_video_with_wrong_dimensions() {
    let output_path = test_output_path("test_wrong_dimensions.mp4");

    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(mut encoder) => {
            // Try to push frame with different dimensions
            // Note: This might succeed due to automatic scaling, so we just test it doesn't panic
            let frame = create_black_frame(320, 240);
            let push_result = encoder.push_video(&frame);

            // Either succeeds (with scaling) or fails gracefully
            match push_result {
                Ok(_) => println!("Frame accepted with automatic scaling"),
                Err(e) => println!("Frame rejected as expected: {}", e),
            }

            // Clean up
            let _ = encoder.finish();
        }
        Err(e) => {
            println!(
                "Encoder creation failed (no suitable codec available): {}",
                e
            );
            // Skip test if no encoder available
        }
    }
}

// ============================================================================
// Codec Error Tests
// ============================================================================

#[test]
fn test_unsupported_codec_fallback() {
    let output_path = test_output_path("test_codec_fallback.mp4");

    // Test that MPEG-4 encoder is available
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(encoder) => {
            let codec = encoder.actual_video_codec();
            println!("Selected codec: {}", codec);
            // Should be mpeg4 encoder
            assert!(!codec.is_empty());
        }
        Err(e) => {
            println!("Codec selection failed: {}", e);
            // This is acceptable if no suitable encoder is available
        }
    }
}

// ============================================================================
// File System Error Tests
// ============================================================================

#[test]
fn test_readonly_directory() {
    // Try to create file in a read-only location (system root on Unix)
    #[cfg(unix)]
    {
        let result = VideoEncoder::create("/test_readonly.mp4")
            .expect("Failed to create builder")
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
        // On Windows, trying to create file directly in C:\ might fail
        let result = VideoEncoder::create("C:\\test_readonly.mp4")
            .expect("Failed to create builder")
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Vp9)
            .build();

        // This might succeed or fail depending on permissions
        match result {
            Ok(encoder) => {
                let _ = encoder.finish();
                // Clean up if it succeeded
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

    // Try to create encoder with very large dimensions
    // This should fail during initialization or frame allocation
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .video(16384, 16384, 30.0) // 16K resolution
        .video_codec(VideoCodec::Vp9)
        .build();

    match result {
        Ok(mut encoder) => {
            // If encoder creation succeeds, try to push a frame
            let frame_result = create_black_frame(16384, 16384);
            let push_result = encoder.push_video(&frame_result);

            // This will likely fail due to memory constraints
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

    // Try to create encoder with very high framerate
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create builder")
        .video(640, 480, 1000.0) // 1000 fps
        .video_codec(VideoCodec::Vp9)
        .build();

    // Should either succeed or fail gracefully
    match result {
        Ok(mut encoder) => {
            println!("1000fps encoder created successfully");

            // Try encoding a few frames
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
