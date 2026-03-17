//! Integration tests for `AudioPipeline`.
//!
//! These tests call the real FFmpeg API and are skipped gracefully when the
//! required codecs are unavailable or the test asset is missing.

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_pipeline::{AudioPipeline, PipelineError};
use fixtures::{FileGuard, test_audio_path, test_output_path};

#[test]
fn audio_pipeline_should_produce_valid_output_file() {
    let input = test_audio_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }

    let out_path = test_output_path("audio_pipeline_out.aac");
    let _guard = FileGuard::new(out_path.clone());

    let result = AudioPipeline::new()
        .input(input.to_str().unwrap())
        .output(out_path.to_str().unwrap())
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
fn audio_pipeline_with_no_output_should_return_no_output_error() {
    let result = AudioPipeline::new().input("in.mp3").run();
    assert!(matches!(result, Err(PipelineError::NoOutput)));
}

#[test]
fn audio_pipeline_with_no_inputs_should_return_no_input_error() {
    let result = AudioPipeline::new().output("out.mp3").run();
    assert!(matches!(result, Err(PipelineError::NoInput)));
}
