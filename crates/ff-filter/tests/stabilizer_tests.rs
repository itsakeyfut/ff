//! Integration tests for `Stabilizer::analyze` (pass 1) and `Stabilizer::transform` (pass 2).

mod fixtures;

use std::path::PathBuf;

use ff_filter::{AnalyzeOptions, FilterError, Interpolation, StabilizeOptions, Stabilizer};
use fixtures::{FileGuard, make_source_file, test_output_path};

/// Verifies that `Stabilizer::analyze` produces a non-empty `.trf` file when
/// run against a valid synthetic video clip.
///
/// Acceptance criterion for issue #392.
#[test]
fn analyze_should_produce_nonempty_trf_file() {
    const W: u32 = 64;
    const H: u32 = 64;
    const FPS: f64 = 30.0;
    const FRAMES: usize = 15;

    let src_path = test_output_path("vidstab_src.mp4");
    let trf_path = test_output_path("vidstab_out.trf");

    let _src_guard = FileGuard::new(src_path.clone());
    let _trf_guard = FileGuard::new(trf_path.clone());

    if make_source_file(&src_path, W, H, FPS, FRAMES, 128, 128, 128).is_none() {
        println!("Skipping: source encoder unavailable");
        return;
    }

    let result = Stabilizer::analyze(&src_path, &trf_path, &AnalyzeOptions::default());

    match result {
        Err(FilterError::Ffmpeg { ref message, .. })
            if message.contains("not available in this FFmpeg build") =>
        {
            println!("Skipping: vidstabdetect not available: {message}");
            return;
        }
        Err(e) => panic!("analyze failed unexpectedly: {e}"),
        Ok(()) => {}
    }

    assert!(
        trf_path.exists(),
        ".trf file should exist after analysis: {trf_path:?}"
    );
    let size = std::fs::metadata(&trf_path)
        .expect("metadata read failed")
        .len();
    assert!(size > 0, ".trf file should be non-empty (got {size} bytes)");
}

/// Verifies that `Stabilizer::analyze` returns `Err(FilterError::Ffmpeg { .. })`
/// when the input file does not exist.
///
/// Acceptance criterion for issue #392.
#[test]
fn analyze_nonexistent_input_should_return_ffmpeg_error() {
    let trf_path = test_output_path("vidstab_nonexistent.trf");
    let _trf_guard = FileGuard::new(trf_path.clone());

    let result = Stabilizer::analyze(
        &PathBuf::from("no_such_file_99999.mp4"),
        &trf_path,
        &AnalyzeOptions::default(),
    );

    match result {
        Err(FilterError::Ffmpeg { ref message, .. })
            if message.contains("not available in this FFmpeg build") =>
        {
            println!("Skipping: vidstabdetect not available: {message}");
        }
        Err(FilterError::Ffmpeg { .. }) => {
            // Expected: FFmpeg reported an error opening the non-existent file.
        }
        Err(e) => panic!("expected FilterError::Ffmpeg, got {e:?}"),
        Ok(()) => panic!("expected error for non-existent input, got Ok(())"),
    }
}

// ── Pass 2 — transform tests ──────────────────────────────────────────────────

/// Verifies that `Stabilizer::transform` produces a non-empty output file when
/// run through both passes against a valid synthetic video clip.
///
/// Acceptance criterion for issue #393.
#[test]
fn transform_should_produce_valid_output_file() {
    const W: u32 = 64;
    const H: u32 = 64;
    const FPS: f64 = 30.0;
    const FRAMES: usize = 15;

    let src_path = test_output_path("vstab_t_src.mp4");
    let trf_path = test_output_path("vstab_t_out.trf");
    let out_path = test_output_path("vstab_t_output.mp4");

    let _src_guard = FileGuard::new(src_path.clone());
    let _trf_guard = FileGuard::new(trf_path.clone());
    let _out_guard = FileGuard::new(out_path.clone());

    if make_source_file(&src_path, W, H, FPS, FRAMES, 128, 128, 128).is_none() {
        println!("Skipping: source encoder unavailable");
        return;
    }

    // Pass 1: analyze
    match Stabilizer::analyze(&src_path, &trf_path, &AnalyzeOptions::default()) {
        Err(FilterError::Ffmpeg { ref message, .. })
            if message.contains("not available in this FFmpeg build") =>
        {
            println!("Skipping: vidstabdetect not available: {message}");
            return;
        }
        Err(e) => panic!("analyze failed unexpectedly: {e}"),
        Ok(()) => {}
    }

    // Pass 2: transform
    let result = Stabilizer::transform(
        &src_path,
        &trf_path,
        &out_path,
        &StabilizeOptions::default(),
    );

    match result {
        Err(FilterError::Ffmpeg { ref message, .. })
            if message.contains("not available in this FFmpeg build") =>
        {
            println!("Skipping: vidstabtransform not available: {message}");
            return;
        }
        Err(FilterError::Ffmpeg { ref message, .. })
            if message.contains("no H.264 encoder available") =>
        {
            println!("Skipping: no H.264 encoder available: {message}");
            return;
        }
        Err(e) => panic!("transform failed unexpectedly: {e}"),
        Ok(()) => {}
    }

    assert!(
        out_path.exists(),
        "output file should exist after transform: {out_path:?}"
    );
    let size = std::fs::metadata(&out_path)
        .expect("metadata read failed")
        .len();
    assert!(
        size > 0,
        "output file should be non-empty (got {size} bytes)"
    );
}

/// Verifies that `Stabilizer::transform` returns `Err(FilterError::Ffmpeg { .. })`
/// when the `.trf` file does not exist.
///
/// Acceptance criterion for issue #393.
#[test]
fn transform_nonexistent_trf_should_return_ffmpeg_error() {
    let src_path = test_output_path("vstab_t_err_src.mp4");
    let out_path = test_output_path("vstab_t_err_out.mp4");

    let _src_guard = FileGuard::new(src_path.clone());
    let _out_guard = FileGuard::new(out_path.clone());

    if make_source_file(&src_path, 64, 64, 30.0, 5, 128, 128, 128).is_none() {
        println!("Skipping: source encoder unavailable");
        return;
    }

    let result = Stabilizer::transform(
        &src_path,
        &PathBuf::from("no_such_trf_99999.trf"),
        &out_path,
        &StabilizeOptions::default(),
    );

    match result {
        Err(FilterError::Ffmpeg { ref message, .. })
            if message.contains("not available in this FFmpeg build") =>
        {
            println!("Skipping: vidstabtransform not available: {message}");
        }
        Err(FilterError::Ffmpeg { .. }) => {
            // Expected: FFmpeg reported an error (trf file not found).
        }
        Err(e) => panic!("expected FilterError::Ffmpeg, got {e:?}"),
        Ok(()) => panic!("expected error for non-existent trf, got Ok(())"),
    }
}

/// Verifies that `StabilizeOptions::default()` has the documented field values.
#[test]
fn stabilize_options_default_should_have_expected_values() {
    let opts = StabilizeOptions::default();
    assert_eq!(opts.smoothing, 10);
    assert!(opts.crop_black);
    assert!((opts.zoom - 0.0_f32).abs() < f32::EPSILON);
    assert_eq!(opts.optzoom, 0);
    assert_eq!(opts.interpol, Interpolation::Bilinear);
}
