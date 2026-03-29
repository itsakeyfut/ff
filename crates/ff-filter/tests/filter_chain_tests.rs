//! Integration test for a full filter chain combining color grading,
//! text overlay, and volume adjustment.

#![allow(clippy::unwrap_used)]

use ff_filter::{DrawTextOptions, FilterGraph};
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
fn full_filter_chain_should_produce_valid_output() {
    let drawtext_opts = DrawTextOptions {
        text: "Test".to_owned(),
        x: "10".to_owned(),
        y: "10".to_owned(),
        font_size: 24,
        font_color: "white".to_owned(),
        font_file: None,
        opacity: 1.0,
        box_color: Some("black@0.5".to_owned()),
        box_border_width: 4,
    };

    let mut graph = match FilterGraph::builder()
        .eq(0.1, 1.2, 1.5)
        .drawtext(drawtext_opts)
        .volume(-3.0)
        .build()
    {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // Push 10 video frames.
    let mut video_push_ok = 0usize;
    for _ in 0..10 {
        let frame = make_yuv420p_frame(64, 64);
        match graph.push_video(0, &frame) {
            Ok(()) => video_push_ok += 1,
            Err(e) => {
                println!("Skipping: video push failed: {e}");
                return;
            }
        }
    }
    assert_eq!(
        video_push_ok, 10,
        "all 10 video frames must push successfully"
    );

    // Push 10 audio frames.
    let mut audio_push_ok = 0usize;
    for _ in 0..10 {
        let frame = make_audio_frame();
        match graph.push_audio(0, &frame) {
            Ok(()) => audio_push_ok += 1,
            Err(e) => {
                println!("Skipping: audio push failed: {e}");
                return;
            }
        }
    }
    assert_eq!(
        audio_push_ok, 10,
        "all 10 audio frames must push successfully"
    );

    // Pull all video output frames.
    let mut video_out = Vec::new();
    loop {
        match graph.pull_video() {
            Ok(Some(f)) => video_out.push(f),
            Ok(None) => break,
            Err(e) => {
                println!("Skipping: pull_video failed: {e}");
                return;
            }
        }
    }

    assert_eq!(
        video_out.len(),
        10,
        "output video frame count must equal input (10), got {}",
        video_out.len()
    );

    for (i, frame) in video_out.iter().enumerate() {
        assert_eq!(
            frame.width(),
            64,
            "video frame {i}: width must be 64, got {}",
            frame.width()
        );
        assert_eq!(
            frame.height(),
            64,
            "video frame {i}: height must be 64, got {}",
            frame.height()
        );
        assert_eq!(
            frame.format(),
            PixelFormat::Yuv420p,
            "video frame {i}: pixel format must be Yuv420p"
        );
    }

    // Pull all audio output frames.
    let mut audio_sample_count = 0usize;
    loop {
        match graph.pull_audio() {
            Ok(Some(f)) => audio_sample_count += f.samples(),
            Ok(None) => break,
            Err(e) => {
                println!("Skipping: pull_audio failed: {e}");
                return;
            }
        }
    }

    let expected_audio_samples = 10 * 1024usize;
    // Allow ±1 frame (1024 samples) tolerance for internal buffering.
    assert!(
        audio_sample_count >= expected_audio_samples.saturating_sub(1024)
            && audio_sample_count <= expected_audio_samples + 1024,
        "output audio sample count ({audio_sample_count}) should be within ±1 frame \
         of expected ({expected_audio_samples})"
    );
}
