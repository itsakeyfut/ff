//! Loudness, audio analysis, and video quality metric tools for media files.
//!
//! This module provides:
//! - [`LoudnessMeter`] for EBU R128 integrated loudness, loudness range, and
//!   true peak measurement.
//! - [`QualityMetrics`] for computing video quality metrics (SSIM, PSNR) between
//!   a reference and a distorted video.
//!
//! All `unsafe` `FFmpeg` calls live in `analysis_inner`; users never need to write
//! `unsafe` code.

// The single `unsafe` block below delegates directly to analysis_inner, where
// all invariants are documented.  Suppressing the lint here keeps the safe API
// surface free of noise while still allowing the call.
#![allow(unsafe_code)]

pub(crate) mod analysis_inner;

use std::path::{Path, PathBuf};

use crate::FilterError;

// ── Public types ──────────────────────────────────────────────────────────────

/// Result of an EBU R128 loudness measurement.
///
/// Values are computed by `FFmpeg`'s `ebur128` filter over the entire
/// duration of the input file.
#[derive(Debug, Clone, PartialEq)]
pub struct LoudnessResult {
    /// Integrated loudness in LUFS (ITU-R BS.1770-4).
    ///
    /// [`f32::NEG_INFINITY`] when the audio is silence or the measurement
    /// could not be computed.
    pub integrated_lufs: f32,
    /// Loudness range (LRA) in LU.
    pub lra: f32,
    /// True peak in dBTP.
    ///
    /// [`f32::NEG_INFINITY`] when the measurement could not be computed.
    pub true_peak_dbtp: f32,
}

/// Measures EBU R128 integrated loudness, loudness range, and true peak.
///
/// Uses `FFmpeg`'s `ebur128=metadata=1:peak=true` filter graph internally.
/// The analysis is self-contained — no external decoder is required.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::LoudnessMeter;
///
/// let result = LoudnessMeter::new("audio.mp3").measure()?;
/// println!("Integrated: {:.1} LUFS", result.integrated_lufs);
/// println!("LRA: {:.1} LU", result.lra);
/// println!("True peak: {:.1} dBTP", result.true_peak_dbtp);
/// ```
pub struct LoudnessMeter {
    input: PathBuf,
}

impl LoudnessMeter {
    /// Creates a new meter for the given audio or video file.
    pub fn new(input: impl AsRef<Path>) -> Self {
        Self {
            input: input.as_ref().to_path_buf(),
        }
    }

    /// Runs EBU R128 loudness analysis and returns the result.
    ///
    /// # Errors
    ///
    /// - [`FilterError::AnalysisFailed`] — input file not found, unsupported
    ///   format, or the filter graph could not be constructed.
    pub fn measure(self) -> Result<LoudnessResult, FilterError> {
        if !self.input.exists() {
            return Err(FilterError::AnalysisFailed {
                reason: format!("file not found: {}", self.input.display()),
            });
        }
        // SAFETY: measure_loudness_unsafe manages all raw pointer lifetimes
        // according to the avfilter ownership rules: the graph is allocated with
        // avfilter_graph_alloc(), built and configured, drained, then freed before
        // returning.  The path CString is valid for the duration of the graph build.
        unsafe { analysis_inner::measure_loudness_unsafe(&self.input) }
    }
}

// ── QualityMetrics ────────────────────────────────────────────────────────────

/// Computes video quality metrics between a reference and a distorted video.
///
/// All methods are static — there is no state to configure.
pub struct QualityMetrics;

impl QualityMetrics {
    /// Computes the mean SSIM (Structural Similarity Index Measure) over all
    /// frames between `reference` and `distorted`.
    ///
    /// Returns a value in `[0.0, 1.0]`:
    /// - `1.0` — the inputs are frame-identical.
    /// - `0.0` — no structural similarity.
    ///
    /// Uses `FFmpeg`'s `ssim` filter internally.  Both inputs must have the
    /// same frame count; if they differ the function returns an error rather
    /// than silently comparing only the overlapping portion.
    ///
    /// # Errors
    ///
    /// - [`FilterError::AnalysisFailed`] — either input file is not found, the
    ///   inputs have different frame counts, or the internal filter graph fails.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_filter::QualityMetrics;
    ///
    /// // Compare a video against itself — should return ≈ 1.0.
    /// let ssim = QualityMetrics::ssim("reference.mp4", "reference.mp4")?;
    /// assert!(ssim > 0.9999);
    /// ```
    pub fn ssim(
        reference: impl AsRef<Path>,
        distorted: impl AsRef<Path>,
    ) -> Result<f32, FilterError> {
        let reference = reference.as_ref();
        let distorted = distorted.as_ref();

        if !reference.exists() {
            return Err(FilterError::AnalysisFailed {
                reason: format!("reference file not found: {}", reference.display()),
            });
        }
        if !distorted.exists() {
            return Err(FilterError::AnalysisFailed {
                reason: format!("distorted file not found: {}", distorted.display()),
            });
        }
        // SAFETY: compute_ssim_unsafe manages all raw pointer lifetimes according
        // to the avfilter ownership rules: every allocated object is freed before
        // returning, either in the bail! macro or in the normal cleanup path.
        unsafe { analysis_inner::compute_ssim_unsafe(reference, distorted) }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_meter_missing_file_should_return_analysis_failed() {
        let result = LoudnessMeter::new("does_not_exist_99999.mp3").measure();
        assert!(
            matches!(result, Err(FilterError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing file, got {result:?}"
        );
    }

    #[test]
    fn quality_metrics_ssim_missing_reference_should_return_analysis_failed() {
        let result = QualityMetrics::ssim("does_not_exist_ref.mp4", "does_not_exist_dist.mp4");
        assert!(
            matches!(result, Err(FilterError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing reference, got {result:?}"
        );
    }

    #[test]
    fn quality_metrics_ssim_missing_distorted_should_return_analysis_failed() {
        // Reference exists (any existing file), distorted does not.
        // We reuse the reference path for the reference file check.
        // Use a path that is guaranteed to exist: the Cargo.toml for this crate.
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let result = QualityMetrics::ssim(&manifest, "does_not_exist_dist_99999.mp4");
        assert!(
            matches!(result, Err(FilterError::AnalysisFailed { .. })),
            "expected AnalysisFailed for missing distorted, got {result:?}"
        );
    }
}
