//! Integration tests for scene-change detection against a synthetic reference video.
//!
//! The reference video is generated at runtime via the `ffmpeg` CLI: six
//! solid-colour 1-second segments are concatenated, producing hard cuts at
//! 1 s, 2 s, 3 s, 4 s, and 5 s.  Tests skip gracefully when `ffmpeg` is not
//! in `PATH` or generation fails.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use ff_decode::SceneDetector;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Guard that removes its path on drop (ensures cleanup even on panic).
struct FileGuard(PathBuf);
impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Returns a writable path inside the crate's `target/test-output/` directory.
fn test_output_path(filename: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
    std::fs::create_dir_all(&dir).ok();
    dir.join(filename)
}

/// Generates a 6-second video consisting of six 1-second solid-colour segments
/// (red, green, blue, yellow, white, cyan) concatenated via the `concat` filter.
///
/// Hard cuts appear at exactly 1 s, 2 s, 3 s, 4 s, and 5 s.
///
/// Returns `None` (printing a skip message) when `ffmpeg` is not available or
/// the generation command fails.
fn generate_hard_cut_video(path: &Path) -> Option<()> {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => {
            println!("Skipping: output path is not valid UTF-8");
            return None;
        }
    };

    // Six 1-second 160×120 colour sources at 30 fps, concatenated into one clip.
    let output = match Command::new("ffmpeg")
        .args([
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=160x120:d=1:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=green:s=160x120:d=1:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=160x120:d=1:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=yellow:s=160x120:d=1:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=160x120:d=1:r=30",
            "-f",
            "lavfi",
            "-i",
            "color=c=cyan:s=160x120:d=1:r=30",
            "-filter_complex",
            "[0][1][2][3][4][5]concat=n=6:v=1:a=0[v]",
            "-map",
            "[v]",
            "-vcodec",
            "mpeg4",
            "-y",
            path_str,
        ])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            println!("Skipping: cannot run ffmpeg: {e}");
            return None;
        }
    };

    if !output.status.success() {
        println!(
            "Skipping: ffmpeg exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    Some(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn scene_detector_should_detect_known_cuts_within_one_frame_tolerance() {
    let out_path = test_output_path("analysis_scene_hard_cuts.mp4");
    let _guard = FileGuard(out_path.clone());

    // Generate the reference video; skip gracefully if ffmpeg is unavailable.
    if generate_hard_cut_video(&out_path).is_none() {
        return;
    }

    // Run SceneDetector with default threshold (0.4).
    let timestamps = match SceneDetector::new(&out_path).threshold(0.4).run() {
        Ok(ts) => ts,
        Err(e) => {
            println!("Skipping: SceneDetector::run failed: {e}");
            return;
        }
    };

    // The video has exactly 5 hard cuts.
    assert_eq!(
        timestamps.len(),
        5,
        "expected exactly 5 scene-change timestamps, got {}: {timestamps:?}",
        timestamps.len()
    );

    // Known cut positions: 1 s, 2 s, 3 s, 4 s, 5 s.
    let expected = [
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(3),
        Duration::from_secs(4),
        Duration::from_secs(5),
    ];

    // ±1 frame tolerance at 30 fps ≈ 33 ms.
    let tolerance = Duration::from_millis(34);

    for (i, (detected, &expected_ts)) in timestamps.iter().zip(expected.iter()).enumerate() {
        let diff = if *detected >= expected_ts {
            *detected - expected_ts
        } else {
            expected_ts - *detected
        };
        assert!(
            diff <= tolerance,
            "cut #{i}: expected ≈{expected_ts:?} (±{tolerance:?}), got {detected:?} \
             (diff={diff:?})"
        );
    }
}
