//! Frame-level audio effects added to [`FilterGraph`] after construction.

use std::path::Path;

use crate::error::FilterError;
use crate::graph::FilterGraph;
use crate::graph::filter_step::FilterStep;

impl FilterGraph {
    /// Add convolution reverb using an impulse response (IR) audio file.
    ///
    /// `ir_path` is a path to a `.wav` or `.flac` impulse response file.
    /// `wet` and `dry` are mix levels clamped to [0.0, 1.0].
    /// `pre_delay_ms` inserts silence before the reverb tail (clamped to 0–500 ms).
    ///
    /// Uses `FFmpeg`'s `amovie` (to load the IR) and `afir` (convolution) filters.
    ///
    /// Call this method after [`FilterGraph::builder()`] / [`build()`] but
    /// **before** the first [`push_audio`] call.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if `ir_path` does not exist.
    /// The `afir` filter availability is checked at graph build time; if not
    /// available the graph build returns [`FilterError::BuildFailed`].
    ///
    /// [`build()`]: crate::FilterGraphBuilder::build
    /// [`push_audio`]: FilterGraph::push_audio
    pub fn reverb_ir(
        &mut self,
        ir_path: &Path,
        wet: f32,
        dry: f32,
        pre_delay_ms: u32,
    ) -> Result<&mut Self, FilterError> {
        if !ir_path.exists() {
            return Err(FilterError::Ffmpeg {
                code: 0,
                message: format!("ir_path does not exist: {}", ir_path.display()),
            });
        }
        let ir_str = ir_path.display().to_string();
        self.inner.push_step(FilterStep::ReverbIr {
            ir_path: ir_str,
            wet: wet.clamp(0.0, 1.0),
            dry: dry.clamp(0.0, 1.0),
            pre_delay_ms: pre_delay_ms.min(500),
        });
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::filter_step::FilterStep;
    use crate::{FilterError, FilterGraph};
    use std::path::Path;

    #[test]
    fn reverb_ir_nonexistent_path_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.reverb_ir(Path::new("no_such_file.wav"), 0.8, 0.2, 0);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "non-existent ir_path must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn filter_step_reverb_ir_should_have_afir_filter_name() {
        let step = FilterStep::ReverbIr {
            ir_path: "hall.wav".to_string(),
            wet: 0.8,
            dry: 0.2,
            pre_delay_ms: 0,
        };
        assert_eq!(step.filter_name(), "afir");
    }

    #[test]
    fn reverb_ir_args_should_contain_wet_dry_and_ir_path() {
        let step = FilterStep::ReverbIr {
            ir_path: "hall.wav".to_string(),
            wet: 0.8,
            dry: 0.2,
            pre_delay_ms: 0,
        };
        let args = step.args();
        assert!(
            args.contains("hall.wav"),
            "args must contain ir_path: {args}"
        );
        assert!(
            args.contains("wet=0.8"),
            "args must contain wet=0.8: {args}"
        );
        assert!(
            args.contains("dry=0.2"),
            "args must contain dry=0.2: {args}"
        );
        assert!(
            !args.contains("adelay"),
            "no pre-delay when pre_delay_ms=0: {args}"
        );
    }

    #[test]
    fn reverb_ir_args_with_pre_delay_should_contain_adelay() {
        let step = FilterStep::ReverbIr {
            ir_path: "hall.wav".to_string(),
            wet: 0.8,
            dry: 0.2,
            pre_delay_ms: 100,
        };
        let args = step.args();
        assert!(
            args.contains("adelay=100"),
            "args must contain adelay=100 when pre_delay_ms=100: {args}"
        );
    }

    #[test]
    fn reverb_ir_pre_delay_above_500_should_be_clamped() {
        let step = FilterStep::ReverbIr {
            ir_path: "hall.wav".to_string(),
            wet: 0.8,
            dry: 0.2,
            pre_delay_ms: 999,
        };
        let args = step.args();
        assert!(
            args.contains("adelay=500"),
            "pre_delay_ms=999 must clamp to 500: {args}"
        );
    }
}
