//! Video stabilization — two-pass motion analysis and correction.

#![allow(unsafe_code)]

use std::path::Path;

use crate::FilterError;

/// Options for the first stabilization pass (motion analysis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzeOptions {
    /// Motion shakiness level 1–10 (default: 5).
    pub shakiness: u8,
    /// Detection accuracy 1–15 (default: 15, highest quality).
    pub accuracy: u8,
    /// Step size for motion search in pixels 1–32 (default: 6).
    pub stepsize: u8,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            shakiness: 5,
            accuracy: 15,
            stepsize: 6,
        }
    }
}

/// Two-pass video stabilization using `FFmpeg`'s `vidstabdetect` /
/// `vidstabtransform` filters.
///
/// **Pass 1**: [`Stabilizer::analyze`] — motion analysis, produces a `.trf` file.
/// **Pass 2**: `Stabilizer::transform` (issue #393) — correction, consumes the `.trf` file.
pub struct Stabilizer;

impl Stabilizer {
    /// Analyze motion in `input` and write the transform file to `output_trf`.
    ///
    /// Runs a self-contained `FFmpeg` filter graph:
    /// `movie → vidstabdetect → nullsink`.
    /// The resulting `.trf` file is consumed by `Stabilizer::transform` in pass 2.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if:
    /// - `vidstabdetect` is not available in the linked `FFmpeg` build.
    /// - The input file is unreadable or does not exist.
    /// - The filter graph cannot be configured or the `.trf` file cannot be written.
    pub fn analyze(
        input: &Path,
        output_trf: &Path,
        opts: &AnalyzeOptions,
    ) -> Result<(), FilterError> {
        // SAFETY: analyze_vidstab_unsafe manages all raw pointer lifetimes
        // under the avfilter ownership rules: graph allocated with
        // avfilter_graph_alloc(), built and configured, drained via
        // avfilter_graph_request_oldest(), then freed before returning.
        // All CString values are kept alive for the duration of the graph build.
        unsafe { super::effects_inner::analyze_vidstab_unsafe(input, output_trf, opts) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_options_default_should_have_expected_values() {
        let opts = AnalyzeOptions::default();
        assert_eq!(opts.shakiness, 5);
        assert_eq!(opts.accuracy, 15);
        assert_eq!(opts.stepsize, 6);
    }
}
