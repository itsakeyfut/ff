//! Codec selection and fallback tests.
//!
//! Tests the automatic codec selection and fallback behavior:
//! - H.264 codec selection (hardware → software fallback → VP9)
//! - H.265 codec selection (hardware → software fallback → AV1)
//! - LGPL compliance verification
//! - Hardware encoder preference

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::{HardwareEncoder, VideoCodec, VideoEncoder};
use fixtures::{FileGuard, assert_valid_output_file, create_black_frame, test_output_path};

// ============================================================================
// H.264 Codec Selection Tests
// ============================================================================

#[test]
fn test_h264_codec_fallback() {
    let output_path = test_output_path("test_h264_fallback.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Request H.264, let the encoder choose the best available
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(1280, 720, 30.0)
        .video_codec(VideoCodec::H264)
        .build();

    match result {
        Ok(encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("H.264 selected codec: {}", actual_codec);

            // Should be one of: h264_nvenc, h264_qsv, h264_amf, h264_videotoolbox,
            // h264_vaapi, libx264 (if GPL), or libvpx-vp9 (fallback)
            assert!(
                actual_codec.contains("h264")
                    || actual_codec.contains("nvenc")
                    || actual_codec.contains("qsv")
                    || actual_codec.contains("amf")
                    || actual_codec.contains("videotoolbox")
                    || actual_codec.contains("vaapi")
                    || actual_codec.contains("x264")
                    || actual_codec.contains("vp9"),
                "Unexpected codec: {}",
                actual_codec
            );
        }
        Err(e) => {
            println!("H.264 encoder creation failed: {}", e);
        }
    }
}

#[test]
fn test_h264_hardware_preference() {
    let output_path = test_output_path("test_h264_hw.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Request H.264 with explicit hardware encoder preference
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(1280, 720, 30.0)
        .video_codec(VideoCodec::H264)
        .hardware_encoder(HardwareEncoder::Auto)
        .build();

    match result {
        Ok(encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("H.264 with HW auto selected: {}", actual_codec);

            // Check if hardware encoder was selected
            if encoder.is_hardware_encoding() {
                println!("✓ Hardware encoder is being used");
                assert!(
                    actual_codec.contains("nvenc")
                        || actual_codec.contains("qsv")
                        || actual_codec.contains("amf")
                        || actual_codec.contains("videotoolbox")
                        || actual_codec.contains("vaapi"),
                    "Should use hardware encoder, got: {}",
                    actual_codec
                );
            } else {
                println!("⚠ Hardware encoder not available, using software fallback");
            }
        }
        Err(e) => {
            println!("H.264 hardware encoder creation failed: {}", e);
        }
    }
}

#[test]
fn test_h264_software_only() {
    let output_path = test_output_path("test_h264_sw.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Request H.264 with explicit software-only encoding
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(1280, 720, 30.0)
        .video_codec(VideoCodec::H264)
        .hardware_encoder(HardwareEncoder::None)
        .build();

    match result {
        Ok(encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("H.264 software-only selected: {}", actual_codec);

            // Should not be a hardware encoder
            assert!(
                !encoder.is_hardware_encoding(),
                "Should use software encoder"
            );

            // Should be libx264 (if GPL enabled) or VP9 fallback
            #[cfg(feature = "gpl")]
            {
                assert!(
                    actual_codec.contains("x264") || actual_codec.contains("vp9"),
                    "Expected libx264 or VP9, got: {}",
                    actual_codec
                );
            }

            #[cfg(not(feature = "gpl"))]
            {
                // Without GPL, should fallback to VP9
                assert!(
                    actual_codec.contains("vp9"),
                    "Expected VP9 fallback, got: {}",
                    actual_codec
                );
            }
        }
        Err(e) => {
            println!("H.264 software encoder creation failed: {}", e);
        }
    }
}

// ============================================================================
// H.265 Codec Selection Tests
// ============================================================================

#[test]
fn test_h265_codec_fallback() {
    let output_path = test_output_path("test_h265_fallback.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Request H.265, let the encoder choose the best available
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(1280, 720, 30.0)
        .video_codec(VideoCodec::H265)
        .build();

    match result {
        Ok(encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("H.265 selected codec: {}", actual_codec);

            // Should be one of: hevc_nvenc, hevc_qsv, hevc_amf, hevc_videotoolbox,
            // hevc_vaapi, libx265 (if GPL), or libaom-av1 (fallback)
            assert!(
                actual_codec.contains("hevc")
                    || actual_codec.contains("h265")
                    || actual_codec.contains("x265")
                    || actual_codec.contains("av1")
                    || actual_codec.contains("aom"),
                "Unexpected codec: {}",
                actual_codec
            );
        }
        Err(e) => {
            println!("H.265 encoder creation failed: {}", e);
        }
    }
}

// ============================================================================
// LGPL Compliance Tests
// ============================================================================

#[test]
fn test_lgpl_compliance_without_gpl_feature() {
    #[cfg(not(feature = "gpl"))]
    {
        let output_path = test_output_path("test_lgpl_compliance.mp4");
        let _guard = FileGuard::new(output_path.clone());

        // Request H.264 without GPL feature enabled
        let result = VideoEncoder::create(&output_path)
            .expect("Failed to create encoder builder")
            .video(1280, 720, 30.0)
            .video_codec(VideoCodec::H264)
            .hardware_encoder(HardwareEncoder::None) // Force software encoding
            .build();

        match result {
            Ok(encoder) => {
                let actual_codec = encoder.actual_video_codec();
                println!("LGPL-compliant codec selected: {}", actual_codec);

                // Without GPL feature, should always be LGPL-compliant
                assert!(
                    encoder.is_lgpl_compliant(),
                    "Encoder should be LGPL-compliant, got: {}",
                    actual_codec
                );

                // Should have fallen back to VP9
                assert!(
                    actual_codec.contains("vp9"),
                    "Should fallback to VP9 without GPL, got: {}",
                    actual_codec
                );
            }
            Err(e) => {
                println!("LGPL-compliant encoder creation failed: {}", e);
            }
        }
    }

    #[cfg(feature = "gpl")]
    {
        println!("Skipping LGPL test (GPL feature is enabled)");
    }
}

#[test]
fn test_lgpl_compliance_with_hardware() {
    let output_path = test_output_path("test_lgpl_hw.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Request H.264 with hardware encoder (should be LGPL-compliant)
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(1280, 720, 30.0)
        .video_codec(VideoCodec::H264)
        .hardware_encoder(HardwareEncoder::Auto)
        .build();

    match result {
        Ok(encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("Hardware codec selected: {}", actual_codec);

            // Hardware encoders are always LGPL-compliant
            if encoder.is_hardware_encoding() {
                assert!(
                    encoder.is_lgpl_compliant(),
                    "Hardware encoder should be LGPL-compliant: {}",
                    actual_codec
                );
                println!("✓ Hardware encoder is LGPL-compliant");
            }
        }
        Err(e) => {
            println!("Hardware encoder creation failed: {}", e);
        }
    }
}

// ============================================================================
// Specific Codec Tests
// ============================================================================

#[test]
fn test_vp9_codec() {
    let output_path = test_output_path("test_vp9_codec.webm");
    let _guard = FileGuard::new(output_path.clone());

    // Request VP9 explicitly
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Vp9)
        .build();

    match result {
        Ok(mut encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("VP9 codec: {}", actual_codec);

            assert!(
                actual_codec.contains("vp9"),
                "Expected VP9 codec, got: {}",
                actual_codec
            );

            // VP9 should always be LGPL-compliant
            assert!(encoder.is_lgpl_compliant(), "VP9 should be LGPL-compliant");

            // Encode a few frames to verify it works
            for _ in 0..10 {
                let frame = create_black_frame(640, 480);
                encoder.push_video(&frame).expect("Failed to push frame");
            }

            encoder.finish().expect("Failed to finish encoding");
            assert_valid_output_file(&output_path);
        }
        Err(e) => {
            println!("VP9 encoder creation failed: {}", e);
        }
    }
}

#[test]
fn test_av1_codec() {
    let output_path = test_output_path("test_av1_codec.webm");
    let _guard = FileGuard::new(output_path.clone());

    // Request AV1 explicitly
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Av1)
        .build();

    match result {
        Ok(encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("AV1 codec: {}", actual_codec);

            assert!(
                actual_codec.contains("av1") || actual_codec.contains("aom"),
                "Expected AV1 codec, got: {}",
                actual_codec
            );

            // AV1 should always be LGPL-compliant
            assert!(encoder.is_lgpl_compliant(), "AV1 should be LGPL-compliant");
        }
        Err(e) => {
            println!("AV1 encoder not available: {}", e);
        }
    }
}

#[test]
fn test_mpeg4_codec() {
    let output_path = test_output_path("test_mpeg4_codec.mp4");
    let _guard = FileGuard::new(output_path.clone());

    // Request MPEG-4 explicitly (should always be available)
    let result = VideoEncoder::create(&output_path)
        .expect("Failed to create encoder builder")
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .build();

    match result {
        Ok(mut encoder) => {
            let actual_codec = encoder.actual_video_codec();
            println!("MPEG-4 codec: {}", actual_codec);

            assert!(
                actual_codec.contains("mpeg4"),
                "Expected MPEG-4 codec, got: {}",
                actual_codec
            );

            // MPEG-4 should be LGPL-compliant
            assert!(
                encoder.is_lgpl_compliant(),
                "MPEG-4 should be LGPL-compliant"
            );

            // Encode a few frames to verify it works
            for _ in 0..10 {
                let frame = create_black_frame(640, 480);
                encoder.push_video(&frame).expect("Failed to push frame");
            }

            encoder.finish().expect("Failed to finish encoding");
            assert_valid_output_file(&output_path);
        }
        Err(e) => {
            panic!("MPEG-4 encoder should always be available, got: {}", e);
        }
    }
}
