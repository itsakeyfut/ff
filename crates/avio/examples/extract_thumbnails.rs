//! Extract JPEG thumbnails at given timestamps using `ThumbnailPipeline`.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example extract_thumbnails --features pipeline -- \
//!   --input  video.mp4 \
//!   --output thumbs/   \
//!   --times  0,30,60,90 \
//!   [--width 320]
//! ```

use std::{path::Path, process, time::Duration};

use avio::{ImageEncoder, ThumbnailPipeline, VideoDecoder};

fn parse_time(s: &str) -> Result<f64, String> {
    // Accept plain seconds "30" or HH:MM:SS "00:01:30"
    if s.contains(':') {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() == 3 {
            let h: f64 = parts[0]
                .parse()
                .map_err(|_| format!("invalid hours in '{s}'"))?;
            let m: f64 = parts[1]
                .parse()
                .map_err(|_| format!("invalid minutes in '{s}'"))?;
            let sec: f64 = parts[2]
                .parse()
                .map_err(|_| format!("invalid seconds in '{s}'"))?;
            Ok(h * 3600.0 + m * 60.0 + sec)
        } else {
            Err(format!(
                "invalid time format '{s}' (use HH:MM:SS or plain seconds)"
            ))
        }
    } else {
        s.parse::<f64>().map_err(|_| format!("invalid time '{s}'"))
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input = None::<String>;
    let mut output = None::<String>;
    let mut times_str = None::<String>;
    let mut width: u32 = 320;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--input" | "-i" => input = Some(args.next().unwrap_or_default()),
            "--output" | "-o" => output = Some(args.next().unwrap_or_default()),
            "--times" | "-t" => times_str = Some(args.next().unwrap_or_default()),
            "--width" => {
                let v = args.next().unwrap_or_default();
                width = v.parse().unwrap_or(320);
            }
            other => {
                eprintln!("Unknown flag: {other}");
                process::exit(1);
            }
        }
    }

    let input = input.unwrap_or_else(|| {
        eprintln!("Usage: extract_thumbnails --input <file> --output <dir> --times 0,30,60");
        process::exit(1);
    });
    let output = output.unwrap_or_else(|| {
        eprintln!("--output is required");
        process::exit(1);
    });
    let times_str = times_str.unwrap_or_else(|| {
        eprintln!("--times is required (e.g. 0,30,60)");
        process::exit(1);
    });

    // Parse timestamps
    let timestamps: Vec<f64> = times_str
        .split(',')
        .map(|s| {
            parse_time(s.trim()).unwrap_or_else(|e| {
                eprintln!("Error parsing time: {e}");
                process::exit(1);
            })
        })
        .collect();

    // Probe source duration so out-of-range timestamps can be skipped gracefully.
    let src_duration = match VideoDecoder::open(&input).build() {
        Ok(dec) => dec.duration(),
        Err(e) => {
            eprintln!("Error opening input: {e}");
            process::exit(1);
        }
    };

    // Filter timestamps that are within the file duration
    let valid_timestamps: Vec<f64> = timestamps
        .iter()
        .copied()
        .filter(|&t| Duration::from_secs_f64(t) <= src_duration)
        .collect();
    let skipped = timestamps.len() - valid_timestamps.len();

    // Create output directory
    if let Err(e) = std::fs::create_dir_all(&output) {
        eprintln!("Error: cannot create output directory: {e}");
        process::exit(1);
    }

    let in_name = Path::new(&input)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&input);

    println!(
        "Extracting {} thumbnails from {in_name}...",
        valid_timestamps.len()
    );
    if skipped > 0 {
        println!("  (skipping {skipped} timestamps beyond file duration)");
    }

    // Run ThumbnailPipeline
    let frames = match ThumbnailPipeline::new(&input)
        .timestamps(valid_timestamps.clone())
        .run()
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    };

    // Encode each frame as JPEG
    for (frame, &timestamp) in frames.iter().zip(valid_timestamps.iter()) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let secs = timestamp as u64;
        let out_name = format!("thumb_{secs:03}.jpg");
        let out_path = Path::new(&output).join(&out_name);

        let w = frame.width();
        let h = frame.height();

        // Scale width if needed (keep aspect ratio)
        let (enc_w, enc_h) = if w > width {
            let scale = f64::from(width) / f64::from(w);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let enc_h = (f64::from(h) * scale).round() as u32;
            (width, enc_h)
        } else {
            (w, h)
        };

        match ImageEncoder::create(&out_path)
            .width(enc_w)
            .height(enc_h)
            .build()
        {
            Ok(enc) => {
                if let Err(e) = enc.encode(frame) {
                    eprintln!("  Warning: failed to encode {out_name}: {e}");
                    continue;
                }
            }
            Err(e) => {
                eprintln!("  Warning: failed to create encoder for {out_name}: {e}");
                continue;
            }
        }

        let hh = secs / 3600;
        let mm = (secs % 3600) / 60;
        let ss = secs % 60;
        println!("  {hh:02}:{mm:02}:{ss:02}  →  {out_name}  ({enc_w}×{enc_h})");
    }

    println!("Done.");
}
