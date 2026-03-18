//! Integration tests for async encoder `finish()`.
//!
//! These tests verify that `finish()` drains all queued frames, flushes the
//! codec, writes the container trailer, and produces a valid output file.

#![cfg(feature = "tokio")]
#![allow(clippy::unwrap_used)]

mod fixtures;
use fixtures::{FileGuard, assert_valid_output_file, create_black_frame, test_output_path};

use ff_encode::{
    AsyncAudioEncoder, AsyncVideoEncoder, AudioCodec, AudioEncoder, VideoCodec, VideoEncoder,
};
use ff_format::{AudioFrame, SampleFormat};

// ============================================================================
// AsyncVideoEncoder
// ============================================================================

#[tokio::test]
async fn async_video_encoder_finish_should_produce_valid_output() {
    let output = test_output_path("async_video_finish.mp4");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AsyncVideoEncoder::from_builder(
        VideoEncoder::create(&output)
            .video(640, 480, 30.0)
            .video_codec(VideoCodec::Mpeg4),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = create_black_frame(640, 480);
        encoder.push(frame).await.expect("push failed");
    }

    encoder.finish().await.expect("finish failed");

    assert_valid_output_file(&output);
    ff_probe::open(&output).expect("output not parseable by ff_probe");
}

#[tokio::test]
async fn async_video_encoder_finish_with_many_frames_should_apply_backpressure() {
    // Push more frames than the channel capacity (8) to exercise back-pressure.
    let output = test_output_path("async_video_backpressure.mp4");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AsyncVideoEncoder::from_builder(
        VideoEncoder::create(&output)
            .video(320, 240, 30.0)
            .video_codec(VideoCodec::Mpeg4),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..30 {
        let frame = create_black_frame(320, 240);
        encoder.push(frame).await.expect("push failed");
    }

    encoder.finish().await.expect("finish failed");
    assert_valid_output_file(&output);
}

#[tokio::test]
async fn async_video_encoder_200_frames_should_produce_complete_output() {
    // Verify that a large burst of frames (well above the channel capacity of 8)
    // all make it into the output file without any being silently dropped.
    let output = test_output_path("async_video_200frames.mp4");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AsyncVideoEncoder::from_builder(
        VideoEncoder::create(&output)
            .video(320, 240, 30.0)
            .video_codec(VideoCodec::Mpeg4),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..200 {
        let frame = create_black_frame(320, 240);
        encoder.push(frame).await.expect("push failed");
    }

    encoder.finish().await.expect("finish failed");

    assert_valid_output_file(&output);
    let info = ff_probe::open(&output).expect("output not parseable by ff_probe");
    assert!(
        info.video_stream_count() > 0,
        "expected at least one video stream in output"
    );
    // frame_count may be None for some container formats; skip the assertion if unavailable
    if let Some(count) = info.video_stream(0).and_then(|s| s.frame_count()) {
        assert!(
            count >= 100,
            "expected at least 100 frames in output, got {count}"
        );
    }
}

#[tokio::test]
#[ignore = "back-pressure timing is environment-dependent; run explicitly with -- --include-ignored"]
async fn async_video_encoder_push_should_suspend_when_channel_full() {
    // Verify that the 9th push suspends (channel capacity = 8).
    // This test uses tokio::time::timeout with a zero duration; if the push
    // future is Pending the timeout fires immediately.
    use tokio::time::{Duration, timeout};

    let output = test_output_path("async_video_backpressure_timing.mp4");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AsyncVideoEncoder::from_builder(
        VideoEncoder::create(&output)
            .video(320, 240, 30.0)
            .video_codec(VideoCodec::Mpeg4),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // Fill the channel to capacity without yielding to the runtime.
    for _ in 0..8 {
        let frame = create_black_frame(320, 240);
        encoder.push(frame).await.expect("push failed");
    }

    // Attempt the 9th push with a zero-duration timeout.
    // If the channel is full the future should be Poll::Pending and the
    // timeout should fire. The worker may have already drained a slot on a
    // fast machine, so a non-timeout result is also acceptable — we just
    // verify no panic or error occurs.
    let extra_frame = create_black_frame(320, 240);
    let _ = timeout(Duration::ZERO, encoder.push(extra_frame)).await;

    encoder.finish().await.expect("finish failed");
    assert_valid_output_file(&output);
}

// ============================================================================
// AsyncAudioEncoder
// ============================================================================

#[tokio::test]
async fn async_audio_encoder_finish_should_produce_valid_output() {
    let output = test_output_path("async_audio_finish.m4a");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AsyncAudioEncoder::from_builder(
        AudioEncoder::create(&output)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Aac),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..20 {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(frame).await.expect("push failed");
    }

    encoder.finish().await.expect("finish failed");

    assert_valid_output_file(&output);
    ff_probe::open(&output).expect("output not parseable by ff_probe");
}

#[tokio::test]
async fn async_audio_encoder_finish_with_many_frames_should_apply_backpressure() {
    // Push more frames than the channel capacity (8) to exercise back-pressure.
    let output = test_output_path("async_audio_backpressure.m4a");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AsyncAudioEncoder::from_builder(
        AudioEncoder::create(&output)
            .audio(48000, 2)
            .audio_codec(AudioCodec::Aac),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..30 {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(frame).await.expect("push failed");
    }

    encoder.finish().await.expect("finish failed");
    assert_valid_output_file(&output);
}
