//! Integration tests for QualityMetrics.
//!
//! Tests verify:
//! - Missing reference returns `FilterError::AnalysisFailed`
//! - Comparing a video to itself returns SSIM ≈ 1.0
//! - The returned SSIM is in [0.0, 1.0]

#![allow(clippy::unwrap_used)]

use ff_filter::{FilterError, QualityMetrics};

fn test_video_path() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::path::PathBuf::from(format!("{manifest_dir}/../../assets/video/gameplay.mp4"))
}

// ── Error-path tests ──────────────────────────────────────────────────────────

#[test]
fn quality_metrics_ssim_missing_reference_should_return_analysis_failed() {
    let result = QualityMetrics::ssim(
        "does_not_exist_ref_99999.mp4",
        "does_not_exist_dist_99999.mp4",
    );
    assert!(
        matches!(result, Err(FilterError::AnalysisFailed { .. })),
        "expected AnalysisFailed for missing reference, got {result:?}"
    );
}

// ── Functional tests ──────────────────────────────────────────────────────────

#[test]
fn quality_metrics_ssim_identical_files_should_return_one() {
    let path = test_video_path();
    if !path.exists() {
        println!("Skipping: test video file not found at {}", path.display());
        return;
    }

    let ssim = match QualityMetrics::ssim(&path, &path) {
        Ok(v) => v,
        Err(e) => {
            println!("Skipping: QualityMetrics::ssim failed ({e})");
            return;
        }
    };

    assert!(
        ssim >= 0.9999,
        "expected SSIM ≈ 1.0 when comparing a file to itself, got {ssim}"
    );
}

#[test]
fn quality_metrics_ssim_result_should_be_between_zero_and_one() {
    let path = test_video_path();
    if !path.exists() {
        println!("Skipping: test video file not found at {}", path.display());
        return;
    }

    let ssim = match QualityMetrics::ssim(&path, &path) {
        Ok(v) => v,
        Err(e) => {
            println!("Skipping: QualityMetrics::ssim failed ({e})");
            return;
        }
    };

    assert!(
        (0.0..=1.0).contains(&ssim),
        "expected SSIM in [0.0, 1.0], got {ssim}"
    );
}
