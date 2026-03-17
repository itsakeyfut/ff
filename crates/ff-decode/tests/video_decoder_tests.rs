//! Basic VideoDecoder tests covering decoder creation, stream info,
//! frame decoding, decoder state, thread configuration, and error handling.

use std::path::PathBuf;
use std::time::Duration;

mod fixtures;
use fixtures::*;

use ff_decode::{HardwareAccel, VideoDecoder};
use ff_format::PixelFormat;

// ============================================================================
// Basic Decoder Creation Tests
// ============================================================================

#[test]
fn test_decoder_opens_successfully() {
    let result = create_decoder();
    assert!(
        result.is_ok(),
        "Failed to open video file: {:?}",
        result.err()
    );
}

#[test]
fn test_decoder_nonexistent_file() {
    let path = assets_dir().join("nonexistent-file.mp4");
    let result = VideoDecoder::open(&path)
        .hardware_accel(HardwareAccel::None)
        .build();

    assert!(result.is_err(), "Opening nonexistent file should fail");
}

// ============================================================================
// Stream Information Tests
// ============================================================================

#[test]
fn test_decoder_stream_info() {
    let decoder = create_decoder().expect("Failed to create decoder");
    let info = decoder.stream_info();

    // Verify basic properties
    assert!(info.width() > 0, "Video width should be positive");
    assert!(info.height() > 0, "Video height should be positive");

    // Frame rate should be valid
    let fps = info.frame_rate();
    let fps_value = fps.num() as f64 / fps.den() as f64;
    assert!(
        fps_value > 0.0 && fps_value < 120.0,
        "Frame rate {} seems unusual",
        fps_value
    );
}

#[test]
fn test_decoder_stream_info_pixel_format() {
    let decoder = create_decoder().expect("Failed to create decoder");
    let info = decoder.stream_info();

    // Pixel format should be a known format
    let format = info.pixel_format();
    assert!(
        !matches!(format, PixelFormat::Other(_)),
        "Pixel format should be a known format, got {:?}",
        format
    );
}

#[test]
fn test_decoder_stream_info_codec() {
    let decoder = create_decoder().expect("Failed to create decoder");
    let info = decoder.stream_info();

    // Codec should be set
    let codec = info.codec();
    assert!(
        codec != ff_format::codec::VideoCodec::Unknown,
        "Video codec should be known"
    );
}

#[test]
fn test_decoder_stream_info_duration() {
    let decoder = create_decoder().expect("Failed to create decoder");
    let info = decoder.stream_info();

    // Duration should be present and reasonable
    if let Some(duration) = info.duration() {
        assert!(duration > Duration::ZERO, "Duration should be positive");
        assert!(
            duration < Duration::from_secs(3600),
            "Duration seems unreasonably long for a test file"
        );
    }
}

// ============================================================================
// Frame Decoding Tests
// ============================================================================

#[test]
fn test_decode_first_frame() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode first frame
    let result = decoder.decode_one();
    assert!(
        result.is_ok(),
        "Failed to decode first frame: {:?}",
        result.err()
    );

    let frame_opt = result.unwrap();
    assert!(frame_opt.is_some(), "First frame should be Some");

    let frame = frame_opt.unwrap();

    // Verify frame properties
    let info = decoder.stream_info();
    assert_eq!(
        frame.width(),
        info.width(),
        "Frame width should match stream info"
    );
    assert_eq!(
        frame.height(),
        info.height(),
        "Frame height should match stream info"
    );
}

#[test]
fn test_decode_multiple_frames() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode first 10 frames
    let mut frame_count = 0;
    for i in 0..10 {
        let result = decoder.decode_one();
        assert!(
            result.is_ok(),
            "Failed to decode frame {}: {:?}",
            i,
            result.err()
        );

        if let Some(frame) = result.unwrap() {
            frame_count += 1;

            // Verify frame is valid
            assert!(
                frame.width() > 0,
                "Frame {} width should be positive",
                frame_count
            );
            assert!(
                frame.height() > 0,
                "Frame {} height should be positive",
                frame_count
            );
            assert!(
                !frame.planes().is_empty(),
                "Frame {} should have planes",
                frame_count
            );
        } else {
            break;
        }
    }

    assert!(frame_count > 0, "Should decode at least one frame");
    assert_eq!(frame_count, 10, "Should decode 10 frames");
}

#[test]
fn test_decode_frames_have_data() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode first frame
    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    // Verify planes have data
    let planes = frame.planes();
    assert!(!planes.is_empty(), "Frame should have at least one plane");

    for (i, plane) in planes.iter().enumerate() {
        assert!(!plane.is_empty(), "Plane {} should not be empty", i);
    }

    // Verify strides
    let strides = frame.strides();
    assert_eq!(
        strides.len(),
        planes.len(),
        "Strides count should match planes count"
    );

    for (i, &stride) in strides.iter().enumerate() {
        assert!(stride > 0, "Stride {} should be positive", i);
    }
}

#[test]
fn test_decode_frame_timestamps() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let mut last_pts = None;

    // Decode first few frames and verify timestamps are increasing
    for i in 0..5 {
        let frame = decoder
            .decode_one()
            .expect("Failed to decode")
            .unwrap_or_else(|| panic!("Frame {} should exist", i));

        let timestamp = frame.timestamp();
        let pts = timestamp.pts();

        if let Some(last) = last_pts {
            assert!(
                pts > last,
                "Timestamp should increase: frame {} pts={}, last_pts={}",
                i,
                pts,
                last
            );
        }

        last_pts = Some(pts);
    }
}

// ============================================================================
// Decoder State Tests
// ============================================================================

#[test]
fn test_decoder_is_eof() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Initially should not be EOF
    assert!(!decoder.is_eof(), "Decoder should not be EOF initially");

    // Decode all frames until EOF
    let mut frame_count = 0;
    loop {
        match decoder.decode_one() {
            Ok(Some(_)) => {
                frame_count += 1;
                // Limit to prevent infinite loop
                if frame_count > 100000 {
                    panic!("Too many frames, possible infinite loop");
                }
            }
            Ok(None) => {
                // EOF reached
                break;
            }
            Err(e) => {
                panic!("Decode error: {:?}", e);
            }
        }
    }

    // Should be EOF now
    assert!(
        decoder.is_eof(),
        "Decoder should be EOF after all frames decoded"
    );

    // Further decode_one calls should return None
    let result = decoder
        .decode_one()
        .expect("decode_one should not error at EOF");
    assert!(result.is_none(), "decode_one should return None at EOF");
}

// ============================================================================
// Thread Configuration Tests
// ============================================================================

#[test]
fn test_decoder_with_thread_count() {
    let path = test_video_path();
    let result = VideoDecoder::open(&path)
        .thread_count(4)
        .hardware_accel(HardwareAccel::None)
        .build();

    assert!(
        result.is_ok(),
        "Failed to create decoder with thread_count: {:?}",
        result.err()
    );

    let mut decoder = result.unwrap();
    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert!(frame.width() > 0, "Frame should be valid with thread_count");
}

#[test]
fn test_decoder_with_zero_threads_uses_auto() {
    let path = test_video_path();
    let result = VideoDecoder::open(&path)
        .thread_count(0) // 0 = auto
        .hardware_accel(HardwareAccel::None)
        .build();

    assert!(
        result.is_ok(),
        "Failed to create decoder with thread_count=0: {:?}",
        result.err()
    );

    let mut decoder = result.unwrap();
    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert!(
        frame.width() > 0,
        "Frame should be valid with auto thread_count"
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_decoder_invalid_path() {
    let path = PathBuf::from("/invalid/path/video.mp4");
    let result = VideoDecoder::open(&path)
        .hardware_accel(HardwareAccel::None)
        .build();

    assert!(result.is_err(), "Should fail to open invalid path");
}

// ============================================================================
// Frame Properties Validation Tests
// ============================================================================

#[test]
fn test_frame_dimensions_match_stream_info() {
    let mut decoder = create_decoder().expect("Failed to create decoder");
    let info = decoder.stream_info();

    let expected_width = info.width();
    let expected_height = info.height();

    // Decode several frames and verify dimensions
    for i in 0..5 {
        let frame = decoder
            .decode_one()
            .expect("Failed to decode")
            .unwrap_or_else(|| panic!("Frame {} should exist", i));

        assert_eq!(frame.width(), expected_width, "Frame {} width mismatch", i);
        assert_eq!(
            frame.height(),
            expected_height,
            "Frame {} height mismatch",
            i
        );
    }
}

#[test]
fn test_frame_pixel_format_consistency() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let first_frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    let expected_format = first_frame.format();

    // Decode more frames and verify format is consistent
    for i in 1..5 {
        let frame = decoder
            .decode_one()
            .expect("Failed to decode")
            .unwrap_or_else(|| panic!("Frame {} should exist", i));

        assert_eq!(
            frame.format(),
            expected_format,
            "Frame {} pixel format mismatch",
            i
        );
    }
}

#[test]
fn test_frame_data_size_matches_dimensions() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    let width = frame.width() as usize;
    let height = frame.height() as usize;

    // Calculate expected data size based on pixel format
    let expected_size = match frame.format() {
        PixelFormat::Rgba | PixelFormat::Bgra => width * height * 4,
        PixelFormat::Rgb24 | PixelFormat::Bgr24 => width * height * 3,
        PixelFormat::Yuv420p => width * height * 3 / 2,
        PixelFormat::Yuv422p => width * height * 2,
        PixelFormat::Yuv444p => width * height * 3,
        PixelFormat::Gray8 => width * height,
        PixelFormat::Nv12 | PixelFormat::Nv21 => width * height * 3 / 2,
        _ => return, // Skip for other formats
    };

    let total_size: usize = frame.planes().iter().map(|p| p.len()).sum();

    assert_eq!(
        total_size, expected_size,
        "Frame data size should match dimensions and format"
    );
}

#[test]
fn test_flush_decoder() {
    let mut decoder = create_decoder().expect("Failed to create decoder");

    // Decode a few frames
    for _ in 0..5 {
        let _ = decoder.decode_one().expect("Failed to decode");
    }

    // Flush the decoder
    decoder.flush();

    // Decoder should not be at EOF after flush
    assert!(!decoder.is_eof(), "Decoder should not be EOF after flush");

    // Should be able to decode frames after flush
    let frame = decoder
        .decode_one()
        .expect("Failed to decode after flush")
        .expect("Frame should exist after flush");

    assert!(frame.width() > 0, "Frame should be valid after flush");
}

#[test]
fn video_stream_info_codec_name_should_not_be_empty() {
    let decoder = create_decoder().expect("Failed to create decoder");
    let info = decoder.stream_info();

    assert!(
        !info.codec_name().is_empty(),
        "codec_name() should not be empty"
    );
}
