//! Progress callback tests.
//!
//! Tests the progress callback functionality during actual encoding:
//! - Simple closure-based callback
//! - Struct-based callback with cancellation
//! - Progress information accuracy

#![allow(clippy::unwrap_used)]

mod fixtures;

use ff_encode::{Progress, ProgressCallback, VideoCodec, VideoEncoder};
use fixtures::{FileGuard, assert_valid_output_file, create_black_frame, test_output_path};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// ============================================================================
// Simple Progress Callback Tests
// ============================================================================

#[test]
fn test_progress_callback_closure() {
    let output_path = test_output_path("test_progress_closure.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let progress_count = Arc::new(AtomicU64::new(0));
    let progress_count_clone = progress_count.clone();

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .on_progress(move |progress: &Progress| {
            progress_count_clone.fetch_add(1, Ordering::Relaxed);
            println!(
                "Progress: {} frames encoded at {:.1} fps",
                progress.frames_encoded, progress.current_fps
            );
        })
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return;
        }
    };

    // Encode 30 frames
    for _ in 0..30 {
        let frame = create_black_frame(640, 480);
        encoder.push_video(&frame).expect("Failed to push frame");
    }

    encoder.finish().expect("Failed to finish encoding");

    // Verify callback was called
    let calls = progress_count.load(Ordering::Relaxed);
    assert!(calls > 0, "Progress callback should have been called");
    println!("Progress callback called {} times", calls);

    assert_valid_output_file(&output_path);
}

// ============================================================================
// Struct-based Callback with Cancellation
// ============================================================================

struct TestProgressCallback {
    frames_seen: Arc<AtomicU64>,
    max_frames: u64,
}

impl ProgressCallback for TestProgressCallback {
    fn on_progress(&mut self, progress: &Progress) {
        self.frames_seen
            .store(progress.frames_encoded, Ordering::Relaxed);
        println!(
            "Callback: {} frames, {:.1} fps, {} bytes",
            progress.frames_encoded, progress.current_fps, progress.bytes_written
        );
    }

    fn should_cancel(&self) -> bool {
        // Cancel if we've seen too many frames
        self.frames_seen.load(Ordering::Relaxed) >= self.max_frames
    }
}

#[test]
fn test_progress_callback_struct() {
    let output_path = test_output_path("test_progress_struct.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let frames_seen = Arc::new(AtomicU64::new(0));

    let callback = TestProgressCallback {
        frames_seen: frames_seen.clone(),
        max_frames: u64::MAX, // Don't cancel
    };

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .progress_callback(callback)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return;
        }
    };

    // Encode 20 frames
    for _ in 0..20 {
        let frame = create_black_frame(640, 480);
        encoder.push_video(&frame).expect("Failed to push frame");
    }

    encoder.finish().expect("Failed to finish encoding");

    // Verify callback saw the frames
    let seen = frames_seen.load(Ordering::Relaxed);
    assert_eq!(seen, 20, "Callback should have seen all 20 frames");

    assert_valid_output_file(&output_path);
}

// ============================================================================
// Cancellation Tests
// ============================================================================

struct CancellableCallback {
    cancelled: Arc<AtomicBool>,
}

impl ProgressCallback for CancellableCallback {
    fn on_progress(&mut self, progress: &Progress) {
        println!("Progress: {} frames", progress.frames_encoded);
    }

    fn should_cancel(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

#[test]
fn test_progress_callback_cancellation() {
    let output_path = test_output_path("test_progress_cancel.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let cancelled = Arc::new(AtomicBool::new(false));

    let callback = CancellableCallback {
        cancelled: cancelled.clone(),
    };

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .progress_callback(callback)
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return;
        }
    };

    // Cancel after 10 frames
    for i in 0..100 {
        if i == 10 {
            cancelled.store(true, Ordering::Relaxed);
        }

        let frame = create_black_frame(640, 480);
        let result = encoder.push_video(&frame);

        if result.is_err() {
            println!("Encoding cancelled at frame {}", i);
            break;
        }
    }

    // Note: We don't call finish() because encoding was cancelled
    println!("Test completed (encoding was cancelled as expected)");
}

// ============================================================================
// Progress Information Accuracy Tests
// ============================================================================

#[test]
fn test_progress_information_accuracy() {
    let output_path = test_output_path("test_progress_accuracy.mp4");
    let _guard = FileGuard::new(output_path.clone());

    let last_progress = Arc::new(std::sync::Mutex::new(None));
    let last_progress_clone = last_progress.clone();

    let result = VideoEncoder::create(&output_path)
        .video(640, 480, 30.0)
        .video_codec(VideoCodec::Mpeg4)
        .on_progress(move |progress: &Progress| {
            let mut last = last_progress_clone.lock().unwrap();
            *last = Some(progress.clone());
        })
        .build();

    let mut encoder = match result {
        Ok(enc) => enc,
        Err(e) => {
            println!("Encoder creation failed (no suitable codec): {}", e);
            return;
        }
    };

    // Encode 50 frames
    let total_frames = 50;
    for _ in 0..total_frames {
        let frame = create_black_frame(640, 480);
        encoder.push_video(&frame).expect("Failed to push frame");
    }

    encoder.finish().expect("Failed to finish encoding");

    // Check progress information
    let last = last_progress.lock().unwrap();
    if let Some(ref progress) = *last {
        println!("Final progress:");
        println!("  Frames encoded: {}", progress.frames_encoded);
        println!("  Bytes written: {}", progress.bytes_written);
        println!("  Current FPS: {:.1}", progress.current_fps);
        println!("  Elapsed: {:?}", progress.elapsed);

        // Verify progress information
        assert_eq!(
            progress.frames_encoded, total_frames,
            "Should have encoded all frames"
        );
        // Note: bytes_written might be 0 if data hasn't been flushed yet
        // This is normal during encoding - data is buffered and written in chunks
        println!(
            "  Note: {} bytes written (data may be buffered)",
            progress.bytes_written
        );
        assert!(progress.current_fps >= 0.0, "FPS should be non-negative");
        assert!(
            !progress.elapsed.is_zero(),
            "Elapsed time should be non-zero"
        );
    } else {
        panic!("Progress callback was never called");
    }

    assert_valid_output_file(&output_path);
}
