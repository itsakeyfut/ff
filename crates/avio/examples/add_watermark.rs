//! Overlay a PNG watermark onto a video using the `overlay` filter.
//!
//! The `overlay` filter requires **two** video input streams:
//! - slot 0: the main video frames (pushed per decoded frame)
//! - slot 1: the watermark frame (pushed per decoded frame; same image repeated)
//!
//! Because `Pipeline` only drives slot 0, this example uses the lower-level
//! `VideoDecoder` → `FilterGraph` → `VideoEncoder` path directly.
//!
//! Note: audio is not copied to the output in this example.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example add_watermark --features pipeline -- \
//!   --input     input.mp4   \
//!   --watermark logo.png    \
//!   --output    branded.mp4 \
//!   [--position bottom-right]  \
//!   [--margin   20]
//! ```

use std::{path::Path, process};

use avio::{BitrateMode, FilterGraphBuilder, ImageDecoder, VideoDecoder, VideoEncoder};

// ── Position helpers ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

impl Position {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-right" => Some(Self::BottomRight),
            "center" => Some(Self::Center),
            _ => None,
        }
    }

    fn compute(self, vid_w: u32, vid_h: u32, wm_w: u32, wm_h: u32, margin: u32) -> (i32, i32) {
        let m = margin.cast_signed();
        let x = match self {
            Self::TopLeft | Self::BottomLeft => m,
            Self::TopRight | Self::BottomRight => vid_w.cast_signed() - wm_w.cast_signed() - m,
            Self::Center => (vid_w.cast_signed() - wm_w.cast_signed()) / 2,
        };
        let y = match self {
            Self::TopLeft | Self::TopRight => m,
            Self::BottomLeft | Self::BottomRight => vid_h.cast_signed() - wm_h.cast_signed() - m,
            Self::Center => (vid_h.cast_signed() - wm_h.cast_signed()) / 2,
        };
        (x, y)
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input = None::<String>;
    let mut watermark = None::<String>;
    let mut output = None::<String>;
    let mut position = Position::BottomRight;
    let mut margin: u32 = 20;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--input" | "-i" => input = Some(args.next().unwrap_or_default()),
            "--watermark" | "-w" => watermark = Some(args.next().unwrap_or_default()),
            "--output" | "-o" => output = Some(args.next().unwrap_or_default()),
            "--position" => {
                let s = args.next().unwrap_or_default();
                position = Position::from_str(&s).unwrap_or_else(|| {
                    eprintln!("Unknown position '{s}' (try top-left, top-right, bottom-left, bottom-right, center)");
                    process::exit(1);
                });
            }
            "--margin" => {
                let v = args.next().unwrap_or_default();
                margin = v.parse().unwrap_or(20);
            }
            other => {
                eprintln!("Unknown flag: {other}");
                process::exit(1);
            }
        }
    }

    let input = input.unwrap_or_else(|| {
        eprintln!("Usage: add_watermark --input <video> --watermark <logo.png> --output <branded.mp4> [--position top-left|top-right|bottom-left|bottom-right|center] [--margin N]");
        process::exit(1);
    });
    let watermark = watermark.unwrap_or_else(|| {
        eprintln!("--watermark is required");
        process::exit(1);
    });
    let output = output.unwrap_or_else(|| {
        eprintln!("--output is required");
        process::exit(1);
    });

    // ── Decode watermark image ────────────────────────────────────────────────

    let wm_dec = match ImageDecoder::open(&watermark).build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error opening watermark: {e}");
            process::exit(1);
        }
    };
    let wm_frame = match wm_dec.decode() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error decoding watermark: {e}");
            process::exit(1);
        }
    };
    let wm_w = wm_frame.width();
    let wm_h = wm_frame.height();

    // ── Open video decoder ────────────────────────────────────────────────────

    let mut vid_dec = match VideoDecoder::open(&input).build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error opening video: {e}");
            process::exit(1);
        }
    };
    let vid_w = vid_dec.width();
    let vid_h = vid_dec.height();
    let fps = vid_dec.frame_rate();

    // ── Compute overlay position ──────────────────────────────────────────────

    let (x, y) = position.compute(vid_w, vid_h, wm_w, wm_h, margin);

    let in_name = Path::new(&input)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&input);
    let wm_name = Path::new(&watermark)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&watermark);
    let out_name = Path::new(&output)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&output);

    println!("Input:      {in_name}  {vid_w}×{vid_h}");
    println!("Watermark:  {wm_name}  {wm_w}×{wm_h}");
    println!("Position:   {position:?}  (x={x}, y={y})");
    println!("Output:     {out_name}");
    println!();

    // ── Build overlay filter graph ────────────────────────────────────────────
    //
    // The overlay filter expects two buffersrc inputs:
    //   slot 0 → main video frame  (pushed for every decoded frame)
    //   slot 1 → watermark frame   (same image pushed again each iteration)

    let mut filter = match FilterGraphBuilder::new().overlay(x, y).build() {
        Ok(fg) => fg,
        Err(e) => {
            eprintln!("Error building filter graph: {e}");
            process::exit(1);
        }
    };

    // ── Build video encoder ───────────────────────────────────────────────────

    // Audio is not handled in this example; output is video-only.
    let mut enc = match VideoEncoder::create(&output)
        .video(vid_w, vid_h, fps)
        .video_codec(avio::VideoCodec::H264)
        .bitrate_mode(BitrateMode::Crf(23))
        .build()
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error creating encoder: {e}");
            process::exit(1);
        }
    };

    // ── Decode → filter → encode loop ────────────────────────────────────────

    let mut frames_out = 0u64;
    loop {
        let frame = match vid_dec.decode_one() {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                eprintln!("Error decoding: {e}");
                process::exit(1);
            }
        };

        // Push main video frame to slot 0.
        if let Err(e) = filter.push_video(0, &frame) {
            eprintln!("Error pushing to filter slot 0: {e}");
            process::exit(1);
        }

        // Push watermark to slot 1 (repeat each frame so the buffersrc stays fed).
        if let Err(e) = filter.push_video(1, &wm_frame) {
            eprintln!("Error pushing watermark to filter slot 1: {e}");
            process::exit(1);
        }

        while let Ok(Some(filtered)) = filter.pull_video() {
            if let Err(e) = enc.push_video(&filtered) {
                eprintln!("Error encoding: {e}");
                process::exit(1);
            }
            frames_out += 1;
            if frames_out.is_multiple_of(100) {
                print!("\r{frames_out} frames encoded    ");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
        }
    }

    if let Err(e) = enc.finish() {
        eprintln!("\nError finishing encode: {e}");
        process::exit(1);
    }

    println!("\rDone. {out_name}  {frames_out} frames");
}
