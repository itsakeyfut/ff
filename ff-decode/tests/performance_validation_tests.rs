//! Performance validation tests for ff-decode.
//!
//! These tests validate that the ff-* implementation meets the performance targets:
//! - Seek: 5-10ms (vs 50-100ms for legacy ffmpeg-next)
//! - Frame decode: 5-10ms
//! - Multiple operations should maintain consistent performance
//!
//! Note: These tests use relaxed thresholds to account for CI environment variability.

// Tests are allowed to use unwrap() for simplicity
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ff_decode::{HardwareAccel, SeekMode, VideoDecoder};

// ============================================================================
// Test Helpers
// ============================================================================

/// Returns the path to the test assets directory.
fn assets_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{}/../assets", manifest_dir))
}

/// Returns the path to the test video file.
fn test_video_path() -> PathBuf {
    assets_dir().join("videos/noma-brain-power.mp4")
}

/// Creates a test decoder with hardware acceleration disabled for consistency.
fn create_decoder() -> VideoDecoder {
    VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder")
}

/// Measures the execution time of a function and returns (result, duration).
fn measure<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let duration = start.elapsed();
    (result, duration)
}

// ============================================================================
// Seek Performance Tests
// ============================================================================

#[test]
fn test_seek_performance_target() {
    let mut decoder = create_decoder();

    // Decode a few frames first to initialize
    for _ in 0..5 {
        let _ = decoder.decode_one();
    }

    // Target: 5-10ms for seek + decode
    // We use 50ms as a relaxed threshold for CI environments (5x the target)
    const THRESHOLD_MS: u128 = 50;

    let (result, duration) = measure(|| {
        decoder
            .seek(Duration::from_secs(2), SeekMode::Keyframe)
            .expect("Seek failed");
        decoder.decode_one().expect("Decode after seek failed")
    });

    assert!(result.is_some(), "Should decode frame after seek");

    let elapsed_ms = duration.as_millis();
    println!("Seek + decode took: {}ms", elapsed_ms);

    assert!(
        elapsed_ms < THRESHOLD_MS,
        "Seek performance target not met: {}ms (threshold: {}ms)",
        elapsed_ms,
        THRESHOLD_MS
    );
}

#[test]
fn test_repeated_seek_performance() {
    let mut decoder = create_decoder();

    // Target: Repeated seeks should be consistently fast
    // Average should be under 25ms in CI environments
    const AVG_THRESHOLD_MS: u128 = 25;
    const ITERATIONS: usize = 10;

    let positions = [
        Duration::from_secs(1),
        Duration::from_secs(3),
        Duration::from_secs(5),
        Duration::from_secs(2),
        Duration::from_secs(4),
    ];

    let (_, duration) = measure(|| {
        for &pos in positions.iter().cycle().take(ITERATIONS) {
            decoder.seek(pos, SeekMode::Keyframe).expect("Seek failed");
            let _ = decoder.decode_one().expect("Decode failed");
        }
    });

    let avg_ms = duration.as_millis() / ITERATIONS as u128;
    println!(
        "Average seek + decode time over {} iterations: {}ms",
        ITERATIONS, avg_ms
    );

    assert!(
        avg_ms < AVG_THRESHOLD_MS,
        "Average seek performance target not met: {}ms (threshold: {}ms)",
        avg_ms,
        AVG_THRESHOLD_MS
    );
}

// ============================================================================
// Decode Performance Tests
// ============================================================================

#[test]
fn test_decode_performance_target() {
    let mut decoder = create_decoder();

    // Decode first frame (may include initialization overhead)
    let _ = decoder.decode_one();

    // Target: 5-10ms per frame for 1080p H.264
    // We use 30ms as a relaxed threshold for CI environments
    const THRESHOLD_MS: u128 = 30;

    let (result, duration) = measure(|| decoder.decode_one().expect("Decode failed"));

    assert!(result.is_some(), "Should decode frame");

    let elapsed_ms = duration.as_millis();
    println!("Frame decode took: {}ms", elapsed_ms);

    assert!(
        elapsed_ms < THRESHOLD_MS,
        "Decode performance target not met: {}ms (threshold: {}ms)",
        elapsed_ms,
        THRESHOLD_MS
    );
}

#[test]
fn test_sequential_decode_performance() {
    let mut decoder = create_decoder();

    // Decode first frame (initialization)
    let _ = decoder.decode_one();

    // Target: Consistent decode performance over multiple frames
    const FRAME_COUNT: usize = 30;
    const AVG_THRESHOLD_MS: u128 = 15; // Average should be under 15ms

    let (_, duration) = measure(|| {
        for _ in 0..FRAME_COUNT {
            let frame = decoder.decode_one().expect("Decode failed");
            if frame.is_none() {
                break;
            }
        }
    });

    let avg_ms = duration.as_millis() / FRAME_COUNT as u128;
    println!(
        "Average decode time over {} frames: {}ms",
        FRAME_COUNT, avg_ms
    );

    assert!(
        avg_ms < AVG_THRESHOLD_MS,
        "Average decode performance target not met: {}ms (threshold: {}ms)",
        avg_ms,
        AVG_THRESHOLD_MS
    );
}

// ============================================================================
// Combined Workflow Performance Tests
// ============================================================================

#[test]
fn test_scrubbing_workflow_performance() {
    // Simulates video scrubbing: seek to position, decode a few frames, repeat
    let mut decoder = create_decoder();

    const SCRUB_POSITIONS: usize = 5;
    const FRAMES_PER_POSITION: usize = 3;
    const TOTAL_THRESHOLD_MS: u128 = 200; // Total for all operations

    let positions = [
        Duration::from_secs(1),
        Duration::from_secs(3),
        Duration::from_secs(5),
        Duration::from_secs(7),
        Duration::from_secs(2),
    ];

    let (_, duration) = measure(|| {
        for &pos in &positions {
            decoder.seek(pos, SeekMode::Keyframe).expect("Seek failed");

            for _ in 0..FRAMES_PER_POSITION {
                let frame = decoder.decode_one().expect("Decode failed");
                if frame.is_none() {
                    break;
                }
            }
        }
    });

    let elapsed_ms = duration.as_millis();
    let avg_per_scrub = elapsed_ms / SCRUB_POSITIONS as u128;

    println!("Scrubbing workflow took: {}ms total", elapsed_ms);
    println!("Average per scrub position: {}ms", avg_per_scrub);

    assert!(
        elapsed_ms < TOTAL_THRESHOLD_MS,
        "Scrubbing workflow performance target not met: {}ms (threshold: {}ms)",
        elapsed_ms,
        TOTAL_THRESHOLD_MS
    );
}

#[test]
fn test_thumbnail_generation_performance() {
    let mut decoder = create_decoder();

    // Target: Thumbnail generation should be fast (under 100ms for single thumbnail)
    const THRESHOLD_MS: u128 = 100;

    let (result, duration) = measure(|| {
        decoder
            .thumbnail_at(Duration::from_secs(2), 320, 180)
            .expect("Thumbnail generation failed")
    });

    assert!(!result.planes().is_empty(), "Should generate thumbnail");

    let elapsed_ms = duration.as_millis();
    println!("Thumbnail generation took: {}ms", elapsed_ms);

    assert!(
        elapsed_ms < THRESHOLD_MS,
        "Thumbnail generation performance target not met: {}ms (threshold: {}ms)",
        elapsed_ms,
        THRESHOLD_MS
    );
}

#[test]
fn test_batch_thumbnail_performance() {
    let mut decoder = create_decoder();

    // Target: Batch thumbnail generation should be efficient
    const THUMBNAIL_COUNT: usize = 10;
    const AVG_THRESHOLD_MS: u128 = 50; // Average per thumbnail

    let (result, duration) = measure(|| {
        decoder
            .thumbnails(THUMBNAIL_COUNT, 160, 90)
            .expect("Thumbnails generation failed")
    });

    assert_eq!(
        result.len(),
        THUMBNAIL_COUNT,
        "Should generate all thumbnails"
    );

    let elapsed_ms = duration.as_millis();
    let avg_ms = elapsed_ms / THUMBNAIL_COUNT as u128;

    println!(
        "Batch thumbnail generation took: {}ms total ({} thumbnails)",
        elapsed_ms, THUMBNAIL_COUNT
    );
    println!("Average per thumbnail: {}ms", avg_ms);

    assert!(
        avg_ms < AVG_THRESHOLD_MS,
        "Batch thumbnail performance target not met: {}ms avg (threshold: {}ms)",
        avg_ms,
        AVG_THRESHOLD_MS
    );
}

// ============================================================================
// Performance Consistency Tests
// ============================================================================

#[test]
fn test_performance_consistency() {
    // Verify that performance is consistent across multiple operations
    let mut decoder = create_decoder();

    const ITERATIONS: usize = 20;
    let mut durations = Vec::with_capacity(ITERATIONS);

    // Initialize decoder
    let _ = decoder.decode_one();

    // Measure decode time for multiple frames
    for _ in 0..ITERATIONS {
        let (_, duration) = measure(|| decoder.decode_one().expect("Decode failed"));
        durations.push(duration.as_micros());
    }

    // Calculate statistics
    let avg = durations.iter().sum::<u128>() / durations.len() as u128;
    let max = *durations.iter().max().unwrap();
    let min = *durations.iter().min().unwrap();

    println!("Decode performance statistics (microseconds):");
    println!("  Average: {}", avg);
    println!("  Min: {}", min);
    println!("  Max: {}", max);
    println!("  Range: {}", max - min);

    // Performance should not vary too much
    // Max should not be more than 5x the average
    assert!(
        max < avg * 5,
        "Performance variance too high: max={}µs, avg={}µs",
        max,
        avg
    );
}
