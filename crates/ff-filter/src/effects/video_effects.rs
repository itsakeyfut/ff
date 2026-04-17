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
}
