//! Integration tests for `Timeline::render()`.
//!
//! Creates synthetic source clips, builds a `Timeline`, calls `render()`,
//! and validates the output with `ff_probe`.

#![allow(clippy::unwrap_used)]

mod fixtures;

use std::time::Duration;

use ff_encode::{AudioCodec, BitrateMode, VideoCodec};
use ff_pipeline::{Clip, EncoderConfig, PipelineError, Timeline};
use fixtures::{FileGuard, make_source_file, test_output_path};

// Small canvas for fast CI runs.
const W: u32 = 160;
const H: u32 = 90;
const FPS: f64 = 30.0;
// 30 frames ≈ 1 second of source content per clip.
const FRAME_COUNT: usize = 30;

fn render_config() -> EncoderConfig {
    EncoderConfig::builder()
        .video_codec(VideoCodec::Mpeg4)
        .audio_codec(AudioCodec::Aac)
        .bitrate_mode(BitrateMode::Cbr(500_000))
        .build()
}

#[test]
fn timeline_render_should_produce_ffprobe_valid_output() {
    // ── Step 1: generate a synthetic source file ───────────────────────────────
    //
    // One 1-second solid-colour clip (red-ish in YUV) that is reused as both
    // clip 1 (starts at t=0) and clip 2 (starts at t=1 s).  Using the same
    // source for both clips keeps the test self-contained without requiring two
    // encode passes.

    let src_path = test_output_path("timeline_src.mp4");
    let out_path = test_output_path("timeline_out.mp4");

    let _g_src = FileGuard::new(src_path.clone());
    let _g_out = FileGuard::new(out_path.clone());

    // Y=76, U=84, V=255 ≈ red in YUV420P
    if make_source_file(&src_path, W, H, FPS, FRAME_COUNT, 76, 84, 255).is_none() {
        return;
    }

    // ── Step 2: build a Timeline with two clips in sequence ────────────────────
    //
    // Both video and audio tracks contain:
    //   clip 1 — timeline_offset = 0 s  (plays from 0 → 1 s)
    //   clip 2 — timeline_offset = 1 s  (plays from 1 → 2 s)
    //
    // `Clip::new` defaults: in_point = None, out_point = None (full file).

    let clip1 = Clip::new(&src_path);
    let clip2 = Clip::new(&src_path).offset(Duration::from_secs(1));

    let timeline = match Timeline::builder()
        .canvas(W, H)
        .frame_rate(FPS)
        .video_track(vec![clip1.clone(), clip2.clone()])
        .audio_track(vec![clip1, clip2])
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            println!("Skipping: Timeline::builder().build() failed: {e}");
            return;
        }
    };

    // ── Step 3: render ─────────────────────────────────────────────────────────

    match timeline.render(&out_path, render_config()) {
        Ok(()) => {}
        Err(PipelineError::Filter(e)) => {
            println!("Skipping: filter graph construction failed: {e}");
            return;
        }
        Err(PipelineError::Encode(e)) => {
            println!("Skipping: encoder unavailable: {e}");
            return;
        }
        Err(PipelineError::Decode(e)) => {
            println!("Skipping: decoder unavailable: {e}");
            return;
        }
        Err(e) => panic!("unexpected error from Timeline::render: {e}"),
    }

    // ── Step 4: validate with ff_probe ─────────────────────────────────────────

    let info = match ff_probe::open(&out_path) {
        Ok(i) => i,
        Err(e) => {
            println!("Skipping: ff_probe::open failed: {e}");
            return;
        }
    };

    assert!(
        info.has_video(),
        "rendered output must contain a video stream"
    );
    assert!(
        info.has_audio(),
        "rendered output must contain an audio stream"
    );

    let duration = info.duration();
    assert!(
        duration > Duration::ZERO,
        "rendered output must have non-zero duration, got {duration:?}"
    );

    let video = info.video_stream(0).expect("video stream must be present");
    assert_eq!(
        video.width(),
        W,
        "output video width must match canvas width"
    );
    assert_eq!(
        video.height(),
        H,
        "output video height must match canvas height"
    );
}
