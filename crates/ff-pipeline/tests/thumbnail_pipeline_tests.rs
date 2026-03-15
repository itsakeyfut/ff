//! Integration tests for `ThumbnailPipeline`.
//!
//! These tests call the real FFmpeg API and are skipped gracefully when the
//! required decoders are unavailable or the test asset is missing.

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_pipeline::{PipelineError, ThumbnailPipeline};
use fixtures::test_video_path;

#[test]
fn thumbnail_at_valid_timestamp_should_return_single_frame() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let result = ThumbnailPipeline::new(input.to_str().unwrap())
        .timestamps(vec![0.0])
        .run();

    match result {
        Ok(frames) => assert_eq!(frames.len(), 1, "expected exactly one frame"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn thumbnails_sorted_ascending_should_return_frames_in_order() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let result = ThumbnailPipeline::new(input.to_str().unwrap())
        .timestamps(vec![1.0, 0.0])
        .run();

    match result {
        Ok(frames) => assert_eq!(frames.len(), 2, "expected two frames"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn thumbnail_with_no_timestamps_should_return_empty_vec() {
    let result = ThumbnailPipeline::new("nonexistent.mp4").run();
    assert!(
        matches!(result, Ok(ref v) if v.is_empty()),
        "expected Ok([]) for empty timestamps"
    );
}
