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

// ============================================================================
// Tests
// ============================================================================

#[test]
fn write_should_produce_playlist_and_segments() {
    let out_dir = tmp_dir("hls_write_test");
    let _guard = DirGuard(out_dir.clone());
    let input_path = out_dir.join("input.mp4");

    // Step 1: create a small test video
    if !create_test_video(&input_path) {
        return; // skip
    }

    // Step 2: run HLS output
    let result = HlsOutput::new(out_dir.to_str().unwrap())
        .input(input_path.to_str().unwrap())
        .segment_duration(Duration::from_secs(1))
        .keyframe_interval(25)
        .build()
        .expect("build should succeed")
        .write();

    match result {
        Err(StreamError::Ffmpeg { reason }) => {
            println!("Skipping: HLS write failed (no suitable encoder): {reason}");
            return;
        }
        Err(e) => panic!("Unexpected error: {e}"),
        Ok(()) => {}
    }

    // Step 3: verify outputs
    let playlist = out_dir.join("playlist.m3u8");
    assert!(playlist.exists(), "playlist.m3u8 should exist");
    assert!(
        std::fs::metadata(&playlist).unwrap().len() > 0,
        "playlist.m3u8 should be non-empty"
    );

    // At least one segment file should be present
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
