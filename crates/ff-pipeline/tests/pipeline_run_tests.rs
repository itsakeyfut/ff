//! Integration tests for `Pipeline::run()`.
//!
//! These tests call the real FFmpeg API and are skipped gracefully when the
//! required codecs or decoders are unavailable.

#![allow(clippy::unwrap_used)]

mod fixtures;

use std::sync::{Arc, Mutex};

use ff_encode::{AudioCodec, BitrateMode, VideoCodec};
use ff_filter::FilterGraph;
use ff_pipeline::{EncoderConfig, Pipeline, PipelineError};
use fixtures::{FileGuard, test_output_path, test_video_path};

fn basic_config() -> EncoderConfig {
    EncoderConfig {
        video_codec: VideoCodec::Mpeg4,
        audio_codec: AudioCodec::Aac,
        bitrate_mode: BitrateMode::Cbr(1_000_000),
        resolution: Some((320, 240)),
        framerate: Some(24.0),
        hardware: None,
    }
}

#[test]
fn transcode_single_input_should_produce_valid_output_file() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_run_basic.mp4");
    let _guard = FileGuard::new(output.clone());

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .output(output.to_str().unwrap(), basic_config())
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: build failed: {e}");
            return;
        }
    };

    match pipeline.run() {
        Ok(()) => {
            assert!(output.exists(), "output file must exist after run");
            assert!(
                std::fs::metadata(&output).unwrap().len() > 0,
                "output file must be non-empty"
            );
        }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn transcode_cancelled_by_callback_should_return_cancelled() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_run_cancel.mp4");
    let _guard = FileGuard::new(output.clone());

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .output(output.to_str().unwrap(), basic_config())
        .on_progress(|_p| false) // cancel on first frame
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: build failed: {e}");
            return;
        }
    };

    match pipeline.run() {
        Err(PipelineError::Cancelled) => { /* expected */ }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Ok(()) => panic!("expected Cancelled but got Ok"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn transcode_progress_callback_should_receive_increasing_frame_counts() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_run_progress.mp4");
    let _guard = FileGuard::new(output.clone());

    let counts: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
    let counts_clone = Arc::clone(&counts);

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .output(output.to_str().unwrap(), basic_config())
        .on_progress(move |p| {
            counts_clone.lock().unwrap().push(p.frames_processed);
            true
        })
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: build failed: {e}");
            return;
        }
    };

    match pipeline.run() {
        Ok(()) => {
            let c = counts.lock().unwrap();
            assert!(
                !c.is_empty(),
                "progress callback must be called at least once"
            );
            for w in c.windows(2) {
                assert!(w[1] > w[0], "frame count must increase monotonically");
            }
        }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn transcode_with_scale_filter_should_produce_valid_output() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_run_filter.mp4");
    let _guard = FileGuard::new(output.clone());

    let filter = match FilterGraph::builder().scale(160, 120).build() {
        Ok(f) => f,
        Err(e) => {
            println!("Skipping: filter build failed: {e}");
            return;
        }
    };

    // Resolution matches filter output; no override needed.
    let config = EncoderConfig {
        video_codec: VideoCodec::Mpeg4,
        audio_codec: AudioCodec::Aac,
        bitrate_mode: BitrateMode::Cbr(500_000),
        resolution: Some((160, 120)),
        framerate: Some(24.0),
        hardware: None,
    };

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .filter(filter)
        .output(output.to_str().unwrap(), config)
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: build failed: {e}");
            return;
        }
    };

    match pipeline.run() {
        Ok(()) => {
            assert!(output.exists(), "output file must exist after run");
        }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(PipelineError::Filter(e)) => println!("Skipping: filter unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}
