//! Hardware acceleration tests for VideoDecoder.
//!
//! Tests various hardware acceleration options (Auto, None, NVDEC, QSV, etc.)
//! and verifies fallback behavior when hardware is unavailable.

mod fixtures;
use fixtures::*;

use ff_decode::{HardwareAccel, VideoDecoder};

// ============================================================================
// Hardware Acceleration Tests
// ============================================================================

#[test]
fn test_hardware_accel_none_explicitly() {
    // Test that explicitly disabling hardware acceleration works
    let decoder = VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder with HardwareAccel::None");

    assert_eq!(
        decoder.hardware_accel(),
        HardwareAccel::None,
        "Hardware acceleration should be None when explicitly disabled"
    );

    // Verify decoder still works
    let mut decoder = decoder;
    let frame = decoder.decode_one().expect("decode_one failed");
    assert!(frame.is_some(), "Should decode a frame");
}

#[test]
fn test_hardware_accel_auto_fallback_to_software() {
    // Test that Auto mode falls back to software when no hardware is available
    // On systems without GPU support, this should work and fall back to None
    let decoder = VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::Auto)
        .build()
        .expect("Failed to create decoder with HardwareAccel::Auto");

    // Auto mode should either select a hardware accelerator or fall back to None
    let active_accel = decoder.hardware_accel();
    assert!(
        matches!(
            active_accel,
            HardwareAccel::None
                | HardwareAccel::Nvdec
                | HardwareAccel::Qsv
                | HardwareAccel::VideoToolbox
                | HardwareAccel::Vaapi
                | HardwareAccel::Amf
        ),
        "Auto mode should select a valid hardware accelerator or fall back to None, got {:?}",
        active_accel
    );

    // Verify decoder still works
    let mut decoder = decoder;
    let frame = decoder.decode_one().expect("decode_one failed");
    assert!(frame.is_some(), "Should decode a frame");
}

#[test]
#[ignore] // Ignored because this test may crash on systems without proper hardware support
fn test_hardware_accel_specific_unavailable() {
    // Test that requesting a specific unavailable hardware accelerator returns an error
    // Note: This test might fail/crash on systems that don't have proper FFmpeg hardware support
    // We test with a less common accelerator to reduce false positives

    // Try a specific hardware accelerator that's unlikely to be available on most systems
    // If it is available, the test will pass anyway (decoder creation succeeds)
    let result = VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::Amf) // AMD AMF - less common than NVDEC/QSV
        .build();

    match result {
        Ok(decoder) => {
            // Hardware accelerator was available - verify it's being used
            assert_eq!(
                decoder.hardware_accel(),
                HardwareAccel::Amf,
                "When AMF is available, it should be active"
            );

            // Verify decoder works
            let mut decoder = decoder;
            let frame = decoder.decode_one().expect("decode_one failed");
            assert!(frame.is_some(), "Should decode a frame");
        }
        Err(err) => {
            // Hardware accelerator not available - this is expected on most systems
            assert!(
                matches!(err, ff_decode::DecodeError::HwAccelUnavailable { .. }),
                "Error should be HwAccelUnavailable when specific accelerator is not available, got: {:?}",
                err
            );
        }
    }
}

#[test]
fn test_hardware_accel_builder_chain() {
    // Test that hardware_accel can be chained with other builder methods
    let result = VideoDecoder::open(&test_video_path())
        .output_format(ff_format::PixelFormat::Rgba)
        .hardware_accel(HardwareAccel::None)
        .thread_count(4)
        .build();

    assert!(
        result.is_ok(),
        "Builder chaining with hardware_accel should work"
    );

    let decoder = result.unwrap();
    assert_eq!(
        decoder.hardware_accel(),
        HardwareAccel::None,
        "Hardware acceleration should be None"
    );
}

#[test]
fn test_hardware_accel_does_not_affect_software_decoding() {
    // Test that HardwareAccel::None behaves identically to software decoding
    let mut decoder_none = VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder with HardwareAccel::None");

    let mut decoder_auto = VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::Auto)
        .build()
        .expect("Failed to create decoder with HardwareAccel::Auto");

    // Decode a frame from each
    let frame_none = decoder_none
        .decode_one()
        .expect("decode_one failed for None")
        .expect("Should have a frame");

    let frame_auto = decoder_auto
        .decode_one()
        .expect("decode_one failed for Auto")
        .expect("Should have a frame");

    // Both should decode successfully and have same dimensions
    assert_eq!(
        frame_none.width(),
        frame_auto.width(),
        "Frames should have same width"
    );
    assert_eq!(
        frame_none.height(),
        frame_auto.height(),
        "Frames should have same height"
    );
}

#[test]
fn test_hardware_accel_enum_name() {
    // Test that HardwareAccel enum has correct names
    assert_eq!(HardwareAccel::Auto.name(), "auto");
    assert_eq!(HardwareAccel::None.name(), "none");
    assert_eq!(HardwareAccel::Nvdec.name(), "nvdec");
    assert_eq!(HardwareAccel::Qsv.name(), "qsv");
    assert_eq!(HardwareAccel::Amf.name(), "amf");
    assert_eq!(HardwareAccel::VideoToolbox.name(), "videotoolbox");
    assert_eq!(HardwareAccel::Vaapi.name(), "vaapi");
}

#[test]
fn test_hardware_accel_is_specific() {
    // Test HardwareAccel::is_specific method
    assert!(!HardwareAccel::Auto.is_specific());
    assert!(!HardwareAccel::None.is_specific());
    assert!(HardwareAccel::Nvdec.is_specific());
    assert!(HardwareAccel::Qsv.is_specific());
    assert!(HardwareAccel::Amf.is_specific());
    assert!(HardwareAccel::VideoToolbox.is_specific());
    assert!(HardwareAccel::Vaapi.is_specific());
}
