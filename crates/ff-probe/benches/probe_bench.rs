//! Benchmark tests for ff-probe.
//!
//! These benchmarks measure the performance of media file probing operations.
//!
//! Run with: `cargo bench -p ff-probe`

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ff_probe::open;

// ============================================================================
// Test Helpers
// ============================================================================

/// Returns the path to the test assets directory.
fn assets_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(format!("{}/../../assets", manifest_dir))
}

/// Returns the path to the test video file.
fn test_video_path() -> PathBuf {
    assets_dir().join("video/gameplay.mp4")
}

/// Returns the path to the test audio file.
fn test_audio_path() -> PathBuf {
    assets_dir().join("audio/konekonoosanpo.mp3")
}

// ============================================================================
// Probe Benchmarks
// ============================================================================

/// Benchmark probing a video file.
fn bench_probe_video(c: &mut Criterion) {
    let path = test_video_path();

    // Verify the file exists
    assert!(path.exists(), "Test video file not found: {:?}", path);

    // Get file size for throughput calculation
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    let mut group = c.benchmark_group("probe_video");
    group.throughput(Throughput::Bytes(file_size));

    group.bench_function("mp4_file", |b| {
        b.iter(|| {
            let info = open(black_box(&path)).expect("Failed to probe video");
            black_box(info)
        });
    });

    group.finish();
}

/// Benchmark probing an audio file.
fn bench_probe_audio(c: &mut Criterion) {
    let path = test_audio_path();

    // Verify the file exists
    assert!(path.exists(), "Test audio file not found: {:?}", path);

    // Get file size for throughput calculation
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    let mut group = c.benchmark_group("probe_audio");
    group.throughput(Throughput::Bytes(file_size));

    group.bench_function("mp3_file", |b| {
        b.iter(|| {
            let info = open(black_box(&path)).expect("Failed to probe audio");
            black_box(info)
        });
    });

    group.finish();
}

/// Benchmark probing different media types.
fn bench_probe_media_types(c: &mut Criterion) {
    let video_path = test_video_path();
    let audio_path = test_audio_path();

    let mut group = c.benchmark_group("probe_media_types");

    // Video file
    if video_path.exists() {
        group.bench_with_input(
            BenchmarkId::new("probe", "video_mp4"),
            &video_path,
            |b, path| {
                b.iter(|| {
                    let info = open(black_box(path)).expect("Failed to probe");
                    black_box(info)
                });
            },
        );
    }

    // Audio file
    if audio_path.exists() {
        group.bench_with_input(
            BenchmarkId::new("probe", "audio_mp3"),
            &audio_path,
            |b, path| {
                b.iter(|| {
                    let info = open(black_box(path)).expect("Failed to probe");
                    black_box(info)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark metadata extraction operations.
fn bench_metadata_extraction(c: &mut Criterion) {
    let video_path = test_video_path();
    let audio_path = test_audio_path();

    // Pre-load the media info
    let video_info = open(&video_path).expect("Failed to probe video");
    let audio_info = open(&audio_path).expect("Failed to probe audio");

    let mut group = c.benchmark_group("metadata_extraction");

    // Benchmark accessing video stream properties
    group.bench_function("video_stream_access", |b| {
        b.iter(|| {
            let info = black_box(&video_info);
            let _ = black_box(info.has_video());
            let _ = black_box(info.video_stream_count());
            let _ = black_box(info.primary_video());
            let _ = black_box(info.resolution());
            let _ = black_box(info.frame_rate());
        });
    });

    // Benchmark accessing audio stream properties
    group.bench_function("audio_stream_access", |b| {
        b.iter(|| {
            let info = black_box(&audio_info);
            let _ = black_box(info.has_audio());
            let _ = black_box(info.audio_stream_count());
            let _ = black_box(info.primary_audio());
            let _ = black_box(info.sample_rate());
            let _ = black_box(info.channels());
        });
    });

    // Benchmark accessing common properties
    group.bench_function("common_properties", |b| {
        b.iter(|| {
            let info = black_box(&video_info);
            let _ = black_box(info.duration());
            let _ = black_box(info.file_size());
            let _ = black_box(info.format());
            let _ = black_box(info.path());
            let _ = black_box(info.file_name());
            let _ = black_box(info.extension());
        });
    });

    group.finish();
}

/// Benchmark repeated probing (simulating batch processing).
fn bench_repeated_probe(c: &mut Criterion) {
    let video_path = test_video_path();

    if !video_path.exists() {
        return;
    }

    let mut group = c.benchmark_group("repeated_probe");

    // Benchmark probing the same file multiple times
    for count in [1, 5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::new("batch_probe", count),
            count,
            |b, &count| {
                b.iter(|| {
                    for _ in 0..count {
                        let info = open(black_box(&video_path)).expect("Failed to probe");
                        black_box(info);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_probe_video,
    bench_probe_audio,
    bench_probe_media_types,
    bench_metadata_extraction,
    bench_repeated_probe,
);

criterion_main!(benches);
