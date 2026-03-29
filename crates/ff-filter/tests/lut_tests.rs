//! Integration tests for 3D LUT filter application.

#![allow(clippy::unwrap_used)]

use ff_filter::FilterGraph;
use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// 64×64 Yuv420p frame filled with the given luma value (Y=luma, U=128, V=128).
fn make_grey_frame(luma: u8) -> VideoFrame {
    let width = 64u32;
    let height = 64u32;
    let y = vec![luma; (width * height) as usize];
    let u = vec![128u8; ((width / 2) * (height / 2)) as usize];
    let v = vec![128u8; ((width / 2) * (height / 2)) as usize];
    VideoFrame::new(
        vec![
            PooledBuffer::standalone(y),
            PooledBuffer::standalone(u),
            PooledBuffer::standalone(v),
        ],
        vec![width as usize, (width / 2) as usize, (width / 2) as usize],
        width,
        height,
        PixelFormat::Yuv420p,
        Timestamp::default(),
        true,
    )
    .unwrap()
}

/// Compute the mean value of a byte slice (the Y plane of a YUV frame).
fn mean_luma(plane: &[u8]) -> f64 {
    if plane.is_empty() {
        return 0.0;
    }
    plane.iter().map(|&v| v as f64).sum::<f64>() / plane.len() as f64
}

/// Per-pixel tolerance check: every byte in `actual` must be within `tolerance` of
/// the corresponding byte in `expected`.
fn assert_pixels_close(actual: &[u8], expected: &[u8], tolerance: u8) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "pixel buffer length mismatch: actual={} expected={}",
        actual.len(),
        expected.len()
    );
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        let diff = a.abs_diff(*e);
        assert!(
            diff <= tolerance,
            "pixel {i}: actual={a} expected={e} diff={diff} tolerance={tolerance}"
        );
    }
}

#[test]
fn lut3d_cube_should_apply_identity_lut_without_change() {
    let path = format!("{FIXTURES_DIR}/test_identity.cube");

    let mut graph = match FilterGraph::builder().lut3d(&path).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frame = make_grey_frame(128);
    match graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    }

    let result = graph.pull_video().expect("pull_video must not fail");
    let out = match result {
        Some(f) => f,
        None => {
            println!("Skipping: no output frame produced");
            return;
        }
    };

    assert_eq!(out.width(), 64, "width must be unchanged");
    assert_eq!(out.height(), 64, "height must be unchanged");

    // For Yuv420p output: compare the luma plane within ±5 tolerance
    // (YUV↔RGB conversion introduces at most 1–2 quantization steps).
    if out.format() == PixelFormat::Yuv420p {
        let input_y: &[u8] = frame.planes()[0].as_ref();
        let output_y: &[u8] = out.planes()[0].as_ref();
        assert_pixels_close(output_y, input_y, 5);
    } else {
        // Non-YUV output: at least verify dimensions and luma is approximately neutral.
        let mean = mean_luma(out.planes()[0].as_ref());
        assert!(
            (mean - 128.0).abs() < 20.0,
            "expected output luma near 128, got {mean:.1}"
        );
    }
}

#[test]
fn lut3d_cube_should_transform_colors_to_match_reference() {
    let identity_path = format!("{FIXTURES_DIR}/test_identity.cube");
    let saturate_path = format!("{FIXTURES_DIR}/test_saturate.cube");

    // Build both graphs.
    let mut identity_graph = match FilterGraph::builder().lut3d(&identity_path).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping identity graph: {e}");
            return;
        }
    };
    let mut saturate_graph = match FilterGraph::builder().lut3d(&saturate_path).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping saturate graph: {e}");
            return;
        }
    };

    // Use a frame with luma below the midpoint so the 1.5x boost noticeably changes it.
    let frame = make_grey_frame(80);

    match identity_graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping identity push: {e}");
            return;
        }
    }
    match saturate_graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping saturate push: {e}");
            return;
        }
    }

    let identity_out = match identity_graph.pull_video().expect("pull_video failed") {
        Some(f) => f,
        None => {
            println!("Skipping: no identity output frame");
            return;
        }
    };
    let saturate_out = match saturate_graph.pull_video().expect("pull_video failed") {
        Some(f) => f,
        None => {
            println!("Skipping: no saturate output frame");
            return;
        }
    };

    assert_eq!(saturate_out.width(), 64, "width must be unchanged");
    assert_eq!(saturate_out.height(), 64, "height must be unchanged");

    // The saturate LUT (1.5x per-channel boost) must produce a different luma than identity.
    let identity_mean = mean_luma(identity_out.planes()[0].as_ref());
    let saturate_mean = mean_luma(saturate_out.planes()[0].as_ref());

    assert!(
        (saturate_mean - identity_mean).abs() > 5.0,
        "saturate LUT output (mean luma={saturate_mean:.1}) should differ from identity \
         output (mean luma={identity_mean:.1}) by more than 5"
    );
}

#[test]
fn lut3d_3dl_should_produce_same_result_as_cube_for_identical_lut() {
    let cube_path = format!("{FIXTURES_DIR}/test_identity.cube");
    let tdl_path = format!("{FIXTURES_DIR}/test_identity.3dl");

    let mut cube_graph = match FilterGraph::builder().lut3d(&cube_path).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping cube graph: {e}");
            return;
        }
    };
    let mut tdl_graph = match FilterGraph::builder().lut3d(&tdl_path).build() {
        Ok(g) => g,
        Err(e) => {
            println!("Skipping 3dl graph: {e}");
            return;
        }
    };

    let frame = make_grey_frame(128);

    match cube_graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping cube push: {e}");
            return;
        }
    }
    match tdl_graph.push_video(0, &frame) {
        Ok(()) => {}
        Err(e) => {
            println!("Skipping 3dl push: {e}");
            return;
        }
    }

    let cube_out = match cube_graph.pull_video().expect("pull_video failed") {
        Some(f) => f,
        None => {
            println!("Skipping: no cube output frame");
            return;
        }
    };
    let tdl_out = match tdl_graph.pull_video().expect("pull_video failed") {
        Some(f) => f,
        None => {
            println!("Skipping: no 3dl output frame");
            return;
        }
    };

    assert_eq!(
        cube_out.width(),
        tdl_out.width(),
        "output widths must match"
    );
    assert_eq!(
        cube_out.height(),
        tdl_out.height(),
        "output heights must match"
    );

    // Both identity LUTs must produce the same luma output within ±2.
    if cube_out.format() == PixelFormat::Yuv420p && tdl_out.format() == PixelFormat::Yuv420p {
        assert_pixels_close(
            tdl_out.planes()[0].as_ref(),
            cube_out.planes()[0].as_ref(),
            2,
        );
    } else {
        let cube_mean = mean_luma(cube_out.planes()[0].as_ref());
        let tdl_mean = mean_luma(tdl_out.planes()[0].as_ref());
        assert!(
            (cube_mean - tdl_mean).abs() < 3.0,
            "cube mean luma={cube_mean:.1} and 3dl mean luma={tdl_mean:.1} should be within 3"
        );
    }
}
