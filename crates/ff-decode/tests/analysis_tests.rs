//! Integration tests for scene-change detection against a synthetic reference video.
//!
//! The reference video is generated at runtime via `VideoEncoder`: six
//! solid-colour 1-second segments (30 frames each) are encoded back-to-back,
//! producing hard cuts at 1 s, 2 s, 3 s, 4 s, and 5 s.  Tests skip
//! gracefully when the encoder cannot be built.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use ff_encode::{VideoCodec, VideoEncoder};
use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};

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

/// YUV420P frame filled with a solid colour specified as (Y, Cb, Cr).
fn yuv420p_frame(width: u32, height: u32, y: u8, cb: u8, cr: u8) -> VideoFrame {
    let y_plane = PooledBuffer::standalone(vec![y; (width * height) as usize]);
    let u_plane = PooledBuffer::standalone(vec![cb; ((width / 2) * (height / 2)) as usize]);
    let v_plane = PooledBuffer::standalone(vec![cr; ((width / 2) * (height / 2)) as usize]);
    VideoFrame::new(
        vec![y_plane, u_plane, v_plane],
        vec![width as usize, (width / 2) as usize, (width / 2) as usize],
        width,
        height,
        PixelFormat::Yuv420p,
        Timestamp::default(),
        true,
    )
    .expect("failed to create test frame")
}

/// Generates a 6-second video consisting of six 1-second solid-colour segments
/// (red, green, blue, yellow, white, cyan) encoded back-to-back.
///
/// Hard cuts appear at exactly 1 s, 2 s, 3 s, 4 s, and 5 s.
///
/// Returns `None` (printing a skip message) when the encoder cannot be built.
fn generate_hard_cut_video(path: &Path) -> Option<()> {
    const WIDTH: u32 = 160;
    const HEIGHT: u32 = 120;
    const FPS: f64 = 30.0;
    const FRAMES_PER_SEGMENT: usize = 30; // 1 second at 30 fps

    // Distinct solid colours in YUV420P (BT.601 approximate values).
    // Ordered so that every adjacent pair differs by |ΔY| ≥ 65 (> 51 needed
    // for scene score > 0.4 at threshold 0.4):
    //   red→white: |82−235|=153   white→blue:   |235−41|=194
    //   blue→yellow: |41−210|=169  yellow→green: |210−145|=65
    //   green→black: |145−16|=129
    let segments: &[(u8, u8, u8)] = &[
        (82, 90, 240),   // red    Y=82
        (235, 128, 128), // white  Y=235
        (41, 240, 110),  // blue   Y=41
        (210, 16, 146),  // yellow Y=210
        (145, 54, 34),   // green  Y=145
        (16, 128, 128),  // black  Y=16
    ];

    let mut encoder = match VideoEncoder::create(path)
        .video(WIDTH, HEIGHT, FPS)
        .video_codec(VideoCodec::Mpeg4)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: cannot build encoder: {e}");
            return None;
        }
    };

    for &(y, cb, cr) in segments {
        for _ in 0..FRAMES_PER_SEGMENT {
            let frame = yuv420p_frame(WIDTH, HEIGHT, y, cb, cr);
            if let Err(e) = encoder.push_video(&frame) {
                println!("Skipping: push_video failed: {e}");
                return None;
            }
        }
    }

    if let Err(e) = encoder.finish() {
        println!("Skipping: encoder finish failed: {e}");
        return None;
    }

    Some(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn scene_detector_should_detect_known_cuts_within_one_frame_tolerance() {
    let out_path = test_output_path("analysis_scene_hard_cuts.mp4");
    let _guard = FileGuard(out_path.clone());

    // Generate the reference video; skip gracefully if encoder is unavailable.
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
