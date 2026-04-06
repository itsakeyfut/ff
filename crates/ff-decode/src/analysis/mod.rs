//! Audio and video analysis tools.
//!
//! This module provides tools for extracting analytical data from media files.
//! Pure-Rust tools use only the safe decoder API.  Tools that require direct
//! `FFmpeg` filter-graph calls (such as [`SceneDetector`]) delegate all
//! `unsafe` code to `analysis_inner`.

// The single `unsafe` block in `SceneDetector::run` delegates directly to
// `analysis_inner`, where all invariants are documented.
#![allow(unsafe_code)]

pub(crate) mod analysis_inner;

use std::path::{Path, PathBuf};
use std::time::Duration;

use ff_format::{PixelFormat, SampleFormat};

use crate::{AudioDecoder, DecodeError, VideoDecoder};

// ── Public types ──────────────────────────────────────────────────────────────

/// A single waveform measurement over a configurable time interval.
///
/// Both amplitude values are expressed in dBFS (decibels relative to full
/// scale). `0.0` dBFS means the signal reached maximum amplitude; values
/// approach [`f32::NEG_INFINITY`] for silence.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformSample {
    /// Start of the time interval this sample covers.
    pub timestamp: Duration,
    /// Peak amplitude in dBFS (`max(|s|)` over all samples in the interval).
    /// [`f32::NEG_INFINITY`] when the interval contains only silence.
    pub peak_db: f32,
    /// RMS amplitude in dBFS (`sqrt(mean(s²))` over all samples).
    /// [`f32::NEG_INFINITY`] when the interval contains only silence.
    pub rms_db: f32,
}

/// Computes peak and RMS amplitude per time interval for an audio file.
///
/// Decodes audio via [`AudioDecoder`] (requesting packed `f32` output so that
/// per-sample arithmetic needs no format dispatch) and computes, for each
/// configurable interval, the peak and RMS amplitudes in dBFS.  The resulting
/// [`Vec<WaveformSample>`] is designed for waveform display rendering.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::WaveformAnalyzer;
/// use std::time::Duration;
///
/// let samples = WaveformAnalyzer::new("audio.mp3")
///     .interval(Duration::from_millis(50))
///     .run()?;
///
/// for s in &samples {
///     println!("{:?}: peak={:.1} dBFS  rms={:.1} dBFS",
///              s.timestamp, s.peak_db, s.rms_db);
/// }
/// ```
pub struct WaveformAnalyzer {
    input: PathBuf,
    interval: Duration,
}

impl WaveformAnalyzer {
    /// Creates a new analyzer for the given audio file.
    ///
    /// The default sampling interval is 100 ms.  Call
    /// [`interval`](Self::interval) to override it.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            interval: Duration::from_millis(100),
        }
    }

    /// Sets the sampling interval.
    ///
    /// Peak and RMS are computed independently for each interval of this
    /// length.  Passing [`Duration::ZERO`] causes [`run`](Self::run) to
    /// return [`DecodeError::AnalysisFailed`].
    ///
    /// Default: 100 ms.
    #[must_use]
    pub fn interval(mut self, d: Duration) -> Self {
        self.interval = d;
        self
    }

    /// Runs the waveform analysis and returns one [`WaveformSample`] per interval.
    ///
    /// The timestamp of each sample is the **start** of its interval.  Audio
    /// is decoded as packed `f32` samples; the decoder performs any necessary
    /// format conversion automatically.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — interval is [`Duration::ZERO`].
    /// - [`DecodeError::FileNotFound`] — input path does not exist.
    /// - Any other [`DecodeError`] propagated from [`AudioDecoder`].
    pub fn run(self) -> Result<Vec<WaveformSample>, DecodeError> {
        if self.interval.is_zero() {
            return Err(DecodeError::AnalysisFailed {
                reason: "interval must be non-zero".to_string(),
            });
        }

        let mut decoder = AudioDecoder::open(&self.input)
            .output_format(SampleFormat::F32)
            .build()?;

        let mut results: Vec<WaveformSample> = Vec::new();
        let mut interval_start = Duration::ZERO;
        let mut bucket: Vec<f32> = Vec::new();

        while let Some(frame) = decoder.decode_one()? {
            let frame_start = frame.timestamp().as_duration();

            // Flush all completed intervals that end before this frame begins.
            while frame_start >= interval_start + self.interval {
                if bucket.is_empty() {
                    results.push(WaveformSample {
                        timestamp: interval_start,
                        peak_db: f32::NEG_INFINITY,
                        rms_db: f32::NEG_INFINITY,
                    });
                } else {
                    results.push(waveform_sample_from_bucket(interval_start, &bucket));
                    bucket.clear();
                }
                interval_start += self.interval;
            }

            if let Some(samples) = frame.as_f32() {
                bucket.extend_from_slice(samples);
            }
        }

        // Flush the final partial interval.
        if !bucket.is_empty() {
            results.push(waveform_sample_from_bucket(interval_start, &bucket));
        }

        log::debug!("waveform analysis complete samples={}", results.len());
        Ok(results)
    }
}

// ── SceneDetector ─────────────────────────────────────────────────────────────

/// Detects scene changes in a video file and returns their timestamps.
///
/// Uses `FFmpeg`'s `select=gt(scene\,threshold)` filter to identify frames
/// where the scene changes.  The `threshold` controls detection sensitivity:
/// lower values detect more cuts (including subtle ones); higher values detect
/// only hard cuts.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::SceneDetector;
///
/// let cuts = SceneDetector::new("video.mp4")
///     .threshold(0.3)
///     .run()?;
///
/// for ts in &cuts {
///     println!("Scene change at {:?}", ts);
/// }
/// ```
pub struct SceneDetector {
    input: PathBuf,
    threshold: f64,
}

impl SceneDetector {
    /// Creates a new detector for the given video file.
    ///
    /// The default detection threshold is `0.4`.  Call
    /// [`threshold`](Self::threshold) to override it.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            threshold: 0.4,
        }
    }

    /// Sets the scene-change detection threshold.
    ///
    /// Must be in the range `[0.0, 1.0]`.  Lower values make the detector more
    /// sensitive (more cuts reported); higher values require a larger visual
    /// difference.  Passing a value outside this range causes
    /// [`run`](Self::run) to return [`DecodeError::AnalysisFailed`].
    ///
    /// Default: `0.4`.
    #[must_use]
    pub fn threshold(self, t: f64) -> Self {
        Self {
            threshold: t,
            ..self
        }
    }

    /// Runs scene-change detection and returns one [`Duration`] per detected cut.
    ///
    /// Timestamps are sorted in ascending order and represent the PTS of the
    /// first frame of each new scene.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — threshold outside `[0.0, 1.0]`,
    ///   input file not found, or an internal filter-graph error.
    pub fn run(self) -> Result<Vec<Duration>, DecodeError> {
        if !(0.0..=1.0).contains(&self.threshold) {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("threshold must be in [0.0, 1.0], got {}", self.threshold),
            });
        }
        if !self.input.exists() {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("file not found: {}", self.input.display()),
            });
        }
        // SAFETY: detect_scenes_unsafe manages all raw pointer lifetimes
        // according to the avfilter ownership rules: the graph is allocated with
        // avfilter_graph_alloc(), built and configured, drained, then freed before
        // returning.  The path CString is valid for the duration of the graph build.
        unsafe { analysis_inner::detect_scenes_unsafe(&self.input, self.threshold) }
    }
}

// ── SilenceDetector ──────────────────────────────────────────────────────────

/// A detected silent interval in an audio stream.
///
/// Both timestamps are measured from the beginning of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct SilenceRange {
    /// Start of the silent interval.
    pub start: Duration,
    /// End of the silent interval.
    pub end: Duration,
}

/// Detects silent intervals in an audio file and returns their time ranges.
///
/// Uses `FFmpeg`'s `silencedetect` filter to identify audio segments whose
/// amplitude stays below `threshold_db` for at least `min_duration`.  Only
/// complete intervals (silence start **and** end detected) are reported; a
/// trailing silence that runs to end-of-file without an explicit end marker is
/// not included.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::SilenceDetector;
/// use std::time::Duration;
///
/// let ranges = SilenceDetector::new("audio.mp3")
///     .threshold_db(-40.0)
///     .min_duration(Duration::from_millis(500))
///     .run()?;
///
/// for r in &ranges {
///     println!("Silence {:?}–{:?}", r.start, r.end);
/// }
/// ```
pub struct SilenceDetector {
    input: PathBuf,
    threshold_db: f32,
    min_duration: Duration,
}

impl SilenceDetector {
    /// Creates a new detector for the given audio file.
    ///
    /// Defaults: `threshold_db = -40.0`, `min_duration = 500 ms`.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            threshold_db: -40.0,
            min_duration: Duration::from_millis(500),
        }
    }

    /// Sets the amplitude threshold in dBFS.
    ///
    /// Audio samples below this level are considered silent.  The value should
    /// be negative (e.g. `-40.0` for −40 dBFS).
    ///
    /// Default: `-40.0` dB.
    #[must_use]
    pub fn threshold_db(self, db: f32) -> Self {
        Self {
            threshold_db: db,
            ..self
        }
    }

    /// Sets the minimum duration a silent segment must last to be reported.
    ///
    /// Silence shorter than this value is ignored.
    ///
    /// Default: 500 ms.
    #[must_use]
    pub fn min_duration(self, d: Duration) -> Self {
        Self {
            min_duration: d,
            ..self
        }
    }

    /// Runs silence detection and returns all detected [`SilenceRange`] values.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — input file not found or an internal
    ///   filter-graph error occurs.
    pub fn run(self) -> Result<Vec<SilenceRange>, DecodeError> {
        if !self.input.exists() {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("file not found: {}", self.input.display()),
            });
        }
        // SAFETY: detect_silence_unsafe manages all raw pointer lifetimes
        // according to the avfilter ownership rules: the graph is allocated with
        // avfilter_graph_alloc(), built and configured, drained, then freed before
        // returning.  The path CString is valid for the duration of the graph build.
        unsafe {
            analysis_inner::detect_silence_unsafe(&self.input, self.threshold_db, self.min_duration)
        }
    }
}

// ── KeyframeEnumerator ────────────────────────────────────────────────────────

/// Enumerates the timestamps of all keyframes in a video stream.
///
/// Reads only packet headers — **no decoding is performed** — making this
/// significantly faster than frame-by-frame decoding.  By default the first
/// video stream is selected; call [`stream_index`](Self::stream_index) to
/// target a specific stream.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::KeyframeEnumerator;
///
/// let keyframes = KeyframeEnumerator::new("video.mp4").run()?;
/// for ts in &keyframes {
///     println!("Keyframe at {:?}", ts);
/// }
/// ```
pub struct KeyframeEnumerator {
    input: PathBuf,
    stream_index: Option<usize>,
}

impl KeyframeEnumerator {
    /// Creates a new enumerator for the given video file.
    ///
    /// The first video stream is used by default.  Call
    /// [`stream_index`](Self::stream_index) to select a different stream.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            stream_index: None,
        }
    }

    /// Selects a specific stream by zero-based index.
    ///
    /// When not set (the default), the first video stream in the file is used.
    #[must_use]
    pub fn stream_index(self, idx: usize) -> Self {
        Self {
            stream_index: Some(idx),
            ..self
        }
    }

    /// Enumerates keyframe timestamps and returns them in presentation order.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — input file not found, no video
    ///   stream exists, the requested stream index is out of range, or an
    ///   internal `FFmpeg` error occurs.
    pub fn run(self) -> Result<Vec<Duration>, DecodeError> {
        if !self.input.exists() {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("file not found: {}", self.input.display()),
            });
        }
        // SAFETY: enumerate_keyframes_unsafe manages all raw pointer lifetimes:
        // - avformat_open_input / avformat_close_input own the format context.
        // - av_packet_alloc / av_packet_free own the packet.
        // - av_packet_unref is called after every av_read_frame success.
        unsafe { analysis_inner::enumerate_keyframes_unsafe(&self.input, self.stream_index) }
    }
}

// ── HistogramExtractor ────────────────────────────────────────────────────────

/// Per-channel color histogram for a single video frame.
///
/// Each array has 256 bins (one per 8-bit intensity level).  For an `N × M`
/// frame the sum of any channel's bins equals `N × M`.
///
/// Luma is computed as `Y = 0.299 R + 0.587 G + 0.114 B` (BT.601 coefficients).
#[derive(Debug, Clone)]
pub struct FrameHistogram {
    /// Presentation timestamp of the sampled frame.
    pub timestamp: Duration,
    /// Red-channel bin counts.
    pub r: [u32; 256],
    /// Green-channel bin counts.
    pub g: [u32; 256],
    /// Blue-channel bin counts.
    pub b: [u32; 256],
    /// Luma bin counts (BT.601 weighted average of R, G, B).
    pub luma: [u32; 256],
}

/// Extracts per-channel color histograms at configurable frame intervals.
///
/// Decodes the input video via [`VideoDecoder`] with `RGB24` output conversion
/// so that histogram accumulation is a simple one-pass loop with no additional
/// format dispatch.  `FFmpeg`'s `histogram` filter is deliberately **not** used
/// because it produces video output rather than structured data.
///
/// # Examples
///
/// ```ignore
/// use ff_decode::HistogramExtractor;
///
/// let histograms = HistogramExtractor::new("video.mp4")
///     .interval_frames(30)
///     .run()?;
///
/// for h in &histograms {
///     println!("Frame at {:?}: r[255]={}", h.timestamp, h.r[255]);
/// }
/// ```
pub struct HistogramExtractor {
    input: PathBuf,
    interval_frames: u32,
}

impl HistogramExtractor {
    /// Creates a new extractor for the given video file.
    ///
    /// The default sampling interval is every frame (`interval_frames = 1`).
    /// Call [`interval_frames`](Self::interval_frames) to sample less frequently.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
            interval_frames: 1,
        }
    }

    /// Sets the frame sampling interval.
    ///
    /// A value of `N` means one histogram is computed per `N` decoded frames.
    /// For example, `interval_frames(30)` on a 30 fps video yields roughly one
    /// histogram per second.
    ///
    /// Passing `0` causes [`run`](Self::run) to return
    /// [`DecodeError::AnalysisFailed`].
    ///
    /// Default: `1` (every frame).
    #[must_use]
    pub fn interval_frames(self, n: u32) -> Self {
        Self {
            interval_frames: n,
            ..self
        }
    }

    /// Runs histogram extraction and returns one [`FrameHistogram`] per
    /// sampled frame.
    ///
    /// Frames are decoded as RGB24 internally; all pixel format conversion is
    /// handled by `FFmpeg`'s software scaler.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::AnalysisFailed`] — `interval_frames` is `0`, the input
    ///   file is not found, or a decode error occurs.
    /// - Any [`DecodeError`] propagated from [`VideoDecoder`].
    pub fn run(self) -> Result<Vec<FrameHistogram>, DecodeError> {
        if self.interval_frames == 0 {
            return Err(DecodeError::AnalysisFailed {
                reason: "interval_frames must be non-zero".to_string(),
            });
        }
        if !self.input.exists() {
            return Err(DecodeError::AnalysisFailed {
                reason: format!("file not found: {}", self.input.display()),
            });
        }

        let mut decoder = VideoDecoder::open(&self.input)
            .output_format(PixelFormat::Rgb24)
            .build()?;

        let mut results: Vec<FrameHistogram> = Vec::new();
        let mut frame_index: u32 = 0;

        while let Some(frame) = decoder.decode_one()? {
            if frame_index.is_multiple_of(self.interval_frames)
                && let Some(hist) = compute_rgb24_histogram(&frame)
            {
                results.push(hist);
            }
            frame_index += 1;
        }

        log::debug!("histogram extraction complete frames={}", results.len());
        Ok(results)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Computes R, G, B, and luma histograms for a single `RGB24` frame.
///
/// Returns `None` when the frame is not `RGB24` or when plane data is
/// unavailable.
fn compute_rgb24_histogram(frame: &ff_format::VideoFrame) -> Option<FrameHistogram> {
    if frame.format() != PixelFormat::Rgb24 {
        return None;
    }
    let width = frame.width() as usize;
    let height = frame.height() as usize;
    let plane = frame.plane(0)?;
    let stride = frame.stride(0)?;

    let mut r = [0u32; 256];
    let mut g = [0u32; 256];
    let mut b = [0u32; 256];
    let mut luma = [0u32; 256];

    for row in 0..height {
        let row_start = row * stride;
        for col in 0..width {
            let offset = row_start + col * 3;
            let rv = plane[offset];
            let gv = plane[offset + 1];
            let bv = plane[offset + 2];
            // f32 can represent all u8 values exactly (mantissa is 23 bits, u8 needs only 8).
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let lv = (0.299_f32
                .mul_add(
                    f32::from(rv),
                    0.587_f32.mul_add(f32::from(gv), 0.114 * f32::from(bv)),
                )
                .round() as usize)
                .min(255);
            r[usize::from(rv)] += 1;
            g[usize::from(gv)] += 1;
            b[usize::from(bv)] += 1;
            luma[lv] += 1;
        }
    }

    Some(FrameHistogram {
        timestamp: frame.timestamp().as_duration(),
        r,
        g,
        b,
        luma,
    })
}

/// Builds a [`WaveformSample`] from the raw `f32` PCM values accumulated for
/// one interval.
#[allow(clippy::cast_precision_loss)] // sample count fits comfortably in f32
fn waveform_sample_from_bucket(timestamp: Duration, samples: &[f32]) -> WaveformSample {
    let peak = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max);

    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = mean_sq.sqrt();

    WaveformSample {
        timestamp,
        peak_db: amplitude_to_db(peak),
        rms_db: amplitude_to_db(rms),
    }
}

/// Converts a linear amplitude (0.0–1.0) to dBFS.
///
/// Zero and negative amplitudes map to [`f32::NEG_INFINITY`].
fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * amplitude.log10()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amplitude_to_db_zero_should_be_neg_infinity() {
        assert_eq!(amplitude_to_db(0.0), f32::NEG_INFINITY);
    }

    #[test]
    fn amplitude_to_db_full_scale_should_be_zero_db() {
        let db = amplitude_to_db(1.0);
        assert!(
            (db - 0.0).abs() < 1e-5,
            "expected ~0 dBFS for full-scale amplitude, got {db}"
        );
    }

    #[test]
    fn amplitude_to_db_half_amplitude_should_be_about_minus_6db() {
        let db = amplitude_to_db(0.5);
        assert!(
            (db - (-6.020_6)).abs() < 0.01,
            "expected ~-6 dBFS for 0.5 amplitude, got {db}"
        );
    }

    #[test]
    fn waveform_analyzer_zero_interval_should_return_analysis_failed() {
        let result = WaveformAnalyzer::new("irrelevant.mp3")
            .interval(Duration::ZERO)
            .run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed, got {result:?}"
        );
    }

    #[test]
    fn keyframe_enumerator_missing_file_should_return_analysis_failed() {
        let result = KeyframeEnumerator::new("does_not_exist_99999.mp4").run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing file, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_invalid_threshold_below_zero_should_return_analysis_failed() {
        let result = SceneDetector::new("irrelevant.mp4").threshold(-0.1).run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for threshold=-0.1, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_invalid_threshold_above_one_should_return_analysis_failed() {
        let result = SceneDetector::new("irrelevant.mp4").threshold(1.1).run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for threshold=1.1, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_missing_file_should_return_analysis_failed() {
        let result = SceneDetector::new("does_not_exist_99999.mp4").run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing file, got {result:?}"
        );
    }

    #[test]
    fn scene_detector_boundary_thresholds_should_be_valid() {
        // 0.0 and 1.0 are valid thresholds (boundary-inclusive check).
        // They return errors only for missing file, not for bad threshold.
        let r0 = SceneDetector::new("irrelevant.mp4").threshold(0.0).run();
        let r1 = SceneDetector::new("irrelevant.mp4").threshold(1.0).run();
        // Both should fail with AnalysisFailed (file not found), NOT threshold error.
        assert!(
            matches!(r0, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed (file), got {r0:?}"
        );
        assert!(
            matches!(r1, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed (file), got {r1:?}"
        );
    }

    #[test]
    fn waveform_analyzer_nonexistent_file_should_return_file_not_found() {
        let result = WaveformAnalyzer::new("does_not_exist_12345.mp3").run();
        assert!(
            matches!(result, Err(DecodeError::FileNotFound { .. })),
            "expected FileNotFound, got {result:?}"
        );
    }

    #[test]
    fn waveform_analyzer_silence_should_have_low_amplitude() {
        let silent: Vec<f32> = vec![0.0; 4800];
        let sample = waveform_sample_from_bucket(Duration::ZERO, &silent);
        assert!(
            sample.peak_db.is_infinite() && sample.peak_db.is_sign_negative(),
            "expected -infinity peak_db for all-zero samples, got {}",
            sample.peak_db
        );
        assert!(
            sample.rms_db.is_infinite() && sample.rms_db.is_sign_negative(),
            "expected -infinity rms_db for all-zero samples, got {}",
            sample.rms_db
        );
    }

    #[test]
    fn silence_detector_missing_file_should_return_analysis_failed() {
        let result = SilenceDetector::new("does_not_exist_99999.mp3").run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing file, got {result:?}"
        );
    }

    #[test]
    fn silence_detector_default_threshold_should_be_minus_40_db() {
        // Verify the default is -40 dB by round-tripping through threshold_db().
        // Setting the same value should not change behaviour.
        let result = SilenceDetector::new("does_not_exist_99999.mp3")
            .threshold_db(-40.0)
            .run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed (missing file) when threshold_db=-40, got {result:?}"
        );
    }

    #[test]
    fn histogram_extractor_missing_file_should_return_analysis_failed() {
        let result = HistogramExtractor::new("does_not_exist_99999.mp4").run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing file, got {result:?}"
        );
    }

    #[test]
    fn histogram_extractor_zero_interval_should_return_analysis_failed() {
        let result = HistogramExtractor::new("irrelevant.mp4")
            .interval_frames(0)
            .run();
        assert!(
            matches!(result, Err(DecodeError::AnalysisFailed { .. })),
            "expected AnalysisFailed for interval_frames=0, got {result:?}"
        );
    }

    #[test]
    fn histogram_solid_red_frame_should_have_r255_peak() {
        use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};

        let w = 4u32;
        let h = 4u32;
        let stride = w as usize * 3;
        // Solid red: R=255, G=0, B=0.
        let mut data = vec![0u8; stride * h as usize];
        for pixel in data.chunks_mut(3) {
            pixel[0] = 255;
        }
        let frame = VideoFrame::new(
            vec![PooledBuffer::standalone(data)],
            vec![stride],
            w,
            h,
            PixelFormat::Rgb24,
            Timestamp::default(),
            false,
        )
        .unwrap();

        let hist = compute_rgb24_histogram(&frame).unwrap();
        let total = w * h;
        assert_eq!(
            hist.r[255], total,
            "r[255] should equal total pixels for solid-red frame"
        );
        assert_eq!(
            hist.g[0], total,
            "g[0] should equal total pixels for solid-red frame"
        );
        assert_eq!(
            hist.b[0], total,
            "b[0] should equal total pixels for solid-red frame"
        );
    }

    #[test]
    fn histogram_bin_sum_should_equal_total_pixels() {
        use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};

        let w = 8u32;
        let h = 6u32;
        let stride = w as usize * 3;
        let mut data = vec![0u8; stride * h as usize];
        for (i, pixel) in data.chunks_mut(3).enumerate() {
            pixel[0] = (i.wrapping_mul(17) % 256) as u8;
            pixel[1] = (i.wrapping_mul(37) % 256) as u8;
            pixel[2] = (i.wrapping_mul(53) % 256) as u8;
        }
        let frame = VideoFrame::new(
            vec![PooledBuffer::standalone(data)],
            vec![stride],
            w,
            h,
            PixelFormat::Rgb24,
            Timestamp::default(),
            false,
        )
        .unwrap();

        let hist = compute_rgb24_histogram(&frame).unwrap();
        let total = w * h;
        assert_eq!(
            hist.r.iter().sum::<u32>(),
            total,
            "r bin sum should equal total pixels"
        );
        assert_eq!(
            hist.g.iter().sum::<u32>(),
            total,
            "g bin sum should equal total pixels"
        );
        assert_eq!(
            hist.b.iter().sum::<u32>(),
            total,
            "b bin sum should equal total pixels"
        );
        assert_eq!(
            hist.luma.iter().sum::<u32>(),
            total,
            "luma bin sum should equal total pixels"
        );
    }
}
