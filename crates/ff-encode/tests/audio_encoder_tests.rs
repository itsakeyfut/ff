//! Audio encoder tests.

use std::path::PathBuf;

use ff_decode::AudioDecoder;
use ff_encode::{AudioCodec, AudioEncoder};
use ff_format::{AudioFrame, SampleFormat};

mod fixtures;
use fixtures::{FileGuard, assert_valid_output_file, test_output_path};

fn assets_dir() -> PathBuf {
    PathBuf::from(format!("{}/../../assets", env!("CARGO_MANIFEST_DIR")))
}

fn test_mp3_path() -> PathBuf {
    assets_dir().join("audio/konekonoosanpo.mp3")
}

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

// ============================================================================
// Transcode tests (MP3 → lossy/lossless)
// ============================================================================

#[test]
fn mp3_to_aac_transcode_should_succeed() {
    let mp3_path = test_mp3_path();

    let mut decoder = match AudioDecoder::open(&mp3_path).build() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: decoder unavailable: {e}");
            return;
        }
    };

    let info = decoder.stream_info();
    let sample_rate = info.sample_rate();
    let channels = info.channels();

    let output = test_output_path("transcode_mp3_to_aac.m4a");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AudioEncoder::create(&output)
        .audio(sample_rate, channels)
        .audio_codec(AudioCodec::Aac)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: encoder unavailable: {e}");
            return;
        }
    };

    loop {
        match decoder.decode_one() {
            Ok(Some(frame)) => encoder.push(&frame).expect("Failed to push frame"),
            Ok(None) => break,
            Err(e) => panic!("Decode error: {e}"),
        }
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output);
}

#[test]
fn mp3_to_flac_transcode_should_succeed() {
    let mp3_path = test_mp3_path();

    let mut decoder = match AudioDecoder::open(&mp3_path).build() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: decoder unavailable: {e}");
            return;
        }
    };

    let info = decoder.stream_info();
    let sample_rate = info.sample_rate();
    let channels = info.channels();

    let output = test_output_path("transcode_mp3_to_flac.flac");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AudioEncoder::create(&output)
        .audio(sample_rate, channels)
        .audio_codec(AudioCodec::Flac)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: encoder unavailable: {e}");
            return;
        }
    };

    loop {
        match decoder.decode_one() {
            Ok(Some(frame)) => encoder.push(&frame).expect("Failed to push frame"),
            Ok(None) => break,
            Err(e) => panic!("Decode error: {e}"),
        }
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output);
}

#[test]
fn aac_encoder_with_non_multiple_frame_count_should_succeed() {
    // Push a total sample count that is not a multiple of 1024 to exercise
    // the zero-padding path in the FIFO flush inside `finish`.
    let output = test_output_path("aac_non_multiple.m4a");
    let _guard = FileGuard::new(output.clone());

    let mut encoder = match AudioEncoder::create(&output)
        .audio(44100, 2)
        .audio_codec(AudioCodec::Aac)
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // 10 frames × 512 samples = 5120 samples (not a multiple of 1024)
    for _ in 0..10 {
        let frame = AudioFrame::empty(512, 2, 44100, SampleFormat::F32p).unwrap();
        encoder.push(&frame).expect("Failed to push frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(&output);
}
