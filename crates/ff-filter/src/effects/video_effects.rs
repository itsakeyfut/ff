//! Frame-level video effects added to [`FilterGraph`] after construction.

use crate::error::FilterError;
use crate::graph::FilterGraph;
use crate::graph::filter_step::FilterStep;

impl FilterGraph {
    /// Simulate motion blur by blending multiple consecutive frames.
    ///
    /// `shutter_angle_degrees` controls the blend ratio (360° = full
    /// frame-period exposure). `sub_frames` sets the number of frames blended
    /// and must be in [2, 16].
    ///
    /// Uses `FFmpeg`'s `tblend` filter with `all_expr`:
    /// the normalised shutter angle becomes the weight for the previous frame
    /// (`B`), and its complement weights the current frame (`A`).
    ///
    /// Call this method after [`FilterGraph::builder()`] / [`build()`] but
    /// **before** the first [`push_video`] call.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if `sub_frames` is outside [2, 16].
    ///
    /// [`build()`]: crate::FilterGraphBuilder::build
    /// [`push_video`]: FilterGraph::push_video
    pub fn motion_blur(
        &mut self,
        shutter_angle_degrees: f32,
        sub_frames: u8,
    ) -> Result<&mut Self, FilterError> {
        if !(2..=16).contains(&sub_frames) {
            return Err(FilterError::Ffmpeg {
                code: 0,
                message: format!("sub_frames must be 2–16, got {sub_frames}"),
            });
        }
        self.inner.push_step(FilterStep::MotionBlur {
            shutter_angle_degrees,
            sub_frames,
        });
        Ok(self)
    }

    /// Correct radial lens distortion using two polynomial coefficients.
    ///
    /// `k1` and `k2` are the first- and second-order radial distortion
    /// coefficients. Negative values correct barrel distortion; positive values
    /// correct pincushion distortion.
    ///
    /// Uses `FFmpeg`'s `lenscorrection` filter.
    ///
    /// Call this method after [`FilterGraph::builder()`] / [`build()`] but
    /// **before** the first [`push_video`] call.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::Ffmpeg`] if either coefficient is outside [−1.0, 1.0].
    ///
    /// [`build()`]: crate::FilterGraphBuilder::build
    /// [`push_video`]: FilterGraph::push_video
    pub fn lens_correction(&mut self, k1: f32, k2: f32) -> Result<&mut Self, FilterError> {
        if !(-1.0..=1.0).contains(&k1) || !(-1.0..=1.0).contains(&k2) {
            return Err(FilterError::Ffmpeg {
                code: 0,
                message: format!("k1/k2 must be in −1.0..=1.0, got k1={k1} k2={k2}"),
            });
        }
        self.inner.push_step(FilterStep::LensCorrection { k1, k2 });
        Ok(self)
    }

    /// Add random per-frame film grain to luma and chroma channels.
    ///
    /// `luma_strength` and `chroma_strength` control grain intensity and are
    /// clamped to [0.0, 100.0]. The `allf=t` flag varies the noise seed each
    /// frame to simulate real film grain temporal variation.
    ///
    /// Uses `FFmpeg`'s `noise` filter with `alls` (luma), `c0s`/`c1s` (Cb/Cr),
    /// and `allf=t` (per-frame seed).
    ///
    /// Call this method after [`FilterGraph::builder()`] / [`build()`] but
    /// **before** the first [`push_video`] call.
    ///
    /// [`build()`]: crate::FilterGraphBuilder::build
    /// [`push_video`]: FilterGraph::push_video
    pub fn film_grain(&mut self, luma_strength: f32, chroma_strength: f32) -> &mut Self {
        self.inner.push_step(FilterStep::FilmGrain {
            luma_strength,
            chroma_strength,
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::graph::filter_step::FilterStep;
    use crate::{FilterError, FilterGraph};

    #[test]
    fn motion_blur_with_valid_params_should_succeed() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.motion_blur(180.0, 2);
        assert!(
            result.is_ok(),
            "motion_blur(180.0, 2) must succeed, got {result:?}"
        );
    }

    #[test]
    fn motion_blur_with_sub_frames_one_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.motion_blur(180.0, 1);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "sub_frames=1 must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn motion_blur_with_sub_frames_seventeen_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.motion_blur(180.0, 17);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "sub_frames=17 must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn filter_step_motion_blur_should_have_tblend_filter_name() {
        let step = FilterStep::MotionBlur {
            shutter_angle_degrees: 180.0,
            sub_frames: 4,
        };
        assert_eq!(step.filter_name(), "tblend");
    }

    #[test]
    fn motion_blur_zero_angle_should_produce_identity_blend_args() {
        let step = FilterStep::MotionBlur {
            shutter_angle_degrees: 0.0,
            sub_frames: 2,
        };
        let args = step.args();
        assert!(
            args.contains("A*1") && args.contains("B*0"),
            "0° shutter angle must produce identity blend (A*1+B*0): {args}"
        );
    }

    #[test]
    fn motion_blur_full_angle_should_produce_full_blend_args() {
        let step = FilterStep::MotionBlur {
            shutter_angle_degrees: 360.0,
            sub_frames: 2,
        };
        let args = step.args();
        assert!(
            args.contains("A*0+B*1"),
            "360° shutter angle must produce full blend (A*0+B*1): {args}"
        );
    }

    #[test]
    fn motion_blur_half_angle_should_produce_equal_blend_args() {
        let step = FilterStep::MotionBlur {
            shutter_angle_degrees: 180.0,
            sub_frames: 2,
        };
        let args = step.args();
        assert!(
            args.contains("A*0.5+B*0.5"),
            "180° shutter angle must produce equal blend (A*0.5+B*0.5): {args}"
        );
    }

    // ── lens_correction ───────────────────────────────────────────────────────

    #[test]
    fn lens_correction_with_valid_coefficients_should_succeed() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.lens_correction(-0.2, 0.0);
        assert!(
            result.is_ok(),
            "lens_correction(-0.2, 0.0) must succeed, got {result:?}"
        );
    }

    #[test]
    fn lens_correction_identity_k1_zero_k2_zero_should_succeed() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.lens_correction(0.0, 0.0);
        assert!(
            result.is_ok(),
            "lens_correction(0.0, 0.0) identity must succeed, got {result:?}"
        );
    }

    #[test]
    fn lens_correction_k1_out_of_range_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.lens_correction(1.5, 0.0);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "k1=1.5 must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn lens_correction_k2_out_of_range_should_return_ffmpeg_error() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.lens_correction(0.0, -1.5);
        assert!(
            matches!(result, Err(FilterError::Ffmpeg { .. })),
            "k2=-1.5 must return Err(FilterError::Ffmpeg {{ .. }}), got {result:?}"
        );
    }

    #[test]
    fn filter_step_lens_correction_should_have_lenscorrection_filter_name() {
        let step = FilterStep::LensCorrection { k1: -0.2, k2: 0.0 };
        assert_eq!(step.filter_name(), "lenscorrection");
    }

    #[test]
    fn lens_correction_args_should_contain_k1_and_k2() {
        let step = FilterStep::LensCorrection { k1: -0.2, k2: 0.1 };
        let args = step.args();
        assert!(
            args.contains("k1=-0.2"),
            "args must contain k1=-0.2: {args}"
        );
        assert!(args.contains("k2=0.1"), "args must contain k2=0.1: {args}");
    }

    // ── film_grain ────────────────────────────────────────────────────────────

    #[test]
    fn film_grain_with_valid_params_should_return_mutable_self() {
        let mut graph = FilterGraph::builder().trim(0.0, 1.0).build().unwrap();
        let result = graph.film_grain(20.0, 5.0);
        // Method returns &mut Self — confirm it compiles and doesn't panic.
        let _ = result;
    }

    #[test]
    fn filter_step_film_grain_should_have_noise_filter_name() {
        let step = FilterStep::FilmGrain {
            luma_strength: 20.0,
            chroma_strength: 5.0,
        };
        assert_eq!(step.filter_name(), "noise");
    }

    #[test]
    fn film_grain_args_should_contain_alls_c0s_c1s_and_allf_t() {
        let step = FilterStep::FilmGrain {
            luma_strength: 20.0,
            chroma_strength: 5.0,
        };
        let args = step.args();
        assert!(
            args.contains("alls=20"),
            "args must contain alls=20: {args}"
        );
        assert!(args.contains("c0s=5"), "args must contain c0s=5: {args}");
        assert!(args.contains("c1s=5"), "args must contain c1s=5: {args}");
        assert!(args.contains("allf=t"), "args must contain allf=t: {args}");
    }

    #[test]
    fn film_grain_zero_strength_should_produce_zero_alls() {
        let step = FilterStep::FilmGrain {
            luma_strength: 0.0,
            chroma_strength: 0.0,
        };
        let args = step.args();
        assert_eq!(args, "alls=0:c0s=0:c1s=0:allf=t");
    }

    #[test]
    fn film_grain_values_above_100_should_be_clamped_to_100() {
        let step = FilterStep::FilmGrain {
            luma_strength: 200.0,
            chroma_strength: 999.0,
        };
        let args = step.args();
        assert!(
            args.contains("alls=100"),
            "luma_strength > 100 must clamp to 100: {args}"
        );
        assert!(
            args.contains("c0s=100") && args.contains("c1s=100"),
            "chroma_strength > 100 must clamp to 100: {args}"
        );
    }

    #[test]
    fn film_grain_negative_values_should_be_clamped_to_zero() {
        let step = FilterStep::FilmGrain {
            luma_strength: -50.0,
            chroma_strength: -10.0,
        };
        let args = step.args();
        assert_eq!(args, "alls=0:c0s=0:c1s=0:allf=t");
    }
}
