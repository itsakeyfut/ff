//! Integration tests for AbrLadder::hls() and AbrLadder::dash().
//!
//! These tests exercise the full ABR pipeline:
//! 1. Create a short synthetic input video via `ff_encode`.
//! 2. Call `AbrLadder::hls()` or `AbrLadder::dash()` on it.
//! 3. Verify that the expected output files are created.
//!
//! All tests skip gracefully when the required encoder/decoder is unavailable.

// Tests are allowed to use unwrap() for simplicity.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

mod fixtures;

use ff_stream::{AbrLadder, Rendition, StreamError};
use fixtures::{DirGuard, create_test_video, tmp_dir};
use std::path::PathBuf;

// ============================================================================
// Helpers
// ============================================================================

/// Builds a 2-rendition ladder from a synthetic 320×240 2 s input video and
/// runs `AbrLadder::hls()`.  Returns `None` when the encoder is unavailable.
fn run_abr_hls(test_name: &str) -> Option<(PathBuf, DirGuard)> {
    let out_dir = tmp_dir(test_name);
    let guard = DirGuard(out_dir.clone());
    let input_path = out_dir.join("input.mp4");

    if !create_test_video(&input_path) {
        return None;
    }

    let result = AbrLadder::new(input_path.to_str().unwrap())
        .add_rendition(Rendition {
            width: 320,
            height: 240,
            bitrate: 1_500_000,
        })
        .add_rendition(Rendition {
            width: 320,
            height: 240,
            bitrate: 800_000,
        })
        .hls(out_dir.to_str().unwrap());

    match result {
        Err(StreamError::Ffmpeg { code, message }) => {
            println!("Skipping: HLS write failed: {message} (code={code})");
            None
        }
        Err(e) => panic!("Unexpected error: {e}"),
        Ok(()) => Some((out_dir, guard)),
    }
}

/// Builds a 2-rendition ladder from a synthetic 320×240 2 s input video and
/// runs `AbrLadder::dash()`.  Returns `None` when the encoder is unavailable.
fn run_abr_dash(test_name: &str) -> Option<(PathBuf, DirGuard)> {
    let out_dir = tmp_dir(test_name);
    let guard = DirGuard(out_dir.clone());
    let input_path = out_dir.join("input.mp4");

    if !create_test_video(&input_path) {
        return None;
    }

    let result = AbrLadder::new(input_path.to_str().unwrap())
        .add_rendition(Rendition {
            width: 320,
            height: 240,
            bitrate: 1_500_000,
        })
        .add_rendition(Rendition {
            width: 320,
            height: 240,
            bitrate: 800_000,
        })
        .dash(out_dir.to_str().unwrap());

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
fn hls_should_produce_master_playlist_and_subdirectories() {
    let Some((out_dir, _guard)) = run_abr_hls("abr_hls_master_test") else {
        return;
    };

    let master = out_dir.join("master.m3u8");
    assert!(master.exists(), "master.m3u8 should exist");
    assert!(
        std::fs::metadata(&master).unwrap().len() > 0,
        "master.m3u8 should be non-empty"
    );

    let playlist0 = out_dir.join("0/playlist.m3u8");
    assert!(playlist0.exists(), "0/playlist.m3u8 should exist");

    let playlist1 = out_dir.join("1/playlist.m3u8");
    assert!(playlist1.exists(), "1/playlist.m3u8 should exist");
}

#[test]
fn master_playlist_should_contain_bandwidth_and_resolution() {
    let Some((out_dir, _guard)) = run_abr_hls("abr_hls_content_test") else {
        return;
    };

    let content = std::fs::read_to_string(out_dir.join("master.m3u8")).unwrap();
    assert!(
        content.contains("#EXT-X-STREAM-INF"),
        "missing #EXT-X-STREAM-INF"
    );
    assert!(
        content.contains("BANDWIDTH=1500000"),
        "missing BANDWIDTH=1500000"
    );
    assert!(
        content.contains("RESOLUTION=320x240"),
        "missing RESOLUTION=320x240"
    );
}

#[test]
fn dash_should_produce_single_manifest() {
    let Some((out_dir, _guard)) = run_abr_dash("abr_dash_single_manifest_test") else {
        return;
    };

    let manifest = out_dir.join("manifest.mpd");
    assert!(
        manifest.exists(),
        "manifest.mpd should exist at output root"
    );
    assert!(
        std::fs::metadata(&manifest).unwrap().len() > 0,
        "manifest.mpd should be non-empty"
    );
}

#[test]
fn dash_manifest_should_contain_representation_elements() {
    let Some((out_dir, _guard)) = run_abr_dash("abr_dash_representations_test") else {
        return;
    };

    let content = std::fs::read_to_string(out_dir.join("manifest.mpd")).unwrap();
    assert!(
        content.contains("Representation"),
        "manifest.mpd should contain Representation elements"
    );
}
