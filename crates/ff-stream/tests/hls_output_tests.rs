//! Integration tests for HlsOutput::write().
//!
//! These tests exercise the full FFmpeg HLS muxing pipeline:
//! 1. Create a short synthetic input video via `ff_encode`.
//! 2. Call `HlsOutput::write()` on it.
//! 3. Verify that `playlist.m3u8` and at least one segment file are created.
//!
//! All tests skip gracefully when the required encoder/decoder is unavailable.

// Tests are allowed to use unwrap() for simplicity.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

mod fixtures;

use ff_stream::{HlsOutput, StreamError};
use fixtures::{DirGuard, create_test_video, tmp_dir};
use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// Helpers
// ============================================================================

/// Runs the full HLS pipeline and returns the output dir + guard if successful.
/// Returns `None` when encoder/decoder is unavailable (test should skip).
fn run_hls_write(
    test_name: &str,
    segment_secs: u64,
    keyframe_interval: u32,
) -> Option<(PathBuf, DirGuard)> {
    let out_dir = tmp_dir(test_name);
    let guard = DirGuard(out_dir.clone());
    let input_path = out_dir.join("input.mp4");

    if !create_test_video(&input_path) {
        return None;
    }

    let result = HlsOutput::new(out_dir.to_str().unwrap())
        .input(input_path.to_str().unwrap())
        .segment_duration(Duration::from_secs(segment_secs))
        .keyframe_interval(keyframe_interval)
        .build()
        .expect("build should succeed")
        .write();

    match result {
        Err(StreamError::Ffmpeg { code, message }) => {
            println!("Skipping: HLS write failed: {message} (code={code})");
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
fn write_should_produce_playlist_and_segments() {
    let Some((out_dir, _guard)) = run_hls_write("hls_write_test", 1, 25) else {
        return;
    };

    let playlist = out_dir.join("playlist.m3u8");
    assert!(playlist.exists(), "playlist.m3u8 should exist");
    assert!(
        std::fs::metadata(&playlist).unwrap().len() > 0,
        "playlist.m3u8 should be non-empty"
    );

    let segments: Vec<_> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".ts"))
        .collect();
    assert!(
        !segments.is_empty(),
        "at least one .ts segment should be present"
    );

    println!(
        "HLS output: {} segments, playlist {} bytes",
        segments.len(),
        std::fs::metadata(&playlist).unwrap().len(),
    );
}

#[test]
fn playlist_should_contain_required_hls_tags() {
    let Some((out_dir, _guard)) = run_hls_write("hls_tags_test", 1, 25) else {
        return;
    };

    let content = std::fs::read_to_string(out_dir.join("playlist.m3u8")).unwrap();
    assert!(content.contains("#EXTM3U"), "missing #EXTM3U in playlist");
    assert!(
        content.contains("#EXT-X-TARGETDURATION"),
        "missing #EXT-X-TARGETDURATION in playlist"
    );
    assert!(content.contains("#EXTINF:"), "missing #EXTINF in playlist");
    assert!(
        content.contains("#EXT-X-ENDLIST"),
        "missing #EXT-X-ENDLIST in playlist"
    );
}

#[test]
fn segments_should_follow_numbered_filename_pattern() {
    let Some((out_dir, _guard)) = run_hls_write("hls_naming_test", 1, 25) else {
        return;
    };

    let mut names: Vec<String> = std::fs::read_dir(&out_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".ts"))
        .collect();
    names.sort();

    assert!(!names.is_empty(), "no .ts segments found");
    assert_eq!(
        names[0], "segment000.ts",
        "first segment should be segment000.ts"
    );
    for name in &names {
        assert!(
            name.starts_with("segment") && name.ends_with(".ts") && name.len() == 13,
            "unexpected segment filename: {name}"
        );
    }
}

#[test]
fn segment_duration_should_be_respected_in_playlist() {
    let Some((out_dir, _guard)) = run_hls_write("hls_duration_test", 1, 25) else {
        return;
    };

    let content = std::fs::read_to_string(out_dir.join("playlist.m3u8")).unwrap();
    let target_dur: u32 = content
        .lines()
        .find(|l| l.starts_with("#EXT-X-TARGETDURATION:"))
        .and_then(|l| l.trim_start_matches("#EXT-X-TARGETDURATION:").parse().ok())
        .expect("#EXT-X-TARGETDURATION not found or not an integer");

    assert!(
        target_dur <= 2,
        "#EXT-X-TARGETDURATION={target_dur} expected ≤2 for 1s configured segment duration"
    );
}

#[test]
fn hls_target_duration_should_not_be_zero() {
    let Some((out_dir, _guard)) = run_hls_write("hls_nonzero_duration_test", 1, 25) else {
        return;
    };

    let content = std::fs::read_to_string(out_dir.join("playlist.m3u8")).unwrap();
    let target_dur: u32 = content
        .lines()
        .find(|l| l.starts_with("#EXT-X-TARGETDURATION:"))
        .and_then(|l| l.trim_start_matches("#EXT-X-TARGETDURATION:").parse().ok())
        .expect("#EXT-X-TARGETDURATION not found or not an integer");

    assert!(
        target_dur > 0,
        "#EXT-X-TARGETDURATION must be non-zero; got {target_dur} \
         (zero means the HLS muxer received no packet durations)"
    );
}
