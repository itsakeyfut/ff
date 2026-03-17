//! Test fixtures and helpers for ff-pipeline integration tests.

#![allow(dead_code)]

use std::path::PathBuf;

/// Returns the path to the shared test assets directory.
pub fn assets_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{}/../../assets", manifest_dir))
}

/// Returns the path to the test video file (contains both video and audio).
pub fn test_video_path() -> PathBuf {
    assets_dir().join("video/gameplay.mp4")
}

/// Returns the path to the test audio file.
pub fn test_audio_path() -> PathBuf {
    assets_dir().join("audio/konekonoosanpo.mp3")
}

/// Returns the directory used for test output files.
pub fn test_output_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{}/target/test-output", manifest_dir))
}

/// Creates a path inside the test output directory.
///
/// The directory is created automatically if it does not exist.
pub fn test_output_path(filename: &str) -> PathBuf {
    let dir = test_output_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join(filename)
}

/// RAII guard that deletes a file when dropped.
///
/// Ensures test output files are cleaned up even when tests panic.
pub struct FileGuard {
    path: PathBuf,
}

impl FileGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
        // Remove empty ancestor directories (test-output/, then target/).
        // remove_dir is a no-op when the directory is not empty, so this is
        // safe when multiple tests run in parallel.
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
            if let Some(grandparent) = parent.parent() {
                let _ = std::fs::remove_dir(grandparent);
            }
        }
    }
}
