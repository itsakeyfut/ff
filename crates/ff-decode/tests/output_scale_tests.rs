//! Output scale tests for VideoDecoder.
//!
//! Tests `output_size()`, `output_width()`, and `output_height()` — the frame
//! scaling options backed by `libswscale`.

mod fixtures;
use fixtures::*;

use ff_decode::{DecodeError, HardwareAccel, VideoDecoder};
use ff_format::PixelFormat;

// ============================================================================
// output_size — exact dimensions
// ============================================================================

#[test]
fn output_size_should_scale_frame_to_exact_dimensions() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_size(320, 240)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(frame.width(), 320, "Width should be 320");
    assert_eq!(frame.height(), 240, "Height should be 240");
}

#[test]
fn output_size_combined_with_output_format_should_convert_and_scale() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Rgb24)
        .output_size(160, 120)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(frame.width(), 160);
    assert_eq!(frame.height(), 120);
    assert_eq!(frame.format(), PixelFormat::Rgb24);
}

// ============================================================================
// output_width — fit to width, preserve aspect ratio
// ============================================================================

#[test]
fn output_width_should_scale_to_given_width_and_preserve_aspect_ratio() {
    let path = test_video_path();

    // First decode at original size to get the source aspect ratio
    let mut src_decoder = VideoDecoder::open(&path)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let src_frame = src_decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");
    let src_w = src_frame.width() as f64;
    let src_h = src_frame.height() as f64;
    let src_ratio = src_w / src_h;

    // Decode scaled
    let target_w = 640u32;
    let mut decoder = VideoDecoder::open(&path)
        .output_width(target_w)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(frame.width(), target_w, "Width should match target");
    // Height must be even
    assert_eq!(frame.height() % 2, 0, "Height must be even");
    // Aspect ratio should be preserved within ±1 pixel tolerance
    let dst_ratio = frame.width() as f64 / frame.height() as f64;
    let tolerance = 1.0 / frame.height() as f64;
    assert!(
        (dst_ratio - src_ratio).abs() < tolerance + 0.02,
        "Aspect ratio should be preserved: src={src_ratio:.3} dst={dst_ratio:.3}"
    );
}

// ============================================================================
// output_height — fit to height, preserve aspect ratio
// ============================================================================

#[test]
fn output_height_should_scale_to_given_height_and_preserve_aspect_ratio() {
    let path = test_video_path();

    let mut src_decoder = VideoDecoder::open(&path)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");
    let src_frame = src_decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");
    let src_w = src_frame.width() as f64;
    let src_h = src_frame.height() as f64;
    let src_ratio = src_w / src_h;

    let target_h = 360u32;
    let mut decoder = VideoDecoder::open(&path)
        .output_height(target_h)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(frame.height(), target_h, "Height should match target");
    assert_eq!(frame.width() % 2, 0, "Width must be even");
    let dst_ratio = frame.width() as f64 / frame.height() as f64;
    let tolerance = 1.0 / frame.width() as f64;
    assert!(
        (dst_ratio - src_ratio).abs() < tolerance + 0.02,
        "Aspect ratio should be preserved: src={src_ratio:.3} dst={dst_ratio:.3}"
    );
}

// ============================================================================
// Last-setter-wins
// ============================================================================

#[test]
fn output_size_after_output_width_should_win() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_width(9999) // set first; should be overridden
        .output_size(160, 90)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(frame.width(), 160);
    assert_eq!(frame.height(), 90);
}

// ============================================================================
// Validation — zero dimensions rejected before file open
// ============================================================================

#[test]
fn output_size_zero_width_should_return_invalid_dimensions_error() {
    // Use a non-existent path: the error must occur before the file is opened.
    let result = VideoDecoder::open("nonexistent_file.mp4")
        .output_size(0, 240)
        .build();

    assert!(
        matches!(result, Err(DecodeError::InvalidOutputDimensions { .. })),
        "Expected InvalidOutputDimensions"
    );
}

#[test]
fn output_size_zero_height_should_return_invalid_dimensions_error() {
    let result = VideoDecoder::open("nonexistent_file.mp4")
        .output_size(320, 0)
        .build();

    assert!(
        matches!(result, Err(DecodeError::InvalidOutputDimensions { .. })),
        "Expected InvalidOutputDimensions"
    );
}

#[test]
fn output_width_zero_should_return_invalid_dimensions_error() {
    let result = VideoDecoder::open("nonexistent_file.mp4")
        .output_width(0)
        .build();

    assert!(
        matches!(result, Err(DecodeError::InvalidOutputDimensions { .. })),
        "Expected InvalidOutputDimensions"
    );
}

#[test]
fn output_height_zero_should_return_invalid_dimensions_error() {
    let result = VideoDecoder::open("nonexistent_file.mp4")
        .output_height(0)
        .build();

    assert!(
        matches!(result, Err(DecodeError::InvalidOutputDimensions { .. })),
        "Expected InvalidOutputDimensions"
    );
}

// ============================================================================
// Multiple frames — sws_ctx caching does not break sequential decoding
// ============================================================================

#[test]
fn output_size_should_produce_correct_dimensions_across_multiple_frames() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_size(320, 240)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    for i in 0..5 {
        let frame = decoder
            .decode_one()
            .unwrap_or_else(|e| panic!("Frame {i} decode error: {e}"))
            .unwrap_or_else(|| panic!("Frame {i} should exist"));

        assert_eq!(frame.width(), 320, "Frame {i}: width should be 320");
        assert_eq!(frame.height(), 240, "Frame {i}: height should be 240");
    }
}
