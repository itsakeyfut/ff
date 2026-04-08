//! Video noise reduction and deinterlace filter methods for [`FilterGraphBuilder`].

#[allow(clippy::wildcard_imports)]
use super::*;

impl FilterGraphBuilder {
    /// Apply a Gaussian blur with the given `sigma` (blur radius).
    ///
    /// `sigma` controls the standard deviation of the Gaussian kernel.
    /// Values near `0.0` are nearly a no-op; values up to `10.0` produce
    /// progressively stronger blur.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if
    /// `sigma` is negative.
    #[must_use]
    pub fn gblur(mut self, sigma: f32) -> Self {
        self.steps.push(FilterStep::GBlur { sigma });
        self
    }

    /// Sharpen or blur the image using an unsharp mask on luma and chroma.
    ///
    /// Positive values sharpen; negative values blur. Pass `0.0` for either
    /// channel to leave it unchanged.
    ///
    /// Valid ranges: `luma_strength` and `chroma_strength` each −1.5 – 1.5.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if either
    /// value is outside `[−1.5, 1.5]`.
    #[must_use]
    pub fn unsharp(mut self, luma_strength: f32, chroma_strength: f32) -> Self {
        self.steps.push(FilterStep::Unsharp {
            luma_strength,
            chroma_strength,
        });
        self
    }

    /// Apply High Quality 3D (`hqdn3d`) noise reduction.
    ///
    /// Typical values: `luma_spatial=4.0`, `chroma_spatial=3.0`,
    /// `luma_tmp=6.0`, `chroma_tmp=4.5`. All values must be ≥ 0.0.
    ///
    /// # Validation
    ///
    /// [`build`](Self::build) returns [`FilterError::InvalidConfig`] if any
    /// value is negative.
    #[must_use]
    pub fn hqdn3d(
        mut self,
        luma_spatial: f32,
        chroma_spatial: f32,
        luma_tmp: f32,
        chroma_tmp: f32,
    ) -> Self {
        self.steps.push(FilterStep::Hqdn3d {
            luma_spatial,
            chroma_spatial,
            luma_tmp,
            chroma_tmp,
        });
        self
    }

    /// Apply non-local means (`nlmeans`) noise reduction.
    ///
    /// `strength` controls denoising intensity; range 1.0–30.0.
    /// Higher values remove more noise at the cost of significantly more CPU.
    ///
    /// NOTE: nlmeans is CPU-intensive; avoid for real-time pipelines.
    #[must_use]
    pub fn nlmeans(mut self, strength: f32) -> Self {
        self.steps.push(FilterStep::Nlmeans { strength });
        self
    }

    /// Deinterlace using the `yadif` (Yet Another Deinterlacing Filter).
    ///
    /// `mode` controls whether one frame or two fields are emitted per input
    /// frame and whether the spatial interlacing check is enabled.
    #[must_use]
    pub fn yadif(mut self, mode: YadifMode) -> Self {
        self.steps.push(FilterStep::Yadif { mode });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_step_gblur_should_produce_correct_filter_name() {
        let step = FilterStep::GBlur { sigma: 5.0 };
        assert_eq!(step.filter_name(), "gblur");
    }

    #[test]
    fn filter_step_gblur_should_produce_correct_args() {
        let step = FilterStep::GBlur { sigma: 5.0 };
        assert_eq!(step.args(), "sigma=5");
    }

    #[test]
    fn filter_step_gblur_small_sigma_should_produce_correct_args() {
        let step = FilterStep::GBlur { sigma: 0.1 };
        assert_eq!(step.args(), "sigma=0.1");
    }

    #[test]
    fn builder_gblur_with_valid_sigma_should_succeed() {
        let result = FilterGraph::builder().gblur(5.0).build();
        assert!(
            result.is_ok(),
            "gblur(5.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_gblur_with_zero_sigma_should_succeed() {
        let result = FilterGraph::builder().gblur(0.0).build();
        assert!(
            result.is_ok(),
            "gblur(0.0) must build successfully (no-op), got {result:?}"
        );
    }

    #[test]
    fn builder_gblur_with_negative_sigma_should_return_invalid_config() {
        let result = FilterGraph::builder().gblur(-1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for sigma < 0.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("sigma"),
                "reason should mention sigma: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_unsharp_should_produce_correct_filter_name() {
        let step = FilterStep::Unsharp {
            luma_strength: 1.0,
            chroma_strength: 0.0,
        };
        assert_eq!(step.filter_name(), "unsharp");
    }

    #[test]
    fn filter_step_unsharp_should_produce_correct_args() {
        let step = FilterStep::Unsharp {
            luma_strength: 1.0,
            chroma_strength: 0.5,
        };
        let args = step.args();
        assert!(
            args.contains("luma_amount=1") && args.contains("chroma_amount=0.5"),
            "args must contain luma and chroma amounts: {args}"
        );
        assert!(
            args.contains("luma_msize_x=5") && args.contains("luma_msize_y=5"),
            "args must contain luma matrix size: {args}"
        );
        assert!(
            args.contains("chroma_msize_x=5") && args.contains("chroma_msize_y=5"),
            "args must contain chroma matrix size: {args}"
        );
    }

    #[test]
    fn builder_unsharp_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().unsharp(1.0, 0.0).build();
        assert!(
            result.is_ok(),
            "unsharp(1.0, 0.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_unsharp_with_negative_luma_should_succeed() {
        let result = FilterGraph::builder().unsharp(-1.0, 0.0).build();
        assert!(
            result.is_ok(),
            "unsharp(-1.0, 0.0) (blur) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_unsharp_with_luma_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().unsharp(2.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for luma_strength > 1.5, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("luma_strength"),
                "reason should mention luma_strength: {reason}"
            );
        }
    }

    #[test]
    fn builder_unsharp_with_luma_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().unsharp(-2.0, 0.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for luma_strength < -1.5, got {result:?}"
        );
    }

    #[test]
    fn builder_unsharp_with_chroma_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().unsharp(0.0, 2.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for chroma_strength > 1.5, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("chroma_strength"),
                "reason should mention chroma_strength: {reason}"
            );
        }
    }

    #[test]
    fn filter_step_hqdn3d_should_produce_correct_filter_name() {
        let step = FilterStep::Hqdn3d {
            luma_spatial: 4.0,
            chroma_spatial: 3.0,
            luma_tmp: 6.0,
            chroma_tmp: 4.5,
        };
        assert_eq!(step.filter_name(), "hqdn3d");
    }

    #[test]
    fn filter_step_hqdn3d_should_produce_correct_args() {
        let step = FilterStep::Hqdn3d {
            luma_spatial: 4.0,
            chroma_spatial: 3.0,
            luma_tmp: 6.0,
            chroma_tmp: 4.5,
        };
        assert_eq!(step.args(), "4:3:6:4.5");
    }

    #[test]
    fn builder_hqdn3d_with_valid_params_should_succeed() {
        let result = FilterGraph::builder().hqdn3d(4.0, 3.0, 6.0, 4.5).build();
        assert!(
            result.is_ok(),
            "hqdn3d(4.0, 3.0, 6.0, 4.5) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_zero_params_should_succeed() {
        let result = FilterGraph::builder().hqdn3d(0.0, 0.0, 0.0, 0.0).build();
        assert!(
            result.is_ok(),
            "hqdn3d(0.0, 0.0, 0.0, 0.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_negative_luma_spatial_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(-1.0, 3.0, 6.0, 4.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative luma_spatial, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("luma_spatial"),
                "reason should mention luma_spatial: {reason}"
            );
        }
    }

    #[test]
    fn builder_hqdn3d_with_negative_chroma_spatial_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(4.0, -1.0, 6.0, 4.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative chroma_spatial, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_negative_luma_tmp_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(4.0, 3.0, -1.0, 4.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative luma_tmp, got {result:?}"
        );
    }

    #[test]
    fn builder_hqdn3d_with_negative_chroma_tmp_should_return_invalid_config() {
        let result = FilterGraph::builder().hqdn3d(4.0, 3.0, 6.0, -1.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for negative chroma_tmp, got {result:?}"
        );
    }

    #[test]
    fn filter_step_nlmeans_should_produce_correct_filter_name() {
        let step = FilterStep::Nlmeans { strength: 8.0 };
        assert_eq!(step.filter_name(), "nlmeans");
    }

    #[test]
    fn filter_step_nlmeans_should_produce_correct_args() {
        let step = FilterStep::Nlmeans { strength: 8.0 };
        assert_eq!(step.args(), "s=8");
    }

    #[test]
    fn builder_nlmeans_with_valid_strength_should_succeed() {
        let result = FilterGraph::builder().nlmeans(8.0).build();
        assert!(
            result.is_ok(),
            "nlmeans(8.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_nlmeans_with_min_strength_should_succeed() {
        let result = FilterGraph::builder().nlmeans(1.0).build();
        assert!(
            result.is_ok(),
            "nlmeans(1.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_nlmeans_with_max_strength_should_succeed() {
        let result = FilterGraph::builder().nlmeans(30.0).build();
        assert!(
            result.is_ok(),
            "nlmeans(30.0) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_nlmeans_with_strength_too_low_should_return_invalid_config() {
        let result = FilterGraph::builder().nlmeans(0.5).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for strength < 1.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("strength"),
                "reason should mention strength: {reason}"
            );
        }
    }

    #[test]
    fn builder_nlmeans_with_strength_too_high_should_return_invalid_config() {
        let result = FilterGraph::builder().nlmeans(31.0).build();
        assert!(
            matches!(result, Err(FilterError::InvalidConfig { .. })),
            "expected InvalidConfig for strength > 30.0, got {result:?}"
        );
        if let Err(FilterError::InvalidConfig { reason }) = result {
            assert!(
                reason.contains("strength"),
                "reason should mention strength: {reason}"
            );
        }
    }

    #[test]
    fn yadif_mode_variants_should_have_correct_discriminants() {
        assert_eq!(YadifMode::Frame as i32, 0);
        assert_eq!(YadifMode::Field as i32, 1);
        assert_eq!(YadifMode::FrameNospatial as i32, 2);
        assert_eq!(YadifMode::FieldNospatial as i32, 3);
    }

    #[test]
    fn filter_step_yadif_should_produce_correct_filter_name() {
        let step = FilterStep::Yadif {
            mode: YadifMode::Frame,
        };
        assert_eq!(step.filter_name(), "yadif");
    }

    #[test]
    fn filter_step_yadif_frame_should_produce_mode_0_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::Frame,
        };
        assert_eq!(step.args(), "mode=0");
    }

    #[test]
    fn filter_step_yadif_field_should_produce_mode_1_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::Field,
        };
        assert_eq!(step.args(), "mode=1");
    }

    #[test]
    fn filter_step_yadif_frame_nospatial_should_produce_mode_2_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::FrameNospatial,
        };
        assert_eq!(step.args(), "mode=2");
    }

    #[test]
    fn filter_step_yadif_field_nospatial_should_produce_mode_3_args() {
        let step = FilterStep::Yadif {
            mode: YadifMode::FieldNospatial,
        };
        assert_eq!(step.args(), "mode=3");
    }

    #[test]
    fn builder_yadif_with_frame_mode_should_succeed() {
        let result = FilterGraph::builder().yadif(YadifMode::Frame).build();
        assert!(
            result.is_ok(),
            "yadif(Frame) must build successfully, got {result:?}"
        );
    }

    #[test]
    fn builder_yadif_with_all_modes_should_succeed() {
        for mode in [
            YadifMode::Frame,
            YadifMode::Field,
            YadifMode::FrameNospatial,
            YadifMode::FieldNospatial,
        ] {
            let result = FilterGraph::builder().yadif(mode).build();
            assert!(
                result.is_ok(),
                "yadif({mode:?}) must build successfully, got {result:?}"
            );
        }
    }
}
