//! decode_range() tests for VideoDecoder.
//!
//! Tests the decode_range() method which decodes all frames within a time range.

use std::time::Duration;

mod fixtures;
use fixtures::*;

use ff_decode::{HardwareAccel, VideoDecoder};
use ff_format::PixelFormat;

// ============================================================================
// decode_range() Tests
// ============================================================================

#[test]
fn test_decode_range_basic() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let start = Duration::from_secs(1);
    let end = Duration::from_secs(3);

    let frames = decoder
        .decode_range(start, end)
        .expect("decode_range should succeed");

    assert!(!frames.is_empty(), "Should decode at least some frames");

    // All frames should be within the range
    for (i, frame) in frames.iter().enumerate() {
        let frame_time = frame.timestamp().as_duration();
        assert!(
            frame_time >= start && frame_time < end,
            "Frame {} timestamp {:?} should be in range [{:?}, {:?})",
            i,
            frame_time,
            start,
            end
        );
    }
}

#[test]
fn test_decode_range_timestamps_ordered() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let start = Duration::from_secs(2);
    let end = Duration::from_secs(5);

    let frames = decoder
        .decode_range(start, end)
        .expect("decode_range should succeed");

    assert!(!frames.is_empty(), "Should decode at least some frames");

    // Timestamps should be in ascending order
    let mut last_time = None;
    for (i, frame) in frames.iter().enumerate() {
        let frame_time = frame.timestamp().as_duration();

        if let Some(last) = last_time {
            assert!(
                frame_time > last,
                "Frame {} timestamp should increase: current={:?}, last={:?}",
                i,
                frame_time,
                last
            );
        }

        last_time = Some(frame_time);
    }
}

#[test]
fn test_decode_range_short_duration() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode a very short range (0.5 seconds)
    let start = Duration::from_millis(1000);
    let end = Duration::from_millis(1500);

    let frames = decoder
        .decode_range(start, end)
        .expect("decode_range should succeed");

    // Should get some frames even for short range
    // At 30fps, 0.5s = ~15 frames
    assert!(!frames.is_empty(), "Should decode frames in short range");

    // All frames should be within the range
    for frame in &frames {
        let frame_time = frame.timestamp().as_duration();
        assert!(
            frame_time >= start && frame_time < end,
            "Frame timestamp {:?} should be in range [{:?}, {:?})",
            frame_time,
            start,
            end
        );
    }
}

#[test]
fn test_decode_range_invalid_range() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // start >= end should fail
    let start = Duration::from_secs(5);
    let end = Duration::from_secs(2);

    let result = decoder.decode_range(start, end);
    assert!(result.is_err(), "Invalid range should return error");

    if let Err(ff_decode::DecodeError::DecodingFailed { reason, .. }) = result {
        assert!(
            reason.contains("Invalid time range"),
            "Error message should mention invalid range"
        );
    } else {
        panic!("Expected DecodingFailed error");
    }
}

#[test]
fn test_decode_range_equal_start_end() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // start == end should fail
    let time = Duration::from_secs(3);

    let result = decoder.decode_range(time, time);
    assert!(result.is_err(), "Equal start and end should return error");
}

#[test]
fn test_decode_range_from_beginning() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let start = Duration::ZERO;
    let end = Duration::from_secs(2);

    let frames = decoder
        .decode_range(start, end)
        .expect("decode_range from beginning should succeed");

    assert!(!frames.is_empty(), "Should decode frames from beginning");

    // First frame should be near the start
    let first_time = frames[0].timestamp().as_duration();
    assert!(
        first_time < Duration::from_millis(500),
        "First frame should be near beginning: {:?}",
        first_time
    );
}

#[test]
fn test_decode_range_multiple_calls() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode multiple ranges in sequence
    let ranges = [
        (Duration::from_secs(1), Duration::from_secs(2)),
        (Duration::from_secs(3), Duration::from_secs(4)),
        (Duration::from_secs(5), Duration::from_secs(6)),
    ];

    for (i, (start, end)) in ranges.iter().enumerate() {
        let frames = decoder
            .decode_range(*start, *end)
            .unwrap_or_else(|e| panic!("Range {} should succeed: {:?}", i, e));

        assert!(!frames.is_empty(), "Range {} should have frames", i);

        // Verify all frames are in range
        for frame in frames {
            let frame_time = frame.timestamp().as_duration();
            assert!(
                frame_time >= *start && frame_time < *end,
                "Frame in range {} should be within bounds",
                i
            );
        }
    }
}

#[test]
fn test_decode_range_position_updates() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let start = Duration::from_secs(2);
    let end = Duration::from_secs(4);

    let _ = decoder
        .decode_range(start, end)
        .expect("decode_range should succeed");

    // After decode_range, position should be at or past end
    let position = decoder.position();
    assert!(
        position >= end || (position >= start && position < end + Duration::from_secs(1)),
        "Position after decode_range should be at or near end: position={:?}, end={:?}",
        position,
        end
    );
}

#[test]
fn test_decode_range_with_rgba_output() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Rgba)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let start = Duration::from_secs(1);
    let end = Duration::from_secs(2);

    let frames = decoder
        .decode_range(start, end)
        .expect("decode_range should succeed");

    assert!(!frames.is_empty(), "Should decode frames");

    // All frames should be RGBA
    for frame in frames {
        assert_eq!(
            frame.format(),
            PixelFormat::Rgba,
            "All frames should be RGBA"
        );
    }
}
