//! Test fixtures and helpers for ff-decode integration tests.
//!
//! This module provides common utilities for testing video and audio decoding:
//! - Asset path helpers (test files, videos, audio)
//! - Decoder creation helpers with default settings
//! - Assertions and validation helpers

#![allow(dead_code)]

use std::path::PathBuf;

use ff_decode::{AudioDecoder, HardwareAccel, VideoDecoder};

// ============================================================================
// Asset Path Helpers
// ============================================================================

/// Returns the path to the test assets directory.
pub fn assets_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{}/../../assets", manifest_dir))
}

/// Returns the path to the test video file.
pub fn test_video_path() -> PathBuf {
    assets_dir().join("video/gameplay.mp4")
}

/// Returns the path to the test audio file.
pub fn test_audio_path() -> PathBuf {
    assets_dir().join("audio/konekonoosanpo.mp3")
}

/// Returns the path to the test JPEG image file.
pub fn test_jpeg_path() -> PathBuf {
    assets_dir().join("img/hello-triangle.jpg")
}

// ============================================================================
// Decoder Creation Helpers
// ============================================================================

/// Creates a basic video decoder with default settings.
///
/// Uses software decoding (no hardware acceleration) for consistent behavior
/// across different test environments.
pub fn create_decoder() -> Result<VideoDecoder, ff_decode::DecodeError> {
    VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::None)
        .build()
}

/// Creates a basic audio decoder with default settings.
pub fn create_audio_decoder() -> Result<AudioDecoder, ff_decode::DecodeError> {
    AudioDecoder::open(&test_audio_path()).build()
}
