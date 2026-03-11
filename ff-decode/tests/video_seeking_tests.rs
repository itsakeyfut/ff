//! Seeking and iterator tests for VideoDecoder.
//!
//! Tests various seeking modes (Keyframe, Exact, Backward) and iterator patterns.

use std::time::Duration;

mod fixtures;
use fixtures::*;

use ff_decode::SeekMode;

// ============================================================================
// Seeking Tests
// ============================================================================

#[test]
fn test_seek_keyframe_mode() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode a few frames first to ensure decoder is initialized
    for _ in 0..5 {
        let _ = decoder.decode_one().expect("Failed to decode");
    }

    // Seek to 2 seconds using keyframe mode
    let target = Duration::from_secs(2);
    let result = decoder.seek(target, SeekMode::Keyframe);

    assert!(
        result.is_ok(),
        "Keyframe seek should succeed: {:?}",
        result.err()
    );

    // Decode a frame after seeking
    let frame = decoder
        .decode_one()
        .expect("Failed to decode after seek")
        .expect("Frame should exist after seek");

    // Frame timestamp should be somewhere in the video (not at the very beginning)
    // Keyframe seeking may not be perfectly accurate, but it should get us
    // somewhere close to the target
    let frame_time = frame.timestamp().as_duration();

    assert!(
        frame_time >= Duration::from_secs(1),
        "Frame after keyframe seek should be past 1s: frame_time={:?}, target={:?}",
        frame_time,
        target
    );

    assert!(
        frame_time <= Duration::from_secs(4),
        "Frame after keyframe seek should be before 4s: frame_time={:?}, target={:?}",
        frame_time,
        target
    );
}

#[test]
fn test_seek_exact_mode() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Seek to 3 seconds using exact mode
    let target = Duration::from_secs(3);
    let result = decoder.seek(target, SeekMode::Exact);

    assert!(
        result.is_ok(),
        "Exact seek should succeed: {:?}",
        result.err()
    );

    // Decode a frame after seeking
    let frame = decoder
        .decode_one()
        .expect("Failed to decode after seek")
        .expect("Frame should exist after seek");

    // Frame timestamp should be at or after target (exact seek)
    let frame_time = frame.timestamp().as_duration();

    assert!(
        frame_time >= target,
        "Frame timestamp should be at or after target for exact seek: target={:?}, frame_time={:?}",
        target,
        frame_time
    );

    // Should not be too far from target
    let diff = frame_time - target;
    assert!(
        diff < Duration::from_millis(500),
        "Frame timestamp should be close to target: diff={:?}",
        diff
    );
}

#[test]
fn test_seek_backward_mode() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Seek to 2 seconds using backward mode
    let target = Duration::from_secs(2);
    let result = decoder.seek(target, SeekMode::Backward);

    assert!(
        result.is_ok(),
        "Backward seek should succeed: {:?}",
        result.err()
    );

    // Decode a frame after seeking
    let frame = decoder
        .decode_one()
        .expect("Failed to decode after seek")
        .expect("Frame should exist after seek");

    // Frame timestamp should be at or before target (backward seek)
    let frame_time = frame.timestamp().as_duration();

    assert!(
        frame_time <= target || (frame_time - target) < Duration::from_millis(100),
        "Frame timestamp should be at or slightly after target for backward seek: target={:?}, frame_time={:?}",
        target,
        frame_time
    );
}

#[test]
fn test_seek_to_beginning() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode a few frames first
    for _ in 0..10 {
        let _ = decoder.decode_one().expect("Failed to decode");
    }

    // Seek back to beginning
    let result = decoder.seek(Duration::ZERO, SeekMode::Keyframe);

    assert!(
        result.is_ok(),
        "Seek to beginning should succeed: {:?}",
        result.err()
    );

    // Decode first frame
    let frame = decoder
        .decode_one()
        .expect("Failed to decode after seek to beginning")
        .expect("First frame should exist");

    // Frame should be near the beginning
    let frame_time = frame.timestamp().as_duration();
    assert!(
        frame_time < Duration::from_secs(1),
        "Frame after seek to beginning should be near start: frame_time={:?}",
        frame_time
    );
}

#[test]
fn test_seek_multiple_times() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Perform multiple seeks
    let positions = [
        Duration::from_secs(5),
        Duration::from_secs(2),
        Duration::from_secs(8),
        Duration::from_secs(1),
    ];

    for (i, &pos) in positions.iter().enumerate() {
        let result = decoder.seek(pos, SeekMode::Keyframe);
        assert!(
            result.is_ok(),
            "Seek #{} to {:?} should succeed: {:?}",
            i,
            pos,
            result.err()
        );

        // Decode a frame after each seek
        let frame = decoder
            .decode_one()
            .unwrap_or_else(|e| panic!("Failed to decode after seek #{}: {:?}", i, e))
            .unwrap_or_else(|| panic!("Frame should exist after seek #{}", i));

        let frame_time = frame.timestamp().as_duration();
        assert!(
            frame_time >= Duration::ZERO,
            "Frame time should be valid after seek #{}",
            i
        );
    }
}

#[test]
fn test_seek_and_decode_multiple_frames() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Seek to 4 seconds
    decoder
        .seek(Duration::from_secs(4), SeekMode::Keyframe)
        .expect("Seek should succeed");

    // Decode 5 frames after seeking
    let mut frame_count = 0;
    for i in 0..5 {
        let frame = decoder
            .decode_one()
            .unwrap_or_else(|e| panic!("Failed to decode frame {} after seek: {:?}", i, e))
            .unwrap_or_else(|| panic!("Frame {} should exist after seek", i));

        frame_count += 1;

        // Verify frame is valid
        assert!(frame.width() > 0, "Frame {} should be valid", i);
        assert!(frame.height() > 0, "Frame {} should be valid", i);
    }

    assert_eq!(frame_count, 5, "Should decode 5 frames after seeking");
}

#[test]
fn test_seek_beyond_duration() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Try to seek far beyond video duration
    let far_position = Duration::from_secs(10000);
    let result = decoder.seek(far_position, SeekMode::Keyframe);

    // Seek might succeed but decoding should return None (EOF)
    // or seek might fail depending on the demuxer
    if result.is_ok() {
        let frame = decoder.decode_one().expect("decode_one should not error");
        // Should either get None (EOF) or a frame near the end
        if let Some(f) = frame {
            let duration = decoder.duration();
            let frame_time = f.timestamp().as_duration();
            assert!(
                frame_time <= duration,
                "Frame time should not exceed duration"
            );
        }
    }
    // If seek fails, that's also acceptable behavior
}

#[test]
fn test_position_updates_after_seek() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode a few frames first
    for _ in 0..5 {
        let _ = decoder.decode_one().expect("Failed to decode");
    }

    // Initial position should be small (we've only decoded 5 frames)
    let initial_pos = decoder.position();
    assert!(
        initial_pos < Duration::from_secs(1),
        "Initial position should be less than 1s after 5 frames"
    );

    // Seek to 2 seconds
    let target = Duration::from_secs(2);
    decoder
        .seek(target, SeekMode::Keyframe)
        .expect("Seek should succeed");

    // Decode a frame to update position
    let frame = decoder
        .decode_one()
        .expect("Failed to decode after seek")
        .expect("Frame should exist after seek");

    // Position should now be updated based on the decoded frame
    let pos_after_seek = decoder.position();
    let frame_time = frame.timestamp().as_duration();

    assert!(
        pos_after_seek >= Duration::from_secs(1),
        "Position after seek and decode should be close to target: pos={:?}, frame_time={:?}, target={:?}",
        pos_after_seek,
        frame_time,
        target
    );
}

// ============================================================================
// Iterator Pattern Tests
// ============================================================================

#[test]
fn test_frame_iterator_basic() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Use iterator to decode first 10 frames
    let frames: Vec<_> = decoder.frames().take(10).collect();

    assert_eq!(frames.len(), 10, "Should collect 10 frames");

    // All frames should be Ok
    for (i, frame_result) in frames.iter().enumerate() {
        assert!(
            frame_result.is_ok(),
            "Frame {} should be Ok: {:?}",
            i,
            frame_result
        );
    }
}

#[test]
fn test_frame_iterator_timestamps_increase() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let mut last_pts = None;

    // Iterate over first 20 frames
    for (i, frame_result) in decoder.frames().take(20).enumerate() {
        let frame =
            frame_result.unwrap_or_else(|e| panic!("Failed to decode frame {}: {:?}", i, e));

        let pts = frame.timestamp().pts();

        if let Some(last) = last_pts {
            assert!(
                pts > last,
                "Frame {} pts should increase: current={}, last={}",
                i,
                pts,
                last
            );
        }

        last_pts = Some(pts);
    }
}

#[test]
fn test_frame_iterator_with_filter() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Use iterator to find frames at or after 2 seconds
    let target = Duration::from_secs(2);

    let late_frames: Vec<_> = decoder
        .frames()
        .filter_map(|r| r.ok())
        .filter(|f| f.timestamp().as_duration() >= target)
        .take(5)
        .collect();

    assert_eq!(late_frames.len(), 5, "Should collect 5 frames after 2s");

    // All frames should be at or after target
    for frame in late_frames {
        assert!(
            frame.timestamp().as_duration() >= target,
            "Frame should be at or after target"
        );
    }
}

#[test]
fn test_frame_iterator_early_break() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Break early in iteration
    let mut count = 0;
    for frame_result in decoder.frames() {
        let _ = frame_result.expect("Frame should decode successfully");
        count += 1;
        if count >= 3 {
            break;
        }
    }

    assert_eq!(count, 3, "Should decode exactly 3 frames before breaking");

    // Should be able to continue decoding after early break
    let next_frame = decoder
        .decode_one()
        .expect("decode_one should work after iterator break")
        .expect("Next frame should exist");

    assert!(next_frame.width() > 0, "Next frame should be valid");
}

#[test]
fn test_frame_iterator_multiple_iterations() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // First iteration
    let first_batch: Vec<_> = decoder.frames().take(5).collect();
    assert_eq!(first_batch.len(), 5, "First batch should have 5 frames");

    // Seek back to beginning
    decoder
        .seek(Duration::ZERO, SeekMode::Keyframe)
        .expect("Seek should succeed");

    // Second iteration
    let second_batch: Vec<_> = decoder.frames().take(5).collect();
    assert_eq!(second_batch.len(), 5, "Second batch should have 5 frames");
}
