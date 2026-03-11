//! Benchmark tests for ff-decode.
//!
//! These benchmarks measure the performance of video/audio decoding operations,
//! seeking, and thumbnail generation.
//!
//! Run with: `cargo bench -p ff-decode`

use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ff_decode::{HardwareAccel, SeekMode, VideoDecoder};

// ============================================================================
// Test Helpers
// ============================================================================

/// Returns the path to the test assets directory.
fn assets_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{}/../assets", manifest_dir))
}

/// Returns the path to the test video file.
fn test_video_path() -> PathBuf {
    assets_dir().join("video/gameplay.mp4")
}

/// Creates a test decoder with hardware acceleration disabled for consistency.
fn create_decoder() -> VideoDecoder {
    VideoDecoder::open(&test_video_path())
        .hardware_accel(HardwareAccel::None)
        .build()
        .expect("Failed to create decoder")
}

// ============================================================================
// Decode Benchmarks
// ============================================================================

/// Benchmark decoding a single frame.
fn bench_decode_single_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");

    group.bench_function("single_frame", |b| {
        b.iter_batched(
            || create_decoder(),
            |mut decoder| {
                let frame = decoder.decode_one().expect("Failed to decode");
                black_box(frame)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark decoding multiple frames sequentially.
fn bench_decode_multiple_frames(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_sequential");

    for count in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter_batched(
                || create_decoder(),
                |mut decoder| {
                    for _ in 0..count {
                        let frame = decoder.decode_one().expect("Failed to decode");
                        if frame.is_none() {
                            break;
                        }
                        black_box(frame);
                    }
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

/// Benchmark frame iterator.
fn bench_frame_iterator(c: &mut Criterion) {
    let mut group = c.benchmark_group("iterator");

    group.bench_function("frames_take_10", |b| {
        b.iter_batched(
            || create_decoder(),
            |mut decoder| {
                let frames: Vec<_> = decoder.frames().take(10).filter_map(|r| r.ok()).collect();
                black_box(frames)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

// ============================================================================
// Seek Benchmarks
// ============================================================================

/// Benchmark keyframe seeking.
fn bench_seek_keyframe(c: &mut Criterion) {
    let mut group = c.benchmark_group("seek_keyframe");

    group.bench_function("seek_2s", |b| {
        b.iter_batched(
            || {
                let mut decoder = create_decoder();
                // Decode a few frames first to initialize
                for _ in 0..5 {
                    let _ = decoder.decode_one();
                }
                decoder
            },
            |mut decoder| {
                decoder
                    .seek(black_box(Duration::from_secs(2)), SeekMode::Keyframe)
                    .expect("Seek failed");
                let frame = decoder.decode_one().expect("Decode after seek failed");
                black_box(frame)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark exact seeking.
fn bench_seek_exact(c: &mut Criterion) {
    let mut group = c.benchmark_group("seek_exact");

    group.bench_function("seek_2s", |b| {
        b.iter_batched(
            || {
                let mut decoder = create_decoder();
                // Decode a few frames first to initialize
                for _ in 0..5 {
                    let _ = decoder.decode_one();
                }
                decoder
            },
            |mut decoder| {
                decoder
                    .seek(black_box(Duration::from_secs(2)), SeekMode::Exact)
                    .expect("Seek failed");
                let frame = decoder.decode_one().expect("Decode after seek failed");
                black_box(frame)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark repeated seeking (simulating scrubbing).
fn bench_repeated_seek(c: &mut Criterion) {
    let mut group = c.benchmark_group("seek_repeated");

    let positions = [
        Duration::from_secs(1),
        Duration::from_secs(3),
        Duration::from_secs(5),
        Duration::from_secs(2),
        Duration::from_secs(4),
    ];

    group.bench_function("scrubbing_5_positions", |b| {
        b.iter_batched(
            || create_decoder(),
            |mut decoder| {
                for &pos in &positions {
                    decoder
                        .seek(black_box(pos), SeekMode::Keyframe)
                        .expect("Seek failed");
                    let frame = decoder.decode_one().expect("Decode failed");
                    black_box(frame);
                }
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

// ============================================================================
// Thumbnail Benchmarks
// ============================================================================

/// Benchmark generating a single thumbnail.
fn bench_thumbnail_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("thumbnail");

    group.bench_function("320x180_at_2s", |b| {
        b.iter_batched(
            || create_decoder(),
            |mut decoder| {
                let thumb = decoder
                    .thumbnail_at(
                        black_box(Duration::from_secs(2)),
                        black_box(320),
                        black_box(180),
                    )
                    .expect("Thumbnail generation failed");
                black_box(thumb)
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark generating multiple thumbnails.
fn bench_thumbnail_multiple(c: &mut Criterion) {
    let mut group = c.benchmark_group("thumbnail_batch");

    for count in [5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter_batched(
                || create_decoder(),
                |mut decoder| {
                    let thumbs = decoder
                        .thumbnails(black_box(count), black_box(160), black_box(90))
                        .expect("Thumbnails generation failed");
                    black_box(thumbs)
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

// ============================================================================
// Mixed Operation Benchmarks
// ============================================================================

/// Benchmark seek + decode workflow (common in video editing).
fn bench_seek_and_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow");

    group.bench_function("seek_decode_3_frames", |b| {
        b.iter_batched(
            || create_decoder(),
            |mut decoder| {
                decoder
                    .seek(black_box(Duration::from_secs(2)), SeekMode::Keyframe)
                    .expect("Seek failed");

                // Decode 3 frames after seeking
                for _ in 0..3 {
                    let frame = decoder.decode_one().expect("Decode failed");
                    if frame.is_none() {
                        break;
                    }
                    black_box(frame);
                }
            },
            criterion::BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_decode_single_frame,
    bench_decode_multiple_frames,
    bench_frame_iterator,
    bench_seek_keyframe,
    bench_seek_exact,
    bench_repeated_seek,
    bench_thumbnail_single,
    bench_thumbnail_multiple,
    bench_seek_and_decode,
);

criterion_main!(benches);
