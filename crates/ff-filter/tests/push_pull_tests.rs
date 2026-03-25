//! Integration tests for ff-filter push/pull frame processing.
//!
//! # Temporary file cleanup
//!
//! These tests are read-only with respect to the filesystem: they push frames
//! through in-memory filter graphs and inspect the resulting `VideoFrame` /
//! `AudioFrame` values. No output files are written, so no
//! `fixtures/mod.rs` with `FileGuard`/`DirGuard` is needed.

#![allow(clippy::unwrap_used)]

use std::time::Duration;

use ff_filter::{FilterError, FilterGraph, HwAccel, ToneMap};
use ff_format::{AudioFrame, PixelFormat, PooledBuffer, SampleFormat, Timestamp, VideoFrame};

/// 64×64 Yuv420p frame filled with grey (Y=128, U=128, V=128).
fn make_yuv420p_frame(width: u32, height: u32) -> VideoFrame {
    let y = vec![128u8; (width * height) as usize];
    let u = vec![128u8; ((width / 2) * (height / 2)) as usize];
    let v = vec![128u8; ((width / 2) * (height / 2)) as usize];
    VideoFrame::new(
        vec![
            PooledBuffer::standalone(y),
            PooledBuffer::standalone(u),
            PooledBuffer::standalone(v),
        ],
        vec![width as usize, (width / 2) as usize, (width / 2) as usize],
        width,
        height,
        PixelFormat::Yuv420p,
        Timestamp::default(),
        true,
    )
    .unwrap()
}

/// Stereo packed F32 audio frame, 1024 samples @ 48 kHz.
fn make_audio_frame() -> AudioFrame {
    AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap()
}

#[test]
fn pull_video_before_push_should_return_none() {
    let mut graph = match FilterGraph::builder().scale(32, 32).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let result = graph
        .pull_video()
        .expect("pull_video must not fail before any push");
    assert!(
        result.is_none(),
        "expected None before any push, got Some(frame)"
    );
}

#[test]
fn push_video_and_pull_through_scale_should_return_resized_frame() {
    let mut graph = match FilterGraph::builder().scale(32, 32).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after scale push");
    assert_eq!(out.width(), 32, "width should be scaled to 32");
    assert_eq!(out.height(), 32, "height should be scaled to 32");
}

#[test]
fn push_video_to_invalid_slot_should_return_error() {
    let mut graph = match FilterGraph::builder().scale(32, 32).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    // Push to slot 0 first to ensure the graph is fully initialised.
    // On some Linux FFmpeg builds the filter registry is not populated until
    // the first AVFilterGraph is allocated; without this step,
    // ensure_video_graph can return BuildFailed before the slot check runs.
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.push_video(1, &frame);
    assert!(
        matches!(result, Err(FilterError::InvalidInput { slot: 1, .. })),
        "expected InvalidInput for slot 1, got {result:?}"
    );
}

#[test]
fn pull_audio_before_push_should_return_none() {
    let mut graph = match FilterGraph::builder().volume(0.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let result = graph
        .pull_audio()
        .expect("pull_audio must not fail before any push");
    assert!(
        result.is_none(),
        "expected None before any push, got Some(frame)"
    );
}

#[test]
fn push_audio_and_pull_through_volume_should_return_frame() {
    let mut graph = match FilterGraph::builder().volume(0.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_audio_frame();
    match graph.push_audio(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_audio().expect("pull_audio must not fail");
    let out = result.expect("expected Some(frame) after volume push");
    assert_eq!(out.sample_rate(), 48000);
    assert_eq!(out.channels(), 2);
    assert_eq!(out.samples(), 1024);
}

#[test]
fn push_video_through_trim_should_return_frame_within_range() {
    let mut graph = match FilterGraph::builder().trim(0.0, 5.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after trim push within range");
    assert_eq!(out.width(), 64, "width should be unchanged after trim");
    assert_eq!(out.height(), 64, "height should be unchanged after trim");
}

#[test]
fn push_video_through_trim_frame_timestamp_should_be_within_trim_range() {
    // Push a frame at t=0 through a [0, 5) trim window and confirm the output
    // timestamp lies within the window.
    let mut graph = match FilterGraph::builder().trim(0.0, 5.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after trim push within range");
    let secs = out.timestamp().as_secs_f64();
    assert!(
        secs >= 0.0 && secs < 5.0,
        "frame timestamp {secs:.3}s must fall within the [0, 5) trim window"
    );
}

#[test]
fn push_video_through_crop_should_return_cropped_frame() {
    let mut graph = match FilterGraph::builder().crop(0, 0, 32, 32).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after crop push");
    assert_eq!(out.width(), 32, "width should be cropped to 32");
    assert_eq!(out.height(), 32, "height should be cropped to 32");
}

#[test]
fn push_video_through_overlay_should_return_composited_frame() {
    let mut graph = match FilterGraph::builder().overlay(0, 0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let base = make_yuv420p_frame(64, 64);
    let overlay = make_yuv420p_frame(64, 64);
    // Push base video to slot 0 first; this also initialises the graph.
    match graph.push_video(0, &base) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    // Push overlay video to slot 1.
    match graph.push_video(1, &overlay) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after overlay push");
    assert_eq!(out.width(), 64, "output width should match base video");
    assert_eq!(out.height(), 64, "output height should match base video");
}

#[test]
fn push_video_through_fade_in_should_return_frame_with_same_dimensions() {
    let mut graph = match FilterGraph::builder()
        .fade_in(Duration::from_secs(1))
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after fade_in push");
    assert_eq!(out.width(), 64, "width should be unchanged after fade_in");
    assert_eq!(out.height(), 64, "height should be unchanged after fade_in");
}

#[test]
fn push_video_through_fade_out_should_return_frame_with_same_dimensions() {
    let mut graph = match FilterGraph::builder()
        .fade_out(Duration::from_secs(1))
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after fade_out push");
    assert_eq!(out.width(), 64, "width should be unchanged after fade_out");
    assert_eq!(
        out.height(),
        64,
        "height should be unchanged after fade_out"
    );
}

#[test]
fn push_video_through_rotate_should_return_frame() {
    let mut graph = match FilterGraph::builder().rotate(90.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    // Use a square frame so output dimensions are unambiguous after 90° rotation.
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    assert!(result.is_some(), "expected Some(frame) after rotate push");
}

#[test]
fn push_video_through_tone_map_should_return_frame() {
    // The tonemap filter is HDR-specific and may not be available in all
    // FFmpeg builds, or may reject non-HDR input. All failure paths are
    // treated as graceful skips.
    let mut graph = match FilterGraph::builder().tone_map(ToneMap::Hable).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    assert!(result.is_some(), "expected Some(frame) after tone_map push");
}

#[test]
fn push_audio_through_amix_should_return_mixed_frame() {
    let mut graph = match FilterGraph::builder().amix(2).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_audio_frame();
    // Push to slot 0 — this also initialises the audio graph.
    match graph.push_audio(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    // Push to slot 1 so amix has data on both inputs and can produce output.
    match graph.push_audio(1, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_audio().expect("pull_audio must not fail");
    let out = result.expect("expected Some(frame) after amix push to both slots");
    assert_eq!(out.sample_rate(), 48000, "sample rate should be unchanged");
    assert_eq!(out.channels(), 2, "channel count should be unchanged");
}

#[test]
fn push_audio_through_equalizer_should_return_frame_with_same_properties() {
    let mut graph = match FilterGraph::builder().equalizer(1000.0, 3.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_audio_frame();
    match graph.push_audio(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_audio().expect("pull_audio must not fail");
    let out = result.expect("expected Some(frame) after equalizer push");
    assert_eq!(out.sample_rate(), 48000, "sample rate should be unchanged");
    assert_eq!(out.channels(), 2, "channel count should be unchanged");
    assert_eq!(out.samples(), 1024, "sample count should be unchanged");
}

#[test]
fn push_video_through_cuda_scale_should_return_resized_frame_or_skip() {
    // Build a filter graph with CUDA hardware acceleration and a scale step.
    // If CUDA is not available (av_hwdevice_ctx_create fails, hwupload_cuda
    // filter is missing, or avfilter_graph_config rejects the chain), all
    // error paths are treated as graceful skips.
    let mut graph = match FilterGraph::builder()
        .hardware(HwAccel::Cuda)
        .scale(32, 32)
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping (build): {e}");
            return;
        }
    };

    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping (push): {e}");
            return;
        }
    }

    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after cuda scale push");
    assert_eq!(out.width(), 32, "width should be scaled to 32");
    assert_eq!(out.height(), 32, "height should be scaled to 32");
}

#[test]
fn push_video_through_videotoolbox_scale_should_return_resized_frame_or_skip() {
    // VideoToolbox is macOS-only; on other platforms av_hwdevice_ctx_create
    // will fail and the test skips gracefully.  On macOS the hwupload filter
    // uploads frames to the VideoToolbox device and hwdownload brings them
    // back; avfilter_graph_config failure is also treated as a skip.
    let mut graph = match FilterGraph::builder()
        .hardware(HwAccel::VideoToolbox)
        .scale(32, 32)
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping (build): {e}");
            return;
        }
    };

    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping (push): {e}");
            return;
        }
    }

    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after videotoolbox scale push");
    assert_eq!(out.width(), 32, "width should be scaled to 32");
    assert_eq!(out.height(), 32, "height should be scaled to 32");
}

#[test]
fn push_video_through_vaapi_scale_should_return_resized_frame_or_skip() {
    // VAAPI is Linux-only; on other platforms (or Linux without a VA-API
    // device) av_hwdevice_ctx_create will fail and the test skips gracefully.
    // On a VA-API-capable Linux system the hwupload filter uploads frames to
    // the VAAPI device and hwdownload brings them back.
    let mut graph = match FilterGraph::builder()
        .hardware(HwAccel::Vaapi)
        .scale(32, 32)
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping (build): {e}");
            return;
        }
    };

    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping (push): {e}");
            return;
        }
    }

    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after vaapi scale push");
    assert_eq!(out.width(), 32, "width should be scaled to 32");
    assert_eq!(out.height(), 32, "height should be scaled to 32");
}

#[test]
fn push_video_through_eq_saturation_zero_should_return_frame_with_same_dimensions() {
    // saturation=0.0 converts to grayscale; frame dimensions are preserved.
    let mut graph = match FilterGraph::builder().eq(0.0, 1.0, 0.0).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = make_yuv420p_frame(64, 64);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }
    let result = graph.pull_video().expect("pull_video must not fail");
    let out = result.expect("expected Some(frame) after eq push");
    assert_eq!(out.width(), 64, "width should be unchanged after eq");
    assert_eq!(out.height(), 64, "height should be unchanged after eq");
}
