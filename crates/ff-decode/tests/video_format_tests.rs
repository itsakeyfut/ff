//! Pixel format conversion tests for VideoDecoder.
//!
//! Tests various output pixel format conversions (RGBA, RGB24, YUV420P, etc.)
//! to ensure the decoder properly converts between formats.

mod fixtures;
use fixtures::*;

use ff_decode::{HardwareAccel, VideoDecoder};
use ff_format::PixelFormat;

// ============================================================================
// Pixel Format Conversion Tests
// ============================================================================

#[test]
fn test_decode_with_rgba_output() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Rgba)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(
        frame.format(),
        PixelFormat::Rgba,
        "Output format should be RGBA"
    );

    // RGBA should have 1 plane
    assert_eq!(frame.planes().len(), 1, "RGBA should have 1 plane");
}

#[test]
fn test_decode_with_rgb24_output() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Rgb24)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(
        frame.format(),
        PixelFormat::Rgb24,
        "Output format should be RGB24"
    );

    // RGB24 should have 1 plane
    assert_eq!(frame.planes().len(), 1, "RGB24 should have 1 plane");
}

#[test]
fn test_decode_with_yuv420p_output() {
    let path = test_video_path();
    let mut decoder = VideoDecoder::open(&path)
        .output_format(PixelFormat::Yuv420p)
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder");

    let frame = decoder
        .decode_one()
        .expect("Failed to decode")
        .expect("First frame should exist");

    assert_eq!(
        frame.format(),
        PixelFormat::Yuv420p,
        "Output format should be YUV420P"
    );

    // YUV420P should have 3 planes
    assert_eq!(frame.planes().len(), 3, "YUV420P should have 3 planes");
}
