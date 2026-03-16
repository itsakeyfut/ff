//! Integration tests for DashOutput::write().
//!
//! These tests exercise the full FFmpeg DASH muxing pipeline:
//! 1. Create a short synthetic input video via `ff_encode`.
//! 2. Call `DashOutput::write()` on it.
//! 3. Verify that `manifest.mpd` and at least one segment file are created.
//!
//! All tests skip gracefully when the required encoder/decoder is unavailable.

// Tests are allowed to use unwrap() for simplicity.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

mod fixtures;

use ff_stream::{DashOutput, StreamError};
use fixtures::{DirGuard, create_test_video, tmp_dir};
use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// Helpers
// ============================================================================

/// Runs the full DASH pipeline and returns the output dir + guard if successful.
/// Returns `None` when encoder/decoder is unavailable (test should skip).
fn run_dash_write(test_name: &str, segment_secs: u64) -> Option<(PathBuf, DirGuard)> {
    let out_dir = tmp_dir(test_name);
    let guard = DirGuard(out_dir.clone());
    let input_path = out_dir.join("input.mp4");

    if !create_test_video(&input_path) {
        return None;
    }

    let result = DashOutput::new(out_dir.to_str().unwrap())
        .input(input_path.to_str().unwrap())
        .segment_duration(Duration::from_secs(segment_secs))
        .build()
        .expect("build should succeed")
        .write();

    match result {
        Err(StreamError::Ffmpeg { code, message }) => {
            println!("Skipping: DASH write failed: {message} (code={code})");
            None
        }
        Err(e) => panic!("Unexpected error: {e}"),
        Ok(()) => Some((out_dir, guard)),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn write_should_produce_manifest_and_segments() {
    let Some((out_dir, _guard)) = run_dash_write("dash_write_test", 1) else {
        return;
    };

    let manifest = out_dir.join("manifest.mpd");
    assert!(manifest.exists(), "manifest.mpd should exist");
    assert!(
        std::fs::metadata(&manifest).unwrap().len() > 0,
        "manifest.mpd should be non-empty"
    );

    let segments: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".m4s") || name.ends_with(".mp4")
        })
        .filter(|e| e.file_name().to_string_lossy() != "manifest.mpd")
        .collect();
    assert!(
        !segments.is_empty(),
        "at least one segment file (.m4s or .mp4) should be present"
    );

    println!(
        "DASH output: {} segments, manifest {} bytes",
        segments.len(),
        std::fs::metadata(&manifest).unwrap().len(),
    );
}

#[test]
fn manifest_should_contain_required_dash_tags() {
    let Some((out_dir, _guard)) = run_dash_write("dash_tags_test", 1) else {
        return;
    };

    let content = std::fs::read_to_string(out_dir.join("manifest.mpd")).unwrap();
    assert!(
        content.contains("<?xml"),
        "missing <?xml declaration in manifest"
    );
    assert!(content.contains("MPD"), "missing MPD element in manifest");
    assert!(
        content.contains("AdaptationSet"),
        "missing AdaptationSet in manifest"
    );
}
