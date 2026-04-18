//! Frame-level audio effects added to [`FilterGraph`] after construction.

use std::path::Path;

use crate::error::FilterError;
use crate::graph::FilterGraph;
use crate::graph::filter_step::FilterStep;

impl FilterGraph {
    /// Shift audio pitch by `semitones` without changing playback speed.
    ///
    /// Range: −12.0 to +12.0 semitones. Uses `asetrate` to change the
    /// decoded sample rate followed by `atempo` to restore original duration.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if `semitones` is outside −12.0..=12.0.
    pub fn pitch_shift(&mut self, semitones: f32) -> Result<&mut Self, FilterError> {
        if !(-12.0..=12.0).contains(&semitones) {
            return Err(FilterError::Ffmpeg {
                code: 0,
                message: format!("semitones must be in -12..=12, got {semitones}"),
            });
        }
        self.inner.push_step(FilterStep::PitchShift { semitones });
        Ok(self)
    }

    /// Add algorithmic echo/reverb with configurable delay taps.
    ///
    /// `in_gain` and `out_gain` are amplitude multipliers clamped to [0.0, 1.0].
    /// `delays` is a list of delay times in milliseconds.
    /// `decays` is the corresponding decay factor for each delay, clamped to [0.0, 1.0].
    /// `delays` and `decays` must have equal length (1–8 taps).
    ///
    /// Uses `FFmpeg`'s `aecho` filter.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if lengths differ or tap count is outside 1–8.
    pub fn reverb_echo(
        &mut self,
        in_gain: f32,
        out_gain: f32,
        delays: &[f32],
        decays: &[f32],
    ) -> Result<&mut Self, FilterError> {
        if delays.len() != decays.len() {
            return Err(FilterError::Ffmpeg {
                code: 0,
                message: "delays and decays must have equal length".into(),
            });
        }
        if !(1..=8).contains(&delays.len()) {
            return Err(FilterError::Ffmpeg {
                code: 0,
                message: format!("tap count must be 1–8, got {}", delays.len()),
            });
        }
        self.inner.push_step(FilterStep::ReverbEcho {
            in_gain: in_gain.clamp(0.0, 1.0),
            out_gain: out_gain.clamp(0.0, 1.0),
            delays: delays.to_vec(),
            decays: decays.iter().map(|d| d.clamp(0.0, 1.0)).collect(),
        });
        Ok(self)
    }

    /// Add convolution reverb using an impulse response (IR) audio file.
    ///
    /// `ir_path` is a path to a `.wav` or `.flac` impulse response file.
    /// `wet` and `dry` are mix levels clamped to [0.0, 1.0].
    /// `pre_delay_ms` inserts silence before the reverb tail (clamped to 0–500 ms).
    ///
    /// Uses `FFmpeg`'s `amovie` (to load the IR) and `afir` (convolution) filters.
    ///
    /// Call this method after [`FilterGraph::builder()`] /
    /// [`FilterGraphBuilder::build()`](crate::FilterGraphBuilder::build) but
    /// **before** the first [`FilterGraph::push_audio()`] call.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if `ir_path` does not exist.
    /// The `afir` filter availability is checked at graph build time; if not
    /// available the graph build returns [`FilterError::BuildFailed`].
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
    fn pitch_shift_above_range_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.pitch_shift(13.0);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "semitones=13.0 must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn pitch_shift_below_range_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.pitch_shift(-13.0);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "semitones=-13.0 must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn pitch_shift_boundary_values_should_succeed() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        assert!(
            graph.pitch_shift(12.0).is_ok(),
            "semitones=12.0 must succeed"
        );
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        assert!(
            graph.pitch_shift(-12.0).is_ok(),
            "semitones=-12.0 must succeed"
        );
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        assert!(graph.pitch_shift(0.0).is_ok(), "semitones=0.0 must succeed");
    }

    #[test]
    fn filter_step_pitch_shift_should_have_asetrate_filter_name() {
        let step = FilterStep::PitchShift { semitones: 7.0 };
        assert_eq!(step.filter_name(), "asetrate");
    }

    #[test]
    fn reverb_echo_mismatched_lengths_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.reverb_echo(0.8, 0.9, &[500.0], &[0.5, 0.3]);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "mismatched lengths must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn reverb_echo_zero_taps_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.reverb_echo(0.8, 0.9, &[], &[]);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "zero taps must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn reverb_echo_nine_taps_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let delays = vec![100.0; 9];
        let decays = vec![0.5; 9];
        let result = graph.reverb_echo(0.8, 0.9, &delays, &decays);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "nine taps must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn filter_step_reverb_echo_should_have_aecho_filter_name() {
        let step = FilterStep::ReverbEcho {
            in_gain: 0.8,
            out_gain: 0.9,
            delays: vec![500.0],
            decays: vec![0.5],
        };
        assert_eq!(step.filter_name(), "aecho");
    }

    #[test]
    fn reverb_echo_args_should_contain_gains_delays_decays() {
        let step = FilterStep::ReverbEcho {
            in_gain: 0.8,
            out_gain: 0.9,
            delays: vec![500.0],
            decays: vec![0.5],
        };
        let args = step.args();
        assert!(
            args.contains("in_gain=0.8"),
            "args must contain in_gain=0.8: {args}"
        );
        assert!(
            args.contains("out_gain=0.9"),
            "args must contain out_gain=0.9: {args}"
        );
        assert!(
            args.contains("delays=500"),
            "args must contain delays=500: {args}"
        );
        assert!(
            args.contains("decays=0.5"),
            "args must contain decays=0.5: {args}"
        );
    }

    #[test]
    fn reverb_echo_multi_tap_args_should_join_with_pipe() {
        let step = FilterStep::ReverbEcho {
            in_gain: 0.8,
            out_gain: 0.9,
            delays: vec![500.0, 300.0],
            decays: vec![0.5, 0.3],
        };
        let args = step.args();
        assert!(
            args.contains("500|300") || args.contains("500.0|300"),
            "multi-tap delays must be joined with '|': {args}"
        );
        assert!(
            args.contains("0.5|0.3"),
            "multi-tap decays must be joined with '|': {args}"
        );
    }

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
