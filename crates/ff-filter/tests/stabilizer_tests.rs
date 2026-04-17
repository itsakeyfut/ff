//! Integration tests for `Stabilizer::analyze` (vidstabdetect pass 1).

mod fixtures;

use std::path::PathBuf;

use ff_filter::{AnalyzeOptions, FilterError, Stabilizer};
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
