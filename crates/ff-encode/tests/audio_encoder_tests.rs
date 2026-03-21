//! Audio encoder tests.

use std::path::PathBuf;

use ff_decode::AudioDecoder;
use ff_encode::{
    AacOptions, AacProfile, AudioCodec, AudioCodecOptions, AudioEncoder, EncodeError, FlacOptions,
    Mp3Options, Mp3Quality, OpusApplication, OpusOptions,
};
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

// ── AudioCodecOptions integration tests ──────────────────────────────────────

#[test]
fn opus_audio_options_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("opus_codec_opts.opus"));

    let opts = OpusOptions {
        application: OpusApplication::Audio,
        frame_duration_ms: None,
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(48000, 2)
        .audio_codec(AudioCodec::Opus)
        .codec_options(AudioCodecOptions::Opus(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(960, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn aac_lc_profile_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("aac_codec_opts.m4a"));

    let opts = AacOptions {
        profile: AacProfile::Lc,
        vbr_quality: None,
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(48000, 2)
        .audio_codec(AudioCodec::Aac)
        .codec_options(AudioCodecOptions::Aac(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn flac_compression_level_options_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("flac_codec_opts.flac"));

    let opts = FlacOptions {
        compression_level: 5,
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(44100, 2)
        .audio_codec(AudioCodec::Flac)
        .codec_options(AudioCodecOptions::Flac(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(4096, 2, 44100, SampleFormat::F32p).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn mp3_quality_options_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("mp3_codec_opts.mp3"));

    let opts = Mp3Options {
        quality: Mp3Quality::Vbr(2),
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(44100, 2)
        .audio_codec(AudioCodec::Mp3)
        .codec_options(AudioCodecOptions::Mp3(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(1152, 2, 44100, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn opus_low_delay_application_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("opus_low_delay.opus"));

    let opts = OpusOptions {
        application: OpusApplication::LowDelay,
        frame_duration_ms: None,
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(48000, 2)
        .audio_codec(AudioCodec::Opus)
        .codec_options(AudioCodecOptions::Opus(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(960, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn opus_frame_duration_20ms_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("opus_frame_duration_20ms.opus"));

    let opts = OpusOptions {
        application: OpusApplication::Audio,
        frame_duration_ms: Some(20),
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(48000, 2)
        .audio_codec(AudioCodec::Opus)
        .codec_options(AudioCodecOptions::Opus(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // 20 ms at 48000 Hz = 960 samples per frame
    for _ in 0..10 {
        let frame = AudioFrame::empty(960, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn opus_invalid_frame_duration_should_return_invalid_option_error() {
    let output = test_output_path("opus_invalid_frame_duration.opus");

    let opts = OpusOptions {
        application: OpusApplication::Audio,
        frame_duration_ms: Some(15),
    };
    let result = AudioEncoder::create(&output)
        .audio(48000, 2)
        .audio_codec(AudioCodec::Opus)
        .codec_options(AudioCodecOptions::Opus(opts))
        .build();

    assert!(
        matches!(result, Err(EncodeError::InvalidOption { ref name, .. }) if name == "frame_duration_ms"),
        "expected InvalidOption for frame_duration_ms"
    );
}

#[test]
fn aac_vbr_quality_3_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("aac_vbr_quality.m4a"));

    let opts = AacOptions {
        profile: AacProfile::Lc,
        vbr_quality: Some(3),
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(48000, 2)
        .audio_codec(AudioCodec::Aac)
        .codec_options(AudioCodecOptions::Aac(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn aac_vbr_quality_out_of_range_should_return_invalid_option_error() {
    let output = test_output_path("aac_vbr_invalid.m4a");

    let opts = AacOptions {
        profile: AacProfile::Lc,
        vbr_quality: Some(6),
    };
    let result = AudioEncoder::create(&output)
        .audio(48000, 2)
        .audio_codec(AudioCodec::Aac)
        .codec_options(AudioCodecOptions::Aac(opts))
        .build();

    assert!(
        matches!(result, Err(EncodeError::InvalidOption { ref name, .. }) if name == "vbr_quality"),
        "expected InvalidOption for vbr_quality"
    );
}

#[test]
fn mp3_cbr_128kbps_should_produce_valid_output() {
    let output = FileGuard::new(test_output_path("mp3_cbr_128k.mp3"));

    let opts = Mp3Options {
        quality: Mp3Quality::Cbr(128_000),
    };
    let mut encoder = match AudioEncoder::create(output.path())
        .audio(44100, 2)
        .audio_codec(AudioCodec::Mp3)
        .codec_options(AudioCodecOptions::Mp3(opts))
        .build()
    {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    for _ in 0..10 {
        let frame = AudioFrame::empty(1152, 2, 44100, SampleFormat::F32).unwrap();
        encoder.push(&frame).expect("Failed to push audio frame");
    }

    encoder.finish().expect("Failed to finish encoding");
    assert_valid_output_file(output.path());
}

#[test]
fn mp3_vbr_quality_out_of_range_should_return_invalid_option_error() {
    let output = test_output_path("mp3_vbr_invalid.mp3");

    let opts = Mp3Options {
        quality: Mp3Quality::Vbr(10),
    };
    let result = AudioEncoder::create(&output)
        .audio(44100, 2)
        .audio_codec(AudioCodec::Mp3)
        .codec_options(AudioCodecOptions::Mp3(opts))
        .build();

    assert!(
        matches!(result, Err(EncodeError::InvalidOption { ref name, .. }) if name == "vbr_quality"),
        "expected InvalidOption for vbr_quality"
    );
}

#[test]
fn flac_compression_level_out_of_range_should_return_invalid_option_error() {
    let output = test_output_path("flac_level_invalid.flac");

    let opts = FlacOptions {
        compression_level: 13,
    };
    let result = AudioEncoder::create(&output)
        .audio(44100, 2)
        .audio_codec(AudioCodec::Flac)
        .codec_options(AudioCodecOptions::Flac(opts))
        .build();

    assert!(
        matches!(result, Err(EncodeError::InvalidOption { ref name, .. }) if name == "compression_level"),
        "expected InvalidOption for compression_level"
    );
}

#[test]
fn flac_level_0_should_produce_larger_file_than_level_12() {
    let output_level_0 = FileGuard::new(test_output_path("flac_level_0.flac"));
    let output_level_12 = FileGuard::new(test_output_path("flac_level_12.flac"));

    let encode_flac = |path: &std::path::PathBuf, level: u8| -> bool {
        let opts = FlacOptions {
            compression_level: level,
        };
        let mut encoder = match AudioEncoder::create(path)
            .audio(44100, 2)
            .audio_codec(AudioCodec::Flac)
            .codec_options(AudioCodecOptions::Flac(opts))
            .build()
        {
            Ok(enc) => enc,
            Err(e) => {
                println!("Skipping: {e}");
                return false;
            }
        };
        // Use a non-silent signal so compression ratio differs meaningfully.
        for i in 0..20u32 {
            let mut frame = AudioFrame::empty(4096, 2, 44100, SampleFormat::F32p).unwrap();
            if let Some(samples) = frame.as_f32_mut() {
                for (j, s) in samples.iter_mut().enumerate() {
                    let t = (i * 4096 + j as u32 / 2) as f32 / 44100.0;
                    *s = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
                }
            }
            encoder.push(&frame).expect("Failed to push frame");
        }
        encoder.finish().expect("Failed to finish encoding");
        true
    };

    if !encode_flac(output_level_0.path(), 0) || !encode_flac(output_level_12.path(), 12) {
        return;
    }

    let size_0 = std::fs::metadata(output_level_0.path()).unwrap().len();
    let size_12 = std::fs::metadata(output_level_12.path()).unwrap().len();
    assert!(
        size_0 >= size_12,
        "expected level 0 file ({size_0} bytes) to be >= level 12 file ({size_12} bytes)"
    );
}
