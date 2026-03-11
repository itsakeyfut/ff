//! Performance validation tests for ff-probe.
//!
//! These tests validate that metadata extraction meets the performance target:
//! - Metadata extraction: 20-30ms (same as legacy ffmpeg-next)
//!
//! Note: These tests use relaxed thresholds to account for CI environment variability.

// Tests are allowed to use unwrap() for simplicity
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ff_probe::open;

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

/// Returns the path to the test audio file.
fn test_audio_path() -> PathBuf {
    assets_dir().join("audio/noma-brain-power.mp3")
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
// Metadata Extraction Performance Tests
// ============================================================================

#[test]
fn test_probe_video_performance_target() {
    let path = test_video_path();

    // Target: 20-30ms for metadata extraction
    // We use 100ms as a relaxed threshold for CI environments
    const THRESHOLD_MS: u128 = 100;

    let (result, duration) = measure(|| open(&path).expect("Failed to probe video"));

    assert!(result.has_video(), "Should detect video stream");

    let elapsed_ms = duration.as_millis();
    println!("Video probe took: {}ms", elapsed_ms);

    assert!(
        elapsed_ms < THRESHOLD_MS,
        "Probe performance target not met: {}ms (threshold: {}ms)",
        elapsed_ms,
        THRESHOLD_MS
    );
}

#[test]
fn test_probe_audio_performance_target() {
    let path = test_audio_path();

    // Target: 20-30ms for metadata extraction
    const THRESHOLD_MS: u128 = 100;

    let (result, duration) = measure(|| open(&path).expect("Failed to probe audio"));

    assert!(result.has_audio(), "Should detect audio stream");

    let elapsed_ms = duration.as_millis();
    println!("Audio probe took: {}ms", elapsed_ms);

    assert!(
        elapsed_ms < THRESHOLD_MS,
        "Probe performance target not met: {}ms (threshold: {}ms)",
        elapsed_ms,
        THRESHOLD_MS
    );
}

#[test]
fn test_repeated_probe_performance() {
    let path = test_video_path();

    // Target: Repeated probing should be consistently fast
    // Average should be under 50ms in CI environments
    const AVG_THRESHOLD_MS: u128 = 50;
    const ITERATIONS: usize = 10;

    let (_, duration) = measure(|| {
        for _ in 0..ITERATIONS {
            let _ = open(&path).expect("Failed to probe");
        }
    });

    let avg_ms = duration.as_millis() / ITERATIONS as u128;
    println!(
        "Average probe time over {} iterations: {}ms",
        ITERATIONS, avg_ms
    );

    assert!(
        avg_ms < AVG_THRESHOLD_MS,
        "Average probe performance target not met: {}ms (threshold: {}ms)",
        avg_ms,
        AVG_THRESHOLD_MS
    );
}

#[test]
fn test_metadata_access_performance() {
    let path = test_video_path();

    // Pre-load the media info
    let info = open(&path).expect("Failed to probe video");

    // Accessing metadata should be instant (under 1ms)
    const THRESHOLD_MICROS: u128 = 1000; // 1ms

    let (_, duration) = measure(|| {
        // Access various metadata fields
        let _ = info.has_video();
        let _ = info.video_stream_count();
        let _ = info.primary_video();
        let _ = info.resolution();
        let _ = info.frame_rate();
        let _ = info.has_audio();
        let _ = info.audio_stream_count();
        let _ = info.primary_audio();
        let _ = info.duration();
        let _ = info.file_size();
        let _ = info.format();
    });

    let elapsed_micros = duration.as_micros();
    println!("Metadata access took: {}µs", elapsed_micros);

    assert!(
        elapsed_micros < THRESHOLD_MICROS,
        "Metadata access too slow: {}µs (threshold: {}µs)",
        elapsed_micros,
        THRESHOLD_MICROS
    );
}

#[test]
fn test_probe_performance_consistency() {
    let path = test_video_path();

    const ITERATIONS: usize = 20;
    let mut durations = Vec::with_capacity(ITERATIONS);

    // Measure probe time for multiple iterations
    for _ in 0..ITERATIONS {
        let (_, duration) = measure(|| open(&path).expect("Failed to probe"));
        durations.push(duration.as_micros());
    }

    // Calculate statistics
    let avg = durations.iter().sum::<u128>() / durations.len() as u128;
    let max = *durations.iter().max().unwrap();
    let min = *durations.iter().min().unwrap();

    println!("Probe performance statistics (microseconds):");
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

#[test]
fn test_probe_cold_vs_warm() {
    let path = test_video_path();

    // First probe (cold - may include file system cache miss)
    let (_, cold_duration) = measure(|| open(&path).expect("Failed to probe"));

    // Second probe (warm - file should be in cache)
    let (_, warm_duration) = measure(|| open(&path).expect("Failed to probe"));

    let cold_ms = cold_duration.as_millis();
    let warm_ms = warm_duration.as_millis();

    println!("Cold probe: {}ms", cold_ms);
    println!("Warm probe: {}ms", warm_ms);

    // Warm probe should not be slower than cold probe
    // (It might be same or faster due to caching)
    assert!(
        warm_ms <= cold_ms * 2,
        "Warm probe unexpectedly slower: warm={}ms, cold={}ms",
        warm_ms,
        cold_ms
    );
}
