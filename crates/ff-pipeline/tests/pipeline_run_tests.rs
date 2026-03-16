//! Integration tests for `Pipeline::run()`.
//!
//! These tests call the real FFmpeg API and are skipped gracefully when the
//! required codecs or decoders are unavailable.

#![allow(clippy::unwrap_used)]

mod fixtures;

use std::sync::atomic::{AtomicU64, Ordering};
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
fn transcode_cancelled_should_leave_partial_output_on_disk() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_cancel_partial.mp4");
    let _guard = FileGuard::new(output.clone());

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .output(output.to_str().unwrap(), basic_config())
        .on_progress(|_p| false) // cancel immediately
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: build failed: {e}");
            return;
        }
    };

    match pipeline.run() {
        Err(PipelineError::Cancelled) => {
            assert!(
                output.exists(),
                "partial output file must remain on disk after cancellation"
            );
            assert!(
                std::fs::metadata(&output).unwrap().len() > 0,
                "partial output file must be non-empty"
            );
        }
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

#[test]
fn transcode_two_inputs_should_produce_larger_output_than_single_input() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let input_str = input.to_str().unwrap();

    let output_single = test_output_path("pipeline_concat_single.mp4");
    let output_double = test_output_path("pipeline_concat_double.mp4");
    let _guard_single = FileGuard::new(output_single.clone());
    let _guard_double = FileGuard::new(output_double.clone());

    // Single-input baseline.
    let single = Pipeline::builder()
        .input(input_str)
        .output(output_single.to_str().unwrap(), basic_config())
        .build()
        .unwrap();

    // Two copies of the same input concatenated.
    let double = Pipeline::builder()
        .input(input_str)
        .input(input_str)
        .output(output_double.to_str().unwrap(), basic_config())
        .build()
        .unwrap();

    let single_result = single.run();
    let double_result = double.run();

    match (single_result, double_result) {
        (Ok(()), Ok(())) => {
            let single_size = std::fs::metadata(&output_single).unwrap().len();
            let double_size = std::fs::metadata(&output_double).unwrap().len();
            assert!(
                double_size > single_size,
                "two-input output ({double_size} bytes) must be larger than \
                 single-input output ({single_size} bytes)"
            );
        }
        (Err(PipelineError::Encode(e)), _) | (_, Err(PipelineError::Encode(e))) => {
            println!("Skipping: encoder unavailable: {e}");
        }
        (Err(PipelineError::Decode(e)), _) | (_, Err(PipelineError::Decode(e))) => {
            println!("Skipping: decoder unavailable: {e}");
        }
        (Err(e), _) | (_, Err(e)) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn transcode_single_input_should_produce_nonzero_duration_and_call_progress() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_duration_progress.mp4");
    let _guard = FileGuard::new(output.clone());

    let call_count = Arc::new(AtomicU64::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .output(output.to_str().unwrap(), basic_config())
        .on_progress(move |_p| {
            call_count_clone.fetch_add(1, Ordering::Relaxed);
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
            assert!(output.exists(), "output file must exist after run");

            let info = match ff_probe::open(&output) {
                Ok(i) => i,
                Err(e) => {
                    println!("Skipping duration check: probe failed: {e}");
                    return;
                }
            };
            assert!(
                info.duration() > std::time::Duration::ZERO,
                "output must have non-zero duration, got {:?}",
                info.duration()
            );

            assert!(
                call_count.load(Ordering::Relaxed) >= 1,
                "progress callback must be called at least once"
            );
        }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn transcode_multi_input_frames_processed_should_be_sum_of_single_runs() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let input_str = input.to_str().unwrap();

    // Count frames for a single-input run.
    let output_single = test_output_path("pipeline_concat_frames_single.mp4");
    let _guard_single = FileGuard::new(output_single.clone());
    let single_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let sc = Arc::clone(&single_count);

    let single = match Pipeline::builder()
        .input(input_str)
        .output(output_single.to_str().unwrap(), basic_config())
        .on_progress(move |p| {
            sc.store(p.frames_processed, std::sync::atomic::Ordering::Relaxed);
            true
        })
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    if let Err(e) = single.run() {
        println!("Skipping single run: {e}");
        return;
    }
    let frames_single = single_count.load(std::sync::atomic::Ordering::Relaxed);

    // Count frames for a two-input run.
    let output_double = test_output_path("pipeline_concat_frames_double.mp4");
    let _guard_double = FileGuard::new(output_double.clone());
    let double_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let dc = Arc::clone(&double_count);

    let double = match Pipeline::builder()
        .input(input_str)
        .input(input_str)
        .output(output_double.to_str().unwrap(), basic_config())
        .on_progress(move |p| {
            dc.store(p.frames_processed, std::sync::atomic::Ordering::Relaxed);
            true
        })
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    if let Err(e) = double.run() {
        println!("Skipping double run: {e}");
        return;
    }
    let frames_double = double_count.load(std::sync::atomic::Ordering::Relaxed);

    assert_eq!(
        frames_double,
        frames_single * 2,
        "two-input pipeline must process exactly 2× the frames of a single-input run"
    );
}

#[test]
fn transcode_cancelled_after_first_callback_should_return_cancelled() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let output = test_output_path("pipeline_cancel_after_first.mp4");
    let _guard = FileGuard::new(output.clone());

    let call_count = Arc::new(AtomicU64::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let pipeline = match Pipeline::builder()
        .input(input.to_str().unwrap())
        .output(output.to_str().unwrap(), basic_config())
        .on_progress(move |_p| {
            // Allow the first call (return true), cancel on the second.
            call_count_clone.fetch_add(1, Ordering::Relaxed) == 0
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
        Err(PipelineError::Cancelled) => {
            assert!(
                call_count.load(Ordering::Relaxed) >= 2,
                "callback must have been invoked at least twice before cancellation"
            );
        }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Ok(()) => panic!("expected Cancelled but got Ok"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}

#[test]
fn concat_two_inputs_should_produce_output_with_approx_sum_duration() {
    let input = test_video_path();
    if !input.exists() {
        println!("Skipping: test asset not found at {input:?}");
        return;
    }
    let input_str = input.to_str().unwrap();

    // Probe the input to determine its duration.
    let input_info = match ff_probe::open(&input) {
        Ok(i) => i,
        Err(e) => {
            println!("Skipping: failed to probe input: {e}");
            return;
        }
    };
    let input_duration = input_info.duration();
    if input_duration == std::time::Duration::ZERO {
        println!("Skipping: input has zero duration");
        return;
    }

    let output = test_output_path("pipeline_concat_duration.mp4");
    let _guard = FileGuard::new(output.clone());

    let pipeline = match Pipeline::builder()
        .input(input_str)
        .input(input_str)
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
            let output_info = match ff_probe::open(&output) {
                Ok(i) => i,
                Err(e) => {
                    println!("Skipping duration check: probe failed: {e}");
                    return;
                }
            };
            let output_duration = output_info.duration();
            let expected = input_duration * 2;
            // Allow ±20% to account for encoding/muxing overhead.
            let tolerance = input_duration.mul_f64(0.4);
            assert!(
                output_duration >= expected.saturating_sub(tolerance),
                "output duration {output_duration:?} shorter than expected \
                 ~{expected:?} (tolerance ±{tolerance:?})"
            );
            assert!(
                output_duration <= expected + tolerance,
                "output duration {output_duration:?} longer than expected \
                 ~{expected:?} (tolerance ±{tolerance:?})"
            );
        }
        Err(PipelineError::Encode(e)) => println!("Skipping: encoder unavailable: {e}"),
        Err(PipelineError::Decode(e)) => println!("Skipping: decoder unavailable: {e}"),
        Err(e) => panic!("unexpected error: {e}"),
    }
}
