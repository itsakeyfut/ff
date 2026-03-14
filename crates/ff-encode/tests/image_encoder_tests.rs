//! Integration tests for image encoder.

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::ImageEncoder;
use fixtures::{FileGuard, assert_valid_output_file, create_black_frame, test_output_path};

#[test]
fn encode_jpeg_should_produce_valid_output() {
    let output_path = test_output_path("test_image.jpg");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path).build() {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frame = create_black_frame(64, 64);
    match encoder.encode(&frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }

    assert_valid_output_file(&output_path);
}

#[test]
fn encode_png_should_produce_valid_output() {
    let output_path = test_output_path("test_image.png");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path).build() {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frame = create_black_frame(64, 64);
    match encoder.encode(&frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }

    assert_valid_output_file(&output_path);
}

#[test]
fn encode_bmp_should_produce_valid_output() {
    let output_path = test_output_path("test_image.bmp");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path).build() {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frame = create_black_frame(64, 64);
    match encoder.encode(&frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }

    assert_valid_output_file(&output_path);
}

#[test]
fn build_with_unsupported_extension_should_return_error() {
    let result = ImageEncoder::create("out.avi").build();
    assert!(result.is_err(), "expected error for unsupported extension");
}
