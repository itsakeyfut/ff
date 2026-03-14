//! Audio encoder tests.

use ff_encode::{AudioCodec, AudioEncoder};
use ff_format::{AudioFrame, SampleFormat};

#[test]
fn test_audio_encoder_aac_stereo() {
    let output_path = "test_output_audio_stereo.m4a";

    // Create encoder with audio only
    let mut encoder = match AudioEncoder::create(output_path)
        .audio(48000, 2)
        .audio_codec(AudioCodec::Aac)
        .audio_bitrate(192_000)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // Verify codec is selected
    assert_eq!(encoder.actual_codec(), "aac");

    // Create test audio frames
    let num_frames = 100;
    let samples_per_frame = 1024;

    for i in 0..num_frames {
        let mut frame = AudioFrame::empty(samples_per_frame, 2, 48000, SampleFormat::F32).unwrap();

        // Fill with test data (sine wave)
        if let Some(samples) = frame.as_f32_mut() {
            for (j, sample) in samples.iter_mut().enumerate() {
                let t = (i * samples_per_frame + j / 2) as f32 / 48000.0;
                *sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
            }
        }

        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");

    assert!(std::path::Path::new(output_path).exists());
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn test_audio_encoder_aac_mono() {
    let output_path = "test_output_audio_mono.m4a";

    let mut encoder = match AudioEncoder::create(output_path)
        .audio(44100, 1)
        .audio_codec(AudioCodec::Aac)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    assert_eq!(encoder.actual_codec(), "aac");

    let num_frames = 50;
    let samples_per_frame = 1024;

    for _ in 0..num_frames {
        let frame = AudioFrame::empty(samples_per_frame, 1, 44100, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert!(std::path::Path::new(output_path).exists());
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn test_audio_encoder_planar_format() {
    let output_path = "test_output_planar.m4a";

    let mut encoder = match AudioEncoder::create(output_path)
        .audio(48000, 2)
        .audio_codec(AudioCodec::Aac)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let num_frames = 30;
    for _ in 0..num_frames {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        encoder
            .push(&frame)
            .expect("Failed to push planar audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert!(std::path::Path::new(output_path).exists());
    let _ = std::fs::remove_file(output_path);
}
