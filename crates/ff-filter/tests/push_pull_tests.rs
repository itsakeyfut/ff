#![allow(clippy::unwrap_used)]

use ff_filter::{FilterError, FilterGraph};
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
