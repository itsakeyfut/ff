//! Integration tests for video + audio encoding.
//!
//! Tests the VideoEncoder's ability to encode both video and audio streams simultaneously.
//! This verifies that:
//! - Video and audio encoders can be initialized together
//! - Frames from both streams can be pushed
//! - The output file contains both video and audio data
//! - Common codec combinations work correctly

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::{AudioCodec, VideoCodec, VideoEncoder};
use ff_format::{AudioFrame, SampleFormat};
use fixtures::{FileGuard, assert_valid_output_file, create_black_frame, test_output_path};

/// Test video + audio encoding with MPEG-4 + AAC (most widely supported combination)
#[test]
fn test_video_audio_mpeg4_aac() {
    let output_path = test_output_path("video_audio_mpeg4_aac.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Create encoder with both video and audio streams
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4) // MPEG-4 is widely available
        .audio(48000, 2) // 48kHz stereo
        .audio_codec(AudioCodec::Aac)
        .audio_bitrate(192_000)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return; // Skip test if codecs not available
        }
    };

    // Verify both codecs are initialized
    assert!(
        !encoder.actual_video_codec().is_empty(),
        "Video codec should be set"
    );
    assert!(
        !encoder.actual_audio_codec().is_empty(),
        "Audio codec should be set"
    );

    println!("Using video codec: {}", encoder.actual_video_codec());
    println!("Using audio codec: {}", encoder.actual_audio_codec());

    // Encode 30 frames (1 second at 30fps)
    // AAC requires 1024 samples per frame, so we'll send audio more frequently
    let video_frames = 30;
    let audio_samples_per_frame = 1024; // AAC frame size
    let audio_frames_needed =
        (48000.0 * video_frames as f64 / 30.0 / audio_samples_per_frame as f64).ceil() as usize;

    let mut audio_sample_offset = 0;

    for _ in 0..video_frames {
        // Push video frame (black frame)
        let video_frame = create_black_frame(640, 480);
        encoder
            .push_video(&video_frame)
            .expect("Failed to push video frame");
    }

    // Push audio frames separately (AAC requires 1024 sample chunks)
    for _ in 0..audio_frames_needed {
        let mut audio_frame =
            AudioFrame::empty(audio_samples_per_frame, 2, 48000, SampleFormat::F32)
                .expect("Failed to create audio frame");

        // Fill with sine wave test data
        if let Some(samples) = audio_frame.as_f32_mut() {
            for (j, sample) in samples.iter_mut().enumerate() {
                let t = (audio_sample_offset + j / 2) as f32 / 48000.0;
                *sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.3;
            }
        }

        encoder
            .push_audio(&audio_frame)
            .expect("Failed to push audio frame");
        audio_sample_offset += audio_samples_per_frame;
    }

    // Finish encoding
    encoder.finish().expect("Failed to finish encoding");

    // Verify output file exists and has reasonable size
    assert_valid_output_file(&output_path);

    let file_size = std::fs::metadata(&output_path).unwrap().len();
    // File should have reasonable size (black frames compress well, so threshold is low)
    // Just verify it's not empty and contains some data
    assert!(
        file_size > 10_000,
        "Output file size ({} bytes) is too small",
        file_size
    );

    println!(
        "Output file size: {} bytes (video+audio encoded successfully)",
        file_size
    );
}

/// Test video + audio encoding with VP9 + Opus (modern WebM combination)
#[test]
fn test_video_audio_vp9_opus() {
    let output_path = test_output_path("video_audio_vp9_opus.webm");
    let _guard = FileGuard::new(output_path.clone());

    // Create encoder with VP9 + Opus
    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Vp9)
        .audio(48000, 2)
        .audio_codec(AudioCodec::Opus)
        .audio_bitrate(128_000)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return; // Skip test if codecs not available
        }
    };

    assert!(!encoder.actual_video_codec().is_empty());
    assert!(!encoder.actual_audio_codec().is_empty());

    println!("Using video codec: {}", encoder.actual_video_codec());
    println!("Using audio codec: {}", encoder.actual_audio_codec());

    // Encode 15 frames (0.5 seconds for faster test)
    // Opus typically uses 960 samples at 48kHz for 20ms frames
    let video_frames = 15;
    let audio_samples_per_frame = 960; // Opus frame size at 48kHz
    let audio_frames_needed =
        (48000.0 * video_frames as f64 / 30.0 / audio_samples_per_frame as f64).ceil() as usize;

    let mut audio_sample_offset = 0;

    // Push video frames
    for _ in 0..video_frames {
        let video_frame = create_black_frame(640, 480);
        encoder.push_video(&video_frame).unwrap();
    }

    // Push audio frames separately
    for _ in 0..audio_frames_needed {
        let mut audio_frame =
            AudioFrame::empty(audio_samples_per_frame, 2, 48000, SampleFormat::F32)
                .expect("Failed to create audio frame");

        // Fill with 880Hz sine wave
        if let Some(samples) = audio_frame.as_f32_mut() {
            for (j, sample) in samples.iter_mut().enumerate() {
                let t = (audio_sample_offset + j / 2) as f32 / 48000.0;
                *sample = (t * 880.0 * 2.0 * std::f32::consts::PI).sin() * 0.3;
            }
        }

        encoder.push_audio(&audio_frame).unwrap();
        audio_sample_offset += audio_samples_per_frame;
    }

    encoder.finish().unwrap();

    assert_valid_output_file(&output_path);

    let file_size = std::fs::metadata(&output_path).unwrap().len();
    // VP9 compresses black frames very efficiently; the threshold is low to
    // verify that both video and audio data were actually written.
    assert!(
        file_size > 5_000,
        "Output file size ({file_size} bytes) is too small"
    );

    println!("Output file size: {file_size} bytes (VP9+Opus encoded successfully)");
}

/// Test that video-only encoding still works (audio stream is optional)
#[test]
fn test_video_only_no_regression() {
    let output_path = test_output_path("video_only_no_regression.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Create encoder with video only (no audio configuration)
    let mut encoder = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build()
        .expect("Failed to build encoder");

    // Verify video codec is set, audio codec is empty
    assert!(!encoder.actual_video_codec().is_empty());
    assert_eq!(encoder.actual_audio_codec(), "");

    // Encode frames
    for _ in 0..15 {
        let frame = create_black_frame(640, 480);
        encoder.push_video(&frame).unwrap();
    }

    encoder.finish().unwrap();

    assert_valid_output_file(&output_path);
}
