//! Thumbnail generation tests for VideoDecoder.
//!
//! Tests thumbnail_at() and thumbnails() methods for generating preview images.

use std::time::Duration;

mod fixtures;
use fixtures::*;

use ff_decode::{HardwareAccel, SeekMode, VideoDecoder};
use ff_format::PixelFormat;

// ============================================================================
// Thumbnail Generation Tests
// ============================================================================

#[test]
fn test_thumbnail_at_basic() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Generate a thumbnail at 2 seconds
    let position = Duration::from_secs(2);
    let width = 320;
    let height = 180;

    let thumbnail = decoder
        .thumbnail_at(position, width, height)
        .expect("thumbnail_at should succeed")
        .expect("frame should be available");

    // Verify dimensions
    assert_eq!(
        thumbnail.width(),
        width,
        "Thumbnail width should match target"
    );
    assert_eq!(
        thumbnail.height(),
        height,
        "Thumbnail height should match target"
    );

    // Verify it's a valid frame
    assert!(!thumbnail.planes().is_empty(), "Thumbnail should have data");
}

#[test]
fn test_thumbnail_at_timestamp() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let position = Duration::from_secs(3);
    let thumbnail = decoder
        .thumbnail_at(position, 320, 180)
        .expect("thumbnail_at should succeed")
        .expect("frame should be available");

    // Timestamp should be close to requested position
    // (may not be exact due to keyframe seeking)
    let thumb_time = thumbnail.timestamp().as_duration();
    let tolerance = Duration::from_secs(2); // GOP size tolerance

    assert!(
        thumb_time >= position.saturating_sub(tolerance) && thumb_time <= position + tolerance,
        "Thumbnail timestamp {:?} should be near requested position {:?}",
        thumb_time,
        position
    );
}

#[test]
fn test_thumbnail_at_beginning() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let thumbnail = decoder
        .thumbnail_at(Duration::ZERO, 160, 90)
        .expect("thumbnail_at at beginning should succeed")
        .expect("frame should be available");

    assert_eq!(thumbnail.width(), 160, "Width should be 160");
    assert_eq!(thumbnail.height(), 90, "Height should be 90");

    // Should be near the beginning
    let thumb_time = thumbnail.timestamp().as_duration();
    assert!(
        thumb_time < Duration::from_secs(1),
        "Thumbnail should be near beginning: {:?}",
        thumb_time
    );
}

#[test]
fn test_thumbnail_at_different_sizes() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let sizes = [(320, 180), (640, 360), (160, 90), (480, 270)];

    for (width, height) in sizes {
        // Seek back to beginning for each test
        decoder
            .seek(Duration::ZERO, SeekMode::Keyframe)
            .expect("Seek should succeed");

        let thumbnail = decoder
            .thumbnail_at(Duration::from_secs(1), width, height)
            .unwrap_or_else(|e| {
                panic!("thumbnail_at({}x{}) should succeed: {:?}", width, height, e)
            })
            .expect("frame should be available");

        assert_eq!(
            thumbnail.width(),
            width,
            "Thumbnail width should be {}",
            width
        );
        assert_eq!(
            thumbnail.height(),
            height,
            "Thumbnail height should be {}",
            height
        );
    }
}

#[test]
fn test_thumbnail_at_with_rgba_output() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Rgba)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let thumbnail = decoder
        .thumbnail_at(Duration::from_secs(1), 320, 180)
        .expect("thumbnail_at should succeed")
        .expect("frame should be available");

    // Should be RGBA format
    assert_eq!(
        thumbnail.format(),
        PixelFormat::Rgba,
        "Thumbnail should be RGBA"
    );

    // Should have correct dimensions
    assert_eq!(thumbnail.width(), 320, "Width should be 320");
    assert_eq!(thumbnail.height(), 180, "Height should be 180");
}

#[test]
fn test_thumbnails_basic() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let count = 5;
    let width = 160;
    let height = 90;

    let thumbnails = decoder
        .thumbnails(count, width, height)
        .expect("thumbnails should succeed");

    // Should generate requested number of thumbnails
    assert_eq!(
        thumbnails.len(),
        count,
        "Should generate {} thumbnails",
        count
    );

    // All thumbnails should have correct dimensions
    for (i, thumbnail) in thumbnails.iter().enumerate() {
        assert_eq!(
            thumbnail.width(),
            width,
            "Thumbnail {} width should be {}",
            i,
            width
        );
        assert_eq!(
            thumbnail.height(),
            height,
            "Thumbnail {} height should be {}",
            i,
            height
        );
    }
}

#[test]
fn test_thumbnails_timestamps_ordered() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let thumbnails = decoder
        .thumbnails(10, 160, 90)
        .expect("thumbnails should succeed");

    assert_eq!(thumbnails.len(), 10, "Should have 10 thumbnails");

    // Timestamps should be in ascending order
    let mut last_time = None;
    for (i, thumbnail) in thumbnails.iter().enumerate() {
        let thumb_time = thumbnail.timestamp().as_duration();

        if let Some(last) = last_time {
            assert!(
                thumb_time >= last,
                "Thumbnail {} timestamp should be >= previous: current={:?}, last={:?}",
                i,
                thumb_time,
                last
            );
        }

        last_time = Some(thumb_time);
    }
}

#[test]
fn test_thumbnails_evenly_distributed() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let count = 5;
    let thumbnails = decoder
        .thumbnails(count, 160, 90)
        .expect("thumbnails should succeed");

    assert_eq!(thumbnails.len(), count, "Should have {} thumbnails", count);

    let duration = decoder.duration();

    // Calculate expected interval
    let expected_interval = duration.as_secs_f64() / count as f64;

    // Verify approximate spacing (with tolerance for keyframe seeking)
    for i in 0..count - 1 {
        let current_time = thumbnails[i].timestamp().as_duration().as_secs_f64();
        let next_time = thumbnails[i + 1].timestamp().as_duration().as_secs_f64();

        let actual_interval = next_time - current_time;

        // Allow significant tolerance due to keyframe seeking
        // Thumbnails may not be exactly evenly spaced due to GOP structure
        assert!(
            actual_interval >= expected_interval * 0.3,
            "Interval between thumbnails {} and {} ({:.2}s) should be at least 30% of expected ({:.2}s)",
            i,
            i + 1,
            actual_interval,
            expected_interval
        );
    }
}

#[test]
fn test_thumbnails_single_thumbnail() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let thumbnails = decoder
        .thumbnails(1, 320, 180)
        .expect("Single thumbnail should succeed");

    assert_eq!(thumbnails.len(), 1, "Should have exactly 1 thumbnail");
    assert_eq!(thumbnails[0].width(), 320, "Width should be 320");
    assert_eq!(thumbnails[0].height(), 180, "Height should be 180");
}

#[test]
fn test_thumbnails_many_thumbnails() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let count = 20;
    let thumbnails = decoder
        .thumbnails(count, 160, 90)
        .expect("Many thumbnails should succeed");

    assert_eq!(
        thumbnails.len(),
        count,
        "Should generate {} thumbnails",
        count
    );

    // All should have valid dimensions
    for thumbnail in thumbnails {
        assert_eq!(thumbnail.width(), 160, "Width should be 160");
        assert_eq!(thumbnail.height(), 90, "Height should be 90");
    }
}

#[test]
fn test_thumbnails_zero_count_fails() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let result = decoder.thumbnails(0, 160, 90);

    assert!(result.is_err(), "Zero count should fail");

    if let Err(ff_decode::DecodeError::DecodingFailed { reason, .. }) = result {
        assert!(
            reason.contains("Thumbnail count must be greater than zero"),
            "Error message should mention zero count"
        );
    } else {
        panic!("Expected DecodingFailed error");
    }
}

#[test]
fn test_thumbnails_with_rgba_output() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Rgba)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let thumbnails = decoder
        .thumbnails(5, 160, 90)
        .expect("thumbnails should succeed");

    assert_eq!(thumbnails.len(), 5, "Should have 5 thumbnails");

    // All should be RGBA
    for (i, thumbnail) in thumbnails.iter().enumerate() {
        assert_eq!(
            thumbnail.format(),
            PixelFormat::Rgba,
            "Thumbnail {} should be RGBA",
            i
        );
    }
}

#[test]
fn test_thumbnail_aspect_ratio_preservation() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Get original video dimensions
    let original_width = decoder.width();
    let original_height = decoder.height();
    let original_aspect = original_width as f64 / original_height as f64;

    // Generate thumbnail with different aspect ratio target
    let target_width = 320;
    let target_height = 320; // Square - different from video aspect

    let thumbnail = decoder
        .thumbnail_at(Duration::from_secs(1), target_width, target_height)
        .expect("thumbnail_at should succeed")
        .expect("frame should be available");

    // Thumbnail should preserve aspect ratio by fitting within target dimensions
    // The thumbnail will not be exactly 320x320 but will fit within it
    assert!(
        thumbnail.width() <= target_width,
        "Thumbnail width should be <= target width"
    );
    assert!(
        thumbnail.height() <= target_height,
        "Thumbnail height should be <= target height"
    );

    // Verify aspect ratio is preserved
    let thumbnail_aspect = thumbnail.width() as f64 / thumbnail.height() as f64;
    let aspect_diff = (thumbnail_aspect - original_aspect).abs();
    assert!(
        aspect_diff < 0.01,
        "Aspect ratio should be preserved: original={:.3}, thumbnail={:.3}",
        original_aspect,
        thumbnail_aspect
    );

    // At least one dimension should match target (fit-within strategy)
    assert!(
        thumbnail.width() == target_width || thumbnail.height() == target_height,
        "At least one dimension should match target"
    );

    assert!(
        !thumbnail.planes().is_empty(),
        "Thumbnail should have valid data"
    );
}

#[test]
fn test_thumbnail_small_dimensions() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Very small thumbnail
    let thumbnail = decoder
        .thumbnail_at(Duration::from_secs(1), 64, 36)
        .expect("Small thumbnail should succeed")
        .expect("frame should be available");

    assert_eq!(thumbnail.width(), 64, "Width should be 64");
    assert_eq!(thumbnail.height(), 36, "Height should be 36");
}

#[test]
fn test_thumbnail_large_dimensions() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Large thumbnail (upscaling)
    let thumbnail = decoder
        .thumbnail_at(Duration::from_secs(1), 1920, 1080)
        .expect("Large thumbnail should succeed")
        .expect("frame should be available");

    assert_eq!(thumbnail.width(), 1920, "Width should be 1920");
    assert_eq!(thumbnail.height(), 1080, "Height should be 1080");
}

#[test]
fn test_thumbnails_after_decode() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode some frames first
    let _frames: Vec<_> = decoder.by_ref().take(10).collect();

    // Then generate thumbnails
    let thumbnails = decoder
        .thumbnails(5, 160, 90)
        .expect("thumbnails after decode should succeed");

    assert_eq!(thumbnails.len(), 5, "Should still generate 5 thumbnails");
}

#[test]
fn test_thumbnail_at_after_seek() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Seek to middle of video
    decoder
        .seek(Duration::from_secs(5), SeekMode::Keyframe)
        .expect("Seek should succeed");

    // Generate thumbnail at a different position
    let thumbnail = decoder
        .thumbnail_at(Duration::from_secs(2), 320, 180)
        .expect("thumbnail_at after seek should succeed")
        .expect("frame should be available");

    assert_eq!(thumbnail.width(), 320, "Width should be 320");
    assert_eq!(thumbnail.height(), 180, "Height should be 180");
}

#[test]
fn test_multiple_thumbnail_calls() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Generate multiple thumbnails at different positions
    let positions = [
        Duration::from_secs(1),
        Duration::from_secs(3),
        Duration::from_secs(5),
    ];

    for (i, position) in positions.iter().enumerate() {
        let thumbnail = decoder
            .thumbnail_at(*position, 160, 90)
            .unwrap_or_else(|e| panic!("Thumbnail {} should succeed: {:?}", i, e))
            .expect("frame should be available");

        assert_eq!(thumbnail.width(), 160, "Thumbnail {} width", i);
        assert_eq!(thumbnail.height(), 90, "Thumbnail {} height", i);
    }
}
