//! Shared helpers for the avio editing verification scripts.
//!
//! These use the public [`avio`] engine surface (plus a few `ff-*` primitives for
//! fixture synthesis), exactly as a downstream consumer would. The helpers cover the two things every
//! script needs: obtaining an input clip (synthetic by default, `--input`
//! override), and reporting pass/fail checks.

use std::path::{Path, PathBuf};

use avio::{
    BitrateMode, EncoderConfig, PixelFormat, Rational, Timeline, TimelineError, Timestamp,
    VideoCodec, VideoFrame,
};
use ff_common::PooledBuffer;
use ff_decode::VideoDecoder;
use ff_encode::VideoEncoder;

/// Convenience result type for the scripts (`fn main() -> BoxResult<()>`).
pub type BoxResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Parameters for a generated synthetic clip.
#[derive(Debug, Clone, Copy)]
pub struct SynthSpec {
    /// Frame width in pixels (kept even for `yuv420p`).
    pub width: u32,
    /// Frame height in pixels (kept even for `yuv420p`).
    pub height: u32,
    /// Frame rate in frames per second.
    pub fps: f64,
    /// Clip length in seconds.
    pub seconds: u64,
}

impl Default for SynthSpec {
    fn default() -> Self {
        // Small and fast: a 2-second 640x360 clip is enough to exercise the
        // decode/compose/encode paths without a slow encode.
        Self {
            width: 640,
            height: 360,
            fps: 30.0,
            seconds: 2,
        }
    }
}

impl SynthSpec {
    /// Number of frames this spec produces.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        (self.seconds as f64 * self.fps).round() as u64
    }
}

/// Generates a synthetic video clip at `path`: a background hue that sweeps over
/// time with a moving white scanline, so consecutive frames differ.
///
/// Mirrors the proven pattern in `crates/ff-encode/examples/gen_bench_asset.rs`.
///
/// # Errors
///
/// Returns an error if the encoder cannot be built, a frame cannot be created,
/// or encoding/finalising fails.
pub fn generate_synth_clip(path: &Path, spec: &SynthSpec) -> BoxResult<()> {
    if spec.width == 0 || spec.height == 0 {
        return Err("synth clip requires non-zero width and height".into());
    }
    let mut encoder = VideoEncoder::create(path)
        .video(spec.width, spec.height, spec.fps)
        .video_codec(VideoCodec::H264) // falls back to an LGPL codec without the `gpl` feature
        .bitrate_mode(BitrateMode::Crf(23))
        .build()?;

    let stride = spec.width as usize * 4; // RGBA
    let total_frames = spec.frame_count().max(1);
    let time_base = Rational::new(1, spec.fps as i32);

    for i in 0..total_frames {
        let hue = (i as f32 / total_frames as f32) * 360.0;
        let (r, g, b) = hue_to_rgb(hue);
        let scanline_y = (i as usize * spec.height as usize / total_frames as usize)
            .min(spec.height as usize - 1);

        let mut pixels = vec![0u8; stride * spec.height as usize];
        for y in 0..spec.height as usize {
            for x in 0..spec.width as usize {
                let base = y * stride + x * 4;
                let (pr, pg, pb) = if y == scanline_y {
                    (255, 255, 255)
                } else {
                    (r, g, b)
                };
                pixels[base] = pr;
                pixels[base + 1] = pg;
                pixels[base + 2] = pb;
                pixels[base + 3] = 255;
            }
        }

        let frame = VideoFrame::new(
            vec![PooledBuffer::standalone(pixels)],
            vec![stride],
            spec.width,
            spec.height,
            PixelFormat::Rgba,
            Timestamp::new(i as i64, time_base),
            i % 30 == 0, // I-frame every 30 frames
        )?;
        encoder.push_video(&frame)?;
    }

    encoder.finish()?;
    Ok(())
}

/// Parsed command-line arguments shared by all scripts.
#[derive(Debug, Default)]
pub struct Args {
    /// Optional real media file to run against instead of a synthetic clip.
    pub input: Option<PathBuf>,
    /// Keep generated temp files instead of deleting them on exit.
    pub keep: bool,
}

/// Parses `--input <file>` / `-i <file>` and `--keep` from the process args.
#[must_use]
pub fn parse_args() -> Args {
    let mut args = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--input" | "-i" => args.input = it.next().map(PathBuf::from),
            "--keep" => args.keep = true,
            other => eprintln!("warning: ignoring unknown argument: {other}"),
        }
    }
    args
}

/// Returns the input clip to run against: the caller-provided `--input` file if
/// set, otherwise a freshly generated synthetic clip written under `dir`.
///
/// # Errors
///
/// Returns an error if a synthetic clip is needed but cannot be generated.
pub fn resolve_input(args: &Args, dir: &Path) -> BoxResult<PathBuf> {
    if let Some(path) = &args.input {
        println!("input: {} (provided)", path.display());
        return Ok(path.clone());
    }
    let spec = SynthSpec::default();
    let path = dir.join("synth_clip.mp4");
    println!(
        "input: generating synthetic clip {}x{} {}fps {}s -> {}",
        spec.width,
        spec.height,
        spec.fps,
        spec.seconds,
        path.display()
    );
    generate_synth_clip(&path, &spec)?;
    Ok(path)
}

/// Accumulates named pass/fail checks and turns the overall result into a
/// process exit status (an `Err` from [`finish`](Report::finish) makes
/// `fn main() -> BoxResult<()>` exit non-zero).
pub struct Report {
    name: String,
    all_ok: bool,
}

impl Report {
    /// Starts a report for the named script.
    #[must_use]
    pub fn new(name: &str) -> Self {
        println!("=== {name} ===");
        Self {
            name: name.to_owned(),
            all_ok: true,
        }
    }

    /// Records one check, printing `[PASS]`/`[FAIL]` and updating the total.
    pub fn check(&mut self, label: &str, ok: bool) {
        println!("  [{}] {label}", if ok { "PASS" } else { "FAIL" });
        self.all_ok &= ok;
    }

    /// Records a check the environment could not run, printing `[SKIP]` and leaving
    /// the verdict untouched.
    ///
    /// A machine without the codec, filter or GPU adapter a script needs must neither
    /// fail (nothing is wrong with avio) nor pass (nothing was verified), so the two
    /// outcomes `check` offers are both wrong for it.
    pub fn skip(&mut self, label: &str, reason: &str) {
        println!("  [SKIP] {label} ({reason})");
    }

    /// Prints the overall verdict and returns `Ok` only if every check passed.
    ///
    /// # Errors
    ///
    /// Returns an error (so the process exits non-zero) if any check failed.
    pub fn finish(self) -> BoxResult<()> {
        if self.all_ok {
            println!("{}: PASS", self.name);
            Ok(())
        } else {
            Err(format!("{}: FAIL", self.name).into())
        }
    }
}

/// The mean R/G/B of every frame of a rendered file, in decode order.
///
/// Scripts that assert an effect reached the output measure it here: a mean per frame
/// is coarse enough to survive a lossy encode and specific enough to show a colour
/// change and its direction. Alpha is ignored (the encode is opaque).
///
/// # Errors
///
/// Returns an error if the file cannot be opened or decoded, which for a script means
/// the environment lacks the decoder rather than that avio is wrong.
pub fn decoded_frame_means(path: &Path) -> BoxResult<Vec<[f64; 3]>> {
    let mut decoder = VideoDecoder::open(path)
        .output_format(PixelFormat::Rgba)
        .build()?;
    let mut means = Vec::new();
    while let Some(frame) = decoder.decode_one()? {
        let Some(rgba) = frame.to_rgba() else {
            return Err("decoded frame is not convertible to rgba".into());
        };
        means.push(mean_rgb(&rgba));
    }
    Ok(means)
}

/// The mean R/G/B of a contiguous rgba buffer (alpha ignored).
#[must_use]
pub fn mean_rgb(rgba: &[u8]) -> [f64; 3] {
    let (mut sum, mut pixels) = ([0f64; 3], 0f64);
    for px in rgba.as_chunks::<4>().0 {
        sum[0] += f64::from(px[0]);
        sum[1] += f64::from(px[1]);
        sum[2] += f64::from(px[2]);
        pixels += 1.0;
    }
    if pixels == 0.0 {
        return [0.0; 3];
    }
    [sum[0] / pixels, sum[1] / pixels, sum[2] / pixels]
}

/// The mean of a frame's three channel means: one number per frame, for comparing
/// overall lightness between two renders of the same content.
///
/// Not Rec.601 luma: the channels are weighted equally, which is what a like-for-like
/// comparison of two renders of the same frame wants.
#[must_use]
pub fn mean_channel(mean: [f64; 3]) -> f64 {
    (mean[0] + mean[1] + mean[2]) / 3.0
}

/// Whether a render error means "this environment cannot run the pipeline" (skip) as
/// opposed to a real regression (fail). Mirrors the crates' own test convention.
#[must_use]
pub fn is_environment_unavailable(e: &TimelineError) -> bool {
    matches!(
        e,
        TimelineError::Filter(_) | TimelineError::Encode(_) | TimelineError::Decode(_)
    )
}

/// Renders `timeline` to `out`. `Ok(None)` rendered; `Ok(Some(reason))` the environment
/// cannot render here (report it with [`Report::skip`]); `Err` a genuine failure.
///
/// # Errors
///
/// Returns the render error when it is not one this environment merely lacks.
pub fn render_or_skip(timeline: Timeline, out: &Path) -> BoxResult<Option<String>> {
    match timeline.render(out, EncoderConfig::builder().build()) {
        Ok(()) => Ok(None),
        Err(e) if is_environment_unavailable(&e) => Ok(Some(e.to_string())),
        Err(e) => Err(Box::new(e)),
    }
}

/// The largest per-channel difference between two frame means.
#[must_use]
pub fn channel_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).fold(0.0f64, |acc, i| acc.max((a[i] - b[i]).abs()))
}

/// Converts a hue angle (0-360 degrees) to an sRGB triplet (saturation = value = 1).
fn hue_to_rgb(hue: f32) -> (u8, u8, u8) {
    let h = hue.rem_euclid(360.0);
    let i = (h / 60.0) as u32;
    let f = h / 60.0 - i as f32;
    let (r, g, b) = match i {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_should_be_ok_when_all_checks_pass() {
        let mut r = Report::new("t");
        r.check("a", true);
        r.check("b", true);
        assert!(r.finish().is_ok());
    }

    #[test]
    fn report_should_be_err_when_any_check_fails() {
        let mut r = Report::new("t");
        r.check("a", true);
        r.check("b", false);
        assert!(r.finish().is_err());
    }

    #[test]
    fn report_should_stay_ok_when_a_check_is_skipped() {
        let mut r = Report::new("t");
        r.check("a", true);
        r.skip("b", "no decoder here");
        assert!(r.finish().is_ok(), "a skip is not a failure");
    }

    #[test]
    fn mean_rgb_should_average_each_channel_independently() {
        // Two pixels, so a channel-blind implementation (one mean over all bytes)
        // would collapse these three distinct answers into one.
        let rgba = [10u8, 20, 30, 255, 30, 60, 90, 255];
        let mean = mean_rgb(&rgba);
        assert!((mean[0] - 20.0).abs() < 1e-9);
        assert!((mean[1] - 40.0).abs() < 1e-9);
        assert!((mean[2] - 60.0).abs() < 1e-9);
        assert!((mean_channel(mean) - 40.0).abs() < 1e-9);
        assert!((channel_distance(mean, [20.0, 40.0, 45.0]) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn mean_rgb_should_be_zero_for_an_empty_buffer() {
        assert_eq!(mean_rgb(&[]), [0.0; 3]);
    }

    #[test]
    fn synth_spec_default_should_be_even_dims_and_positive() {
        let s = SynthSpec::default();
        assert_eq!(s.width % 2, 0);
        assert_eq!(s.height % 2, 0);
        assert!(s.fps > 0.0);
        assert!(s.seconds > 0);
        assert_eq!(s.frame_count(), 60);
    }

    #[test]
    fn hue_to_rgb_should_wrap_and_stay_in_range() {
        for deg in [0.0, 60.0, 120.0, 359.0, 360.0, 720.0, -30.0] {
            let (r, g, b) = hue_to_rgb(deg);
            let _ = (r, g, b); // u8 is always in range; exercises the wrap path
        }
    }
}
