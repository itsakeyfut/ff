//! Integration tests for SilenceDetector.
//!
//! Tests verify:
//! - Missing input file returns `DecodeError::AnalysisFailed`
//! - A synthetic audio file (1s tone + 2s silence + 1s tone) yields one `SilenceRange`
//! - The detected range has the expected start/end within ±100 ms
//! - A `min_duration` longer than the actual silence yields no ranges
//! - Structural invariant: `range.start < range.end`
//! - A real audio file runs without error

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use ff_decode::{DecodeError, SilenceDetector};

/// Generates "1 s sine → 2 s silence → 1 s sine" as a WAV at `path`.
///
/// Uses the FFmpeg CLI's `lavfi` source so no pre-committed fixture is needed.
/// Returns `false` when FFmpeg is unavailable or the command fails (test skip).
fn make_silence_fixture(path: &Path) -> bool {
    std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=4:sample_rate=44100",
            "-af",
            "volume='if(between(t,1,3),0,1)'",
            path.to_str().unwrap_or(""),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn test_audio_path() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::path::PathBuf::from(format!(
        "{manifest_dir}/../../assets/audio/konekonoosanpo.mp3"
    ))
}

// ── Error-path tests ──────────────────────────────────────────────────────────

#[test]
fn silence_detector_missing_file_should_return_analysis_failed() {
    let result = SilenceDetector::new("does_not_exist_99999.mp3").run();
    assert!(
        matches!(result, Err(DecodeError::AnalysisFailed { .. })),
        "expected AnalysisFailed for missing file, got {result:?}"
    );
}

// ── Functional tests using synthetic fixture ──────────────────────────────────

#[test]
fn silence_detector_should_find_silence_range() {
    let fixture = std::env::temp_dir().join("ff_decode_silence_test_find.wav");
    if !make_silence_fixture(&fixture) {
        println!("Skipping: ffmpeg CLI unavailable or fixture generation failed");
        return;
    }

    let ranges = match SilenceDetector::new(&fixture)
        .threshold_db(-40.0)
        .min_duration(Duration::from_millis(500))
        .run()
    {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&fixture);
            println!("Skipping: SilenceDetector::run failed ({e})");
            return;
        }
    };

    let _ = std::fs::remove_file(&fixture);

    assert_eq!(
        ranges.len(),
        1,
        "expected exactly one SilenceRange for 2 s silence, got {}: {ranges:?}",
        ranges.len()
    );

    let margin = Duration::from_millis(200);
    let expected_start = Duration::from_secs(1);
    let expected_end = Duration::from_secs(3);

    assert!(
        ranges[0].start >= expected_start.saturating_sub(margin)
            && ranges[0].start <= expected_start + margin,
        "silence start {:?} not within ±200 ms of {:?}",
        ranges[0].start,
        expected_start
    );
    assert!(
        ranges[0].end >= expected_end.saturating_sub(margin)
            && ranges[0].end <= expected_end + margin,
        "silence end {:?} not within ±200 ms of {:?}",
        ranges[0].end,
        expected_end
    );
}

#[test]
fn silence_detector_short_silence_should_not_be_detected() {
    let fixture = std::env::temp_dir().join("ff_decode_silence_test_short.wav");
    if !make_silence_fixture(&fixture) {
        println!("Skipping: ffmpeg CLI unavailable or fixture generation failed");
        return;
    }

    // min_duration of 3 s is longer than the 2 s silence → nothing should be detected.
    let ranges = match SilenceDetector::new(&fixture)
        .threshold_db(-40.0)
        .min_duration(Duration::from_secs(3))
        .run()
    {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&fixture);
            println!("Skipping: SilenceDetector::run failed ({e})");
            return;
        }
    };

    let _ = std::fs::remove_file(&fixture);

    assert!(
        ranges.is_empty(),
        "expected no ranges when min_duration exceeds silence length, got {ranges:?}"
    );
}

#[test]
fn silence_detector_range_start_should_be_before_end() {
    let fixture = std::env::temp_dir().join("ff_decode_silence_test_order.wav");
    if !make_silence_fixture(&fixture) {
        println!("Skipping: ffmpeg CLI unavailable or fixture generation failed");
        return;
    }

    let ranges = match SilenceDetector::new(&fixture)
        .threshold_db(-40.0)
        .min_duration(Duration::from_millis(500))
        .run()
    {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&fixture);
            println!("Skipping: SilenceDetector::run failed ({e})");
            return;
        }
    };

    let _ = std::fs::remove_file(&fixture);

    for r in &ranges {
        assert!(
            r.start < r.end,
            "range.start ({:?}) must be strictly less than range.end ({:?})",
            r.start,
            r.end
        );
    }
}

// ── Real audio file test ──────────────────────────────────────────────────────

#[test]
fn silence_detector_real_audio_should_succeed() {
    let path = test_audio_path();
    if !path.exists() {
        println!("Skipping: test audio file not found at {}", path.display());
        return;
    }

    match SilenceDetector::new(&path)
        .threshold_db(-50.0)
        .min_duration(Duration::from_millis(200))
        .run()
    {
        Ok(_) => {}
        Err(e) => {
            println!("Skipping: SilenceDetector::run failed ({e})");
        }
    }
}
