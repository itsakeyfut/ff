//! Test fixtures and helpers for ff-stream integration tests.

#![allow(dead_code)]

use std::path::PathBuf;

// ============================================================================
// Output Path Helpers
// ============================================================================

/// Returns the path to the shared test output directory.
pub fn test_output_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{manifest_dir}/target/test-output"))
}

/// Creates a unique temporary subdirectory under `target/test-output/`.
pub fn tmp_dir(name: &str) -> PathBuf {
    let dir = test_output_dir().join(name);
    std::fs::create_dir_all(&dir).ok();
    dir
}

// ============================================================================
// Cleanup Helpers
// ============================================================================

/// Guard that removes a directory tree when dropped.
///
/// After removing the named subdirectory, it also attempts to remove the
/// parent `target/test-output` directory; the removal is a no-op when other
/// tests have left their own subdirectories behind.
pub struct DirGuard(pub PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        // Walk up and remove each ancestor only if it is now empty.
        let mut dir = self.0.as_path();
        while let Some(parent) = dir.parent() {
            if std::fs::remove_dir(parent).is_err() {
                break;
            }
            dir = parent;
        }
    }
}

// ============================================================================
// Video Fixture Helper
// ============================================================================

/// Create a minimal synthetic video file at `path` using ff_encode.
///
/// Returns `false` and prints a skip message if the encoder is unavailable.
pub fn create_test_video(path: &PathBuf) -> bool {
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
        let frame = match VideoFrame::new(
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
        ) {
            Ok(f) => f,
            Err(_) => {
                println!("Skipping test: frame creation failed");
                return false;
            }
        };
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
