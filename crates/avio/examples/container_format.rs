//! Encode video with an explicitly selected output container format.
//!
//! Demonstrates:
//! - `Container` enum — `Mp4`, `Mkv`, `WebM`, `Avi`, `Mov`
//! - `Container::as_str()` — `FFmpeg` format name
//! - `Container::default_extension()` — canonical file extension
//! - `VideoEncoderBuilder::container()` — override the container inferred
//!   from the output file extension
//!
//! By default the container is auto-detected from the output extension.
//! Use `container()` when the extension does not match the desired format
//! or when you need to guarantee a specific muxer regardless of the path.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example container_format --features "decode encode" -- \
//!   --input     input.mp4    \
//!   --output    output.mkv   \
//!   [--container mp4|mkv|webm|avi|mov]  # default: infer from extension
//! ```

use std::{path::Path, process};

use avio::{Container, VideoCodec, VideoDecoder, VideoEncoder};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input = None::<String>;
    let mut output = None::<String>;
    let mut container_str: Option<String> = None;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--input" | "-i" => input = Some(args.next().unwrap_or_default()),
            "--output" | "-o" => output = Some(args.next().unwrap_or_default()),
            "--container" | "-f" => container_str = Some(args.next().unwrap_or_default()),
            other => {
                eprintln!("Unknown flag: {other}");
                process::exit(1);
            }
        }
    }

    let input = input.unwrap_or_else(|| {
        eprintln!(
            "Usage: container_format --input <file> --output <file> \
             [--container mp4|mkv|webm|avi|mov]"
        );
        process::exit(1);
    });
    let output = output.unwrap_or_else(|| {
        eprintln!("--output is required");
        process::exit(1);
    });

    // ── Parse Container variant ───────────────────────────────────────────────

    let container = container_str
        .as_deref()
        .map(|s| match s.to_lowercase().as_str() {
            "mp4" => Container::Mp4,
            "mkv" | "matroska" => Container::Mkv,
            "webm" => Container::WebM,
            "avi" => Container::Avi,
            "mov" => Container::Mov,
            other => {
                eprintln!("Unknown container '{other}' (try mp4, mkv, webm, avi, mov)");
                process::exit(1);
            }
        });

    // ── Probe source ──────────────────────────────────────────────────────────

    let probe = match VideoDecoder::open(&input).build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error opening input: {e}");
            process::exit(1);
        }
    };
    let width = probe.width();
    let height = probe.height();
    let fps = probe.frame_rate();
    let in_codec = probe.stream_info().codec_name().to_string();
    drop(probe);

    let in_name = Path::new(&input)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&input);
    let out_name = Path::new(&output)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&output);

    println!("Input:  {in_name}  {width}×{height}  {fps:.2} fps  codec={in_codec}");

    match container {
        Some(c) => println!(
            "Container: {c:?} (format='{}')  default_ext='{}'",
            c.as_str(),
            c.default_extension()
        ),
        None => println!("Container: (inferred from output extension)"),
    }

    println!("Output: {out_name}");
    println!();

    // ── Build encoder with explicit container ─────────────────────────────────
    //
    // .container() overrides the muxer selection.
    // When omitted, FFmpeg infers the container from the output path extension.

    let mut enc_builder = VideoEncoder::create(&output)
        .video(width, height, fps)
        .video_codec(VideoCodec::H264);

    if let Some(c) = container {
        enc_builder = enc_builder.container(c);
    }

    let mut encoder = match enc_builder.build() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error building encoder: {e}");
            process::exit(1);
        }
    };

    println!("Encoding...");

    // ── Decode + encode loop ──────────────────────────────────────────────────

    let mut decoder = match VideoDecoder::open(&input).build() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error opening decoder: {e}");
            process::exit(1);
        }
    };

    let mut frames: u64 = 0;

    loop {
        let frame = match decoder.decode_one() {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                eprintln!("Decode error: {e}");
                process::exit(1);
            }
        };

        if let Err(e) = encoder.push_video(&frame) {
            eprintln!("Encode error: {e}");
            process::exit(1);
        }

        frames += 1;
    }

    if let Err(e) = encoder.finish() {
        eprintln!("Error finalising output: {e}");
        process::exit(1);
    }

    let size_str = match std::fs::metadata(&output) {
        #[allow(clippy::cast_precision_loss)]
        Ok(m) => format!("{:.1} MB", m.len() as f64 / 1_048_576.0),
        Err(_) => "(unknown size)".to_string(),
    };

    println!("Done. {out_name}  {size_str}  {frames} frames");
}
