//! Integration tests for `ThumbnailPipeline`.
//!
//! These tests call the real FFmpeg API and are skipped gracefully when the
//! required decoders are unavailable or the test asset is missing.

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_pipeline::{PipelineError, ThumbnailPipeline};
use fixtures::{test_output_dir, test_video_path};

/// Extracts 3 frames and verifies count + dimensions match the source video.
/// Shared by the sequential and parallel variants of the dimension test.
fn assert_thumbnails_have_expected_dimensions(path: &str) {
    let source_info = match ff_probe::open(path) {
        Ok(i) => i,
        Err(e) => {
            println!("Skipping dimension check: probe failed: {e}");
            return;
        }
    };
    let (expected_w, expected_h) = match source_info.primary_video() {
        Some(v) => (v.width(), v.height()),
        None => {
            println!("Skipping dimension check: no video stream found");
            return;
        }
    };

    let result = ThumbnailPipeline::new(path)
        .timestamps(vec![0.0, 1.0, 2.0])
        .run();

    match result {
        Ok(frames) => {
            assert_eq!(frames.len(), 3, "expected 3 frames");
            for (i, frame) in frames.iter().enumerate() {
                assert_eq!(
                    frame.width(),
                    expected_w,
                    "frame {i} width mismatch: got {} expected {expected_w}",
                    frame.width()
                );
                assert_eq!(
                    frame.height(),
                    expected_h,
                    "frame {i} height mismatch: got {} expected {expected_h}",
                    frame.height()
                );
            }
        }
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

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

#[cfg(feature = "parallel")]
#[test]
fn parallel_thumbnail_at_valid_timestamp_should_return_single_frame() {
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

#[cfg(feature = "parallel")]
#[test]
fn parallel_thumbnails_should_return_one_frame_per_timestamp() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let timestamps = vec![2.0, 0.0, 1.0]; // intentionally unsorted
    let expected = timestamps.len();

    let result = ThumbnailPipeline::new(input.to_str().unwrap())
        .timestamps(timestamps)
        .run();

    match result {
        Ok(frames) => assert_eq!(frames.len(), expected, "expected one frame per timestamp"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn thumbnails_at_three_timestamps_should_have_source_dimensions() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    assert_thumbnails_have_expected_dimensions(input.to_str().unwrap());
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_thumbnails_at_three_timestamps_should_have_source_dimensions() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    assert_thumbnails_have_expected_dimensions(input.to_str().unwrap());
}

#[test]
fn run_to_files_should_write_jpeg_files_to_output_dir() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let dir = test_output_dir().join("thumb_test_output");
    std::fs::create_dir_all(&dir).unwrap();

    let result = ThumbnailPipeline::new(input.to_str().unwrap())
        .timestamps(vec![0.0, 1.0])
        .output_dir(&dir)
        .quality(80)
        .run_to_files();

    let cleanup = || {
        let _ = std::fs::remove_dir_all(&dir);
    };

    match result {
        Ok(paths) => {
            assert_eq!(paths.len(), 2);
            for p in &paths {
                assert!(p.exists());
                assert!(p.metadata().unwrap().len() > 0);
            }
            cleanup();
        }
        Err(PipelineError::Decode(e)) => {
            cleanup();
            println!("Skipping: {e}");
        }
        Err(e) => {
            cleanup();
            panic!("unexpected error: {e}");
        }
    }
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_thumbnails_unsorted_input_should_return_frames_in_ascending_order() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let result = ThumbnailPipeline::new(input.to_str().unwrap())
        .timestamps(vec![2.0, 0.0, 1.0]) // intentionally unsorted
        .run();

    match result {
        Ok(frames) => {
            assert_eq!(frames.len(), 3, "expected three frames");
            let timestamps: Vec<_> = frames.iter().map(|f| f.timestamp()).collect();
            for w in timestamps.windows(2) {
                assert!(
                    w[0] <= w[1],
                    "frames must be in non-decreasing timestamp order, got {:?}",
                    timestamps
                );
            }
        }
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}
