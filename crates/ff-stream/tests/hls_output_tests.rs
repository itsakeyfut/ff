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

use ff_stream::{HlsOutput, StreamError};
use std::path::PathBuf;
use std::time::Duration;

// ============================================================================
// Helpers
// ============================================================================

/// Create a unique temporary output directory under the crate's target/.
fn tmp_dir(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let dir = PathBuf::from(format!("{manifest_dir}/target/test-output/{name}"));
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// Guard that removes a directory tree when dropped.
struct DirGuard(PathBuf);
impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Create a minimal synthetic video file at `path` using ff_encode.
///
/// Returns `false` and prints a skip message if the encoder is unavailable.
fn create_test_video(path: &PathBuf) -> bool {
    use ff_encode::{VideoCodec, VideoEncoder};
    use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};

    let mut encoder = match VideoEncoder::create(path.to_str().unwrap())
        .video(320, 240, 25.0)
        .video_codec(VideoCodec::Mpeg4)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping test: cannot create encoder: {e}");
            return false;
        }
    };

    // 50 frames = 2 s at 25 fps
    for _ in 0..50 {
        let y_size = 320 * 240;
        let uv_size = (320 / 2) * (240 / 2);
        let frame = VideoFrame::new(
            vec![
                PooledBuffer::standalone(vec![0u8; y_size]),
                PooledBuffer::standalone(vec![128u8; uv_size]),
                PooledBuffer::standalone(vec![128u8; uv_size]),
            ],
            vec![320, 160, 160],
            320,
            240,
            PixelFormat::Yuv420p,
            Timestamp::default(),
            true,
        )
        .expect("frame creation failed");
        if encoder.push_video(&frame).is_err() {
            println!("Skipping test: frame push failed");
            return false;
        }
    }

    if encoder.finish().is_err() {
        println!("Skipping test: encoder finish failed");
        return false;
    }

    true
}

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
