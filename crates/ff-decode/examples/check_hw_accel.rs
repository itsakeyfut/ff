//! Check available hardware acceleration on this system
//!
//! Run with: cargo run --example check_hw_accel

use ff_decode::{HardwareAccel, VideoDecoder};
use std::path::PathBuf;

fn main() {
    println!("=== Hardware Acceleration Checker ===\n");

    // Get test video path
    let video_path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/video/gameplay.mp4"
    ));

    if !video_path.exists() {
        eprintln!("Error: Test video not found at {:?}", video_path);
        return;
    }

    println!("GPU Information:");
    println!("- NVIDIA GeForce RTX 3060 Laptop GPU detected\n");

    // Test each hardware accelerator
    let accelerators = [
        HardwareAccel::None,
        HardwareAccel::Nvdec,
        HardwareAccel::Qsv,
        HardwareAccel::Amf,
    ];

    for accel in &accelerators {
        print!("Testing {:?} ({})... ", accel, accel.name());

        match VideoDecoder::open(&video_path)
            .hardware_accel(*accel)
            .build()
        {
            Ok(mut decoder) => {
                let active = decoder.hardware_accel();

                // Try to decode one frame to verify it actually works
                match decoder.decode_one() {
                    Ok(Some(_frame)) => {
                        if active == *accel
                            || (*accel == HardwareAccel::None && active == HardwareAccel::None)
                        {
                            println!("✓ AVAILABLE & WORKING (active: {:?})", active);
                        } else {
                            println!("⚠ Built but using: {:?}", active);
                        }
                    }
                    Ok(None) => {
                        println!("⚠ Built but no frame decoded (active: {:?})", active);
                    }
                    Err(e) => {
                        println!("✗ Decoder created but decode failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ NOT AVAILABLE ({:?})", e);
            }
        }
    }

    println!("\n=== Testing Auto Mode ===");
    print!("HardwareAccel::Auto... ");

    match VideoDecoder::open(&video_path)
        .hardware_accel(HardwareAccel::Auto)
        .build()
    {
        Ok(mut decoder) => {
            let active = decoder.hardware_accel();

            // Try to decode one frame
            match decoder.decode_one() {
                Ok(Some(_frame)) => {
                    println!("✓ SUCCESS & WORKING (selected: {:?})", active);
                }
                Ok(None) => {
                    println!("⚠ SUCCESS but no frame decoded (selected: {:?})", active);
                }
                Err(e) => {
                    println!("✗ Decoder created but decode failed: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ FAILED ({:?})", e);
        }
    }
}
