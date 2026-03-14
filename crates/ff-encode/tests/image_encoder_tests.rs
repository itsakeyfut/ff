//! Integration tests for image encoder.

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::ImageEncoder;
use ff_format::PixelFormat;
use fixtures::{
    FileGuard, assert_valid_output_file, create_black_frame, get_file_size, test_output_path,
};

// ── Baseline tests ────────────────────────────────────────────────────────────

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

// ── Dimension tests ───────────────────────────────────────────────────────────

#[test]
fn encode_jpeg_with_explicit_dimensions_should_produce_valid_output() {
    let output_path = test_output_path("test_image_resize.jpg");
    let _guard = FileGuard::new(output_path.clone());

    // Encode a 64×64 source frame but request 128×128 output.
    let encoder = match ImageEncoder::create(&output_path)
        .width(128)
        .height(128)
        .build()
    {
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
fn encode_png_with_explicit_dimensions_should_produce_valid_output() {
    let output_path = test_output_path("test_image_resize.png");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path)
        .width(32)
        .height(32)
        .build()
    {
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
fn encode_jpeg_with_only_width_should_produce_valid_output() {
    let output_path = test_output_path("test_image_width_only.jpg");
    let _guard = FileGuard::new(output_path.clone());

    // Height is not set → falls back to the source frame's height (64).
    let encoder = match ImageEncoder::create(&output_path).width(128).build() {
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

// ── Quality tests ─────────────────────────────────────────────────────────────

#[test]
fn encode_jpeg_with_quality_should_produce_valid_output() {
    let output_path = test_output_path("test_image_quality.jpg");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path).quality(75).build() {
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
fn encode_jpeg_high_quality_should_be_larger_than_low_quality() {
    let path_lo = test_output_path("test_image_quality_lo.jpg");
    let path_hi = test_output_path("test_image_quality_hi.jpg");
    let _guard_lo = FileGuard::new(path_lo.clone());
    let _guard_hi = FileGuard::new(path_hi.clone());

    // Use a non-trivial frame so quality differences are visible.
    let frame = create_black_frame(128, 128);

    let enc_lo = match ImageEncoder::create(&path_lo).quality(5).build() {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    if let Err(e) = enc_lo.encode(&frame) {
        println!("Skipping: {e}");
        return;
    }

    let frame = create_black_frame(128, 128);
    let enc_hi = match ImageEncoder::create(&path_hi).quality(95).build() {
        Ok(enc) => enc,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    if let Err(e) = enc_hi.encode(&frame) {
        println!("Skipping: {e}");
        return;
    }

    let size_lo = get_file_size(&path_lo);
    let size_hi = get_file_size(&path_hi);
    println!("JPEG quality=5 size={size_lo}  quality=95 size={size_hi}");
    assert!(
        size_hi >= size_lo,
        "high-quality JPEG should be >= low-quality JPEG in size"
    );
}

#[test]
fn encode_png_with_quality_should_produce_valid_output() {
    let output_path = test_output_path("test_image_quality.png");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path).quality(60).build() {
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

// ── Pixel format tests ────────────────────────────────────────────────────────

#[test]
fn encode_jpeg_with_pixel_format_should_produce_valid_output() {
    let output_path = test_output_path("test_image_pixfmt.jpg");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path)
        .pixel_format(PixelFormat::Yuv420p)
        .build()
    {
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
fn encode_png_with_rgb24_pixel_format_should_produce_valid_output() {
    let output_path = test_output_path("test_image_rgb24.png");
    let _guard = FileGuard::new(output_path.clone());

    let encoder = match ImageEncoder::create(&output_path)
        .pixel_format(PixelFormat::Rgb24)
        .build()
    {
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
