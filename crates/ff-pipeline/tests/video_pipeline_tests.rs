//! Integration tests for `VideoPipeline`.
//!
//! These tests call the real FFmpeg API and are skipped gracefully when the
//! required codecs are unavailable or the test asset is missing.

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_pipeline::{PipelineError, VideoPipeline};
use fixtures::{FileGuard, test_output_path, test_video_path};

#[test]
fn video_pipeline_should_produce_valid_output_file() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let out_path = test_output_path("video_pipeline_out.mp4");
    let _guard = FileGuard::new(out_path.clone());

    let result = VideoPipeline::new()
        .input(input.to_str().unwrap())
        .output(out_path.to_str().unwrap())
        .mute()
        .run();

    match result {
        Ok(()) => {
            assert!(out_path.exists(), "output file should exist");
            assert!(
                out_path.metadata().unwrap().len() > 0,
                "output file should be non-empty"
            );
        }
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn video_pipeline_with_no_output_should_return_no_output_error() {
    let result = VideoPipeline::new().input("in.mp4").run();
    assert!(matches!(result, Err(PipelineError::NoOutput)));
}

#[test]
fn video_pipeline_with_no_inputs_should_return_no_input_error() {
    let result = VideoPipeline::new().output("out.mp4").run();
    assert!(matches!(result, Err(PipelineError::NoInput)));
}
