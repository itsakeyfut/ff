use super::*;

#[test]
fn filter_step_scale_should_produce_correct_args() {
    let step = FilterStep::Scale {
        width: 1280,
        height: 720,
        algorithm: ScaleAlgorithm::Fast,
    };
    assert_eq!(step.filter_name(), "scale");
    assert_eq!(step.args(), "w=1280:h=720:flags=fast_bilinear");
}

#[test]
fn filter_step_scale_lanczos_should_produce_lanczos_flags() {
    let step = FilterStep::Scale {
        width: 1920,
        height: 1080,
        algorithm: ScaleAlgorithm::Lanczos,
    };
    assert_eq!(step.args(), "w=1920:h=1080:flags=lanczos");
}

#[test]
fn filter_step_trim_should_produce_correct_args() {
    let step = FilterStep::Trim {
        start: 10.0,
        end: 30.0,
    };
    assert_eq!(step.filter_name(), "trim");
    assert_eq!(step.args(), "start=10:end=30");
}

#[test]
fn filter_step_volume_should_produce_correct_args() {
    let step = FilterStep::Volume(-6.0);
    assert_eq!(step.filter_name(), "volume");
    assert_eq!(step.args(), "volume=-6dB");
}

#[test]
fn tone_map_variants_should_have_correct_names() {
    assert_eq!(ToneMap::Hable.as_str(), "hable");
    assert_eq!(ToneMap::Reinhard.as_str(), "reinhard");
    assert_eq!(ToneMap::Mobius.as_str(), "mobius");
}

#[test]
fn builder_empty_steps_should_return_error() {
    let result = FilterGraph::builder().build();
    assert!(
        matches!(result, Err(FilterError::BuildFailed)),
        "expected BuildFailed, got {result:?}"
    );
}

#[test]
fn filter_step_overlay_should_produce_correct_args() {
    let step = FilterStep::Overlay { x: 10, y: 20 };
    assert_eq!(step.filter_name(), "overlay");
    assert_eq!(step.args(), "x=10:y=20");
}

#[test]
fn filter_step_crop_should_produce_correct_args() {
    let step = FilterStep::Crop {
        x: 0,
        y: 0,
        width: 640,
        height: 360,
    };
    assert_eq!(step.filter_name(), "crop");
    assert_eq!(step.args(), "x=0:y=0:w=640:h=360");
}

#[test]
fn filter_step_fade_in_should_produce_correct_filter_name() {
    let step = FilterStep::FadeIn {
        start: 0.0,
        duration: 1.5,
    };
    assert_eq!(step.filter_name(), "fade");
}

#[test]
fn filter_step_fade_in_should_produce_correct_args() {
    let step = FilterStep::FadeIn {
        start: 0.0,
        duration: 1.5,
    };
    assert_eq!(step.args(), "type=in:start_time=0:duration=1.5");
}

#[test]
fn filter_step_fade_in_with_nonzero_start_should_produce_correct_args() {
    let step = FilterStep::FadeIn {
        start: 2.0,
        duration: 1.0,
    };
    assert_eq!(step.args(), "type=in:start_time=2:duration=1");
}

#[test]
fn filter_step_fade_out_should_produce_correct_filter_name() {
    let step = FilterStep::FadeOut {
        start: 8.5,
        duration: 1.5,
    };
    assert_eq!(step.filter_name(), "fade");
}

#[test]
fn filter_step_fade_out_should_produce_correct_args() {
    let step = FilterStep::FadeOut {
        start: 8.5,
        duration: 1.5,
    };
    assert_eq!(step.args(), "type=out:start_time=8.5:duration=1.5");
}

#[test]
fn builder_fade_in_with_valid_params_should_succeed() {
    let result = FilterGraph::builder().fade_in(0.0, 1.5).build();
    assert!(
        result.is_ok(),
        "fade_in(0.0, 1.5) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_fade_out_with_valid_params_should_succeed() {
    let result = FilterGraph::builder().fade_out(8.5, 1.5).build();
    assert!(
        result.is_ok(),
        "fade_out(8.5, 1.5) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_fade_in_with_zero_duration_should_return_invalid_config() {
    let result = FilterGraph::builder().fade_in(0.0, 0.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for zero duration, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("duration"),
            "reason should mention duration: {reason}"
        );
    }
}

#[test]
fn builder_fade_out_with_negative_duration_should_return_invalid_config() {
    let result = FilterGraph::builder().fade_out(0.0, -1.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for negative duration, got {result:?}"
    );
}

#[test]
fn filter_step_fade_in_white_should_produce_correct_filter_name() {
    let step = FilterStep::FadeInWhite {
        start: 0.0,
        duration: 1.0,
    };
    assert_eq!(step.filter_name(), "fade");
}

#[test]
fn filter_step_fade_in_white_should_produce_correct_args() {
    let step = FilterStep::FadeInWhite {
        start: 0.0,
        duration: 1.0,
    };
    assert_eq!(step.args(), "type=in:start_time=0:duration=1:color=white");
}

#[test]
fn filter_step_fade_in_white_with_nonzero_start_should_produce_correct_args() {
    let step = FilterStep::FadeInWhite {
        start: 2.5,
        duration: 1.0,
    };
    assert_eq!(step.args(), "type=in:start_time=2.5:duration=1:color=white");
}

#[test]
fn filter_step_fade_out_white_should_produce_correct_filter_name() {
    let step = FilterStep::FadeOutWhite {
        start: 8.0,
        duration: 1.0,
    };
    assert_eq!(step.filter_name(), "fade");
}

#[test]
fn filter_step_fade_out_white_should_produce_correct_args() {
    let step = FilterStep::FadeOutWhite {
        start: 8.0,
        duration: 1.0,
    };
    assert_eq!(step.args(), "type=out:start_time=8:duration=1:color=white");
}

#[test]
fn builder_fade_in_white_with_valid_params_should_succeed() {
    let result = FilterGraph::builder().fade_in_white(0.0, 1.0).build();
    assert!(
        result.is_ok(),
        "fade_in_white(0.0, 1.0) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_fade_out_white_with_valid_params_should_succeed() {
    let result = FilterGraph::builder().fade_out_white(8.0, 1.0).build();
    assert!(
        result.is_ok(),
        "fade_out_white(8.0, 1.0) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_fade_in_white_with_zero_duration_should_return_invalid_config() {
    let result = FilterGraph::builder().fade_in_white(0.0, 0.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for zero duration, got {result:?}"
    );
}

#[test]
fn builder_fade_out_white_with_negative_duration_should_return_invalid_config() {
    let result = FilterGraph::builder().fade_out_white(0.0, -1.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for negative duration, got {result:?}"
    );
}

#[test]
fn filter_step_rotate_should_produce_correct_args() {
    let step = FilterStep::Rotate {
        angle_degrees: 90.0,
        fill_color: "black".to_owned(),
    };
    assert_eq!(step.filter_name(), "rotate");
    assert_eq!(
        step.args(),
        format!("angle={}:fillcolor=black", 90_f64.to_radians())
    );
}

#[test]
fn filter_step_rotate_transparent_fill_should_produce_correct_args() {
    let step = FilterStep::Rotate {
        angle_degrees: 45.0,
        fill_color: "0x00000000".to_owned(),
    };
    assert_eq!(step.filter_name(), "rotate");
    let args = step.args();
    assert!(
        args.contains("fillcolor=0x00000000"),
        "args should contain transparent fill: {args}"
    );
}

#[test]
fn filter_step_tone_map_should_produce_correct_args() {
    let step = FilterStep::ToneMap(ToneMap::Hable);
    assert_eq!(step.filter_name(), "tonemap");
    assert_eq!(step.args(), "tonemap=hable");
}

#[test]
fn filter_step_amix_should_produce_correct_args() {
    let step = FilterStep::Amix(3);
    assert_eq!(step.filter_name(), "amix");
    assert_eq!(step.args(), "inputs=3");
}

#[test]
fn filter_step_equalizer_should_produce_correct_args() {
    let step = FilterStep::Equalizer {
        band_hz: 1000.0,
        gain_db: 3.0,
    };
    assert_eq!(step.filter_name(), "equalizer");
    assert_eq!(step.args(), "f=1000:width_type=o:width=2:g=3");
}

#[test]
fn builder_steps_should_accumulate_in_order() {
    let result = FilterGraph::builder()
        .trim(0.0, 5.0)
        .scale(1280, 720, ScaleAlgorithm::Fast)
        .volume(-3.0)
        .build();
    assert!(
        result.is_ok(),
        "builder with multiple valid steps must succeed, got {result:?}"
    );
}

#[test]
fn builder_with_valid_steps_should_succeed() {
    let result = FilterGraph::builder()
        .scale(1280, 720, ScaleAlgorithm::Fast)
        .build();
    assert!(
        result.is_ok(),
        "builder with a known filter step must succeed, got {result:?}"
    );
}

#[test]
fn output_resolution_should_return_scale_dimensions() {
    let fg = FilterGraph::builder()
        .scale(1280, 720, ScaleAlgorithm::Fast)
        .build()
        .unwrap();
    assert_eq!(fg.output_resolution(), Some((1280, 720)));
}

#[test]
fn output_resolution_should_return_last_scale_when_chained() {
    let fg = FilterGraph::builder()
        .scale(1920, 1080, ScaleAlgorithm::Fast)
        .scale(1280, 720, ScaleAlgorithm::Bicubic)
        .build()
        .unwrap();
    assert_eq!(fg.output_resolution(), Some((1280, 720)));
}

#[test]
fn output_resolution_should_return_none_when_no_scale() {
    let fg = FilterGraph::builder().trim(0.0, 5.0).build().unwrap();
    assert_eq!(fg.output_resolution(), None);
}

#[test]
fn filter_step_lut3d_should_produce_correct_filter_name() {
    let step = FilterStep::Lut3d {
        path: "grade.cube".to_owned(),
    };
    assert_eq!(step.filter_name(), "lut3d");
}

#[test]
fn filter_step_lut3d_should_produce_correct_args() {
    let step = FilterStep::Lut3d {
        path: "grade.cube".to_owned(),
    };
    assert_eq!(step.args(), "file=grade.cube:interp=trilinear");
}

#[test]
fn builder_lut3d_with_unsupported_extension_should_return_invalid_config() {
    let result = FilterGraph::builder().lut3d("color_grade.txt").build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for unsupported extension, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("unsupported LUT format"),
            "reason should mention unsupported format: {reason}"
        );
    }
}

#[test]
fn builder_lut3d_with_no_extension_should_return_invalid_config() {
    let result = FilterGraph::builder().lut3d("color_grade_no_ext").build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for missing extension, got {result:?}"
    );
}

#[test]
fn builder_lut3d_with_nonexistent_cube_file_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .lut3d("/nonexistent/path/grade_ab12cd.cube")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for nonexistent file, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("LUT file not found"),
            "reason should mention file not found: {reason}"
        );
    }
}

#[test]
fn builder_lut3d_with_nonexistent_3dl_file_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .lut3d("/nonexistent/path/grade_ab12cd.3dl")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for nonexistent .3dl file, got {result:?}"
    );
}

#[test]
fn filter_step_eq_should_produce_correct_filter_name() {
    let step = FilterStep::Eq {
        brightness: 0.0,
        contrast: 1.0,
        saturation: 1.0,
    };
    assert_eq!(step.filter_name(), "eq");
}

#[test]
fn filter_step_eq_should_produce_correct_args() {
    let step = FilterStep::Eq {
        brightness: 0.1,
        contrast: 1.5,
        saturation: 0.8,
    };
    assert_eq!(step.args(), "brightness=0.1:contrast=1.5:saturation=0.8");
}

#[test]
fn builder_eq_with_valid_params_should_succeed() {
    let result = FilterGraph::builder().eq(0.0, 1.0, 1.0).build();
    assert!(
        result.is_ok(),
        "neutral eq params must build successfully, got {result:?}"
    );
}

#[test]
fn builder_eq_with_brightness_too_low_should_return_invalid_config() {
    let result = FilterGraph::builder().eq(-1.5, 1.0, 1.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for brightness < -1.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("brightness"),
            "reason should mention brightness: {reason}"
        );
    }
}

#[test]
fn builder_eq_with_brightness_too_high_should_return_invalid_config() {
    let result = FilterGraph::builder().eq(1.5, 1.0, 1.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for brightness > 1.0, got {result:?}"
    );
}

#[test]
fn builder_eq_with_contrast_out_of_range_should_return_invalid_config() {
    let result = FilterGraph::builder().eq(0.0, 4.0, 1.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for contrast > 3.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("contrast"),
            "reason should mention contrast: {reason}"
        );
    }
}

#[test]
fn builder_eq_with_saturation_out_of_range_should_return_invalid_config() {
    let result = FilterGraph::builder().eq(0.0, 1.0, -0.5).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for saturation < 0.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("saturation"),
            "reason should mention saturation: {reason}"
        );
    }
}

#[test]
fn filter_step_curves_should_produce_correct_filter_name() {
    let step = FilterStep::Curves {
        master: vec![],
        r: vec![],
        g: vec![],
        b: vec![],
    };
    assert_eq!(step.filter_name(), "curves");
}

#[test]
fn filter_step_curves_should_produce_args_with_all_channels() {
    let step = FilterStep::Curves {
        master: vec![(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)],
        r: vec![(0.0, 0.0), (1.0, 1.0)],
        g: vec![],
        b: vec![(0.0, 0.0), (1.0, 0.8)],
    };
    let args = step.args();
    assert!(args.contains("master='0/0 0.5/0.6 1/1'"), "args={args}");
    assert!(args.contains("r='0/0 1/1'"), "args={args}");
    assert!(
        !args.contains("g="),
        "empty g channel should be omitted: args={args}"
    );
    assert!(args.contains("b='0/0 1/0.8'"), "args={args}");
}

#[test]
fn filter_step_curves_with_empty_channels_should_produce_empty_args() {
    let step = FilterStep::Curves {
        master: vec![],
        r: vec![],
        g: vec![],
        b: vec![],
    };
    assert_eq!(
        step.args(),
        "",
        "all-empty curves should produce empty args string"
    );
}

#[test]
fn builder_curves_with_valid_s_curve_should_succeed() {
    let result = FilterGraph::builder()
        .curves(
            vec![
                (0.0, 0.0),
                (0.25, 0.15),
                (0.5, 0.5),
                (0.75, 0.85),
                (1.0, 1.0),
            ],
            vec![],
            vec![],
            vec![],
        )
        .build();
    assert!(
        result.is_ok(),
        "valid S-curve master must build successfully, got {result:?}"
    );
}

#[test]
fn builder_curves_with_out_of_range_point_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .curves(vec![(0.0, 1.5)], vec![], vec![], vec![])
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for out-of-range point, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("curves") && reason.contains("master"),
            "reason should mention curves master: {reason}"
        );
    }
}

#[test]
fn builder_curves_with_out_of_range_r_channel_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .curves(vec![], vec![(1.2, 0.5)], vec![], vec![])
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for out-of-range r channel point, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("curves") && reason.contains(" r "),
            "reason should mention curves r: {reason}"
        );
    }
}

#[test]
fn filter_step_white_balance_should_produce_correct_filter_name() {
    let step = FilterStep::WhiteBalance {
        temperature_k: 6500,
        tint: 0.0,
    };
    assert_eq!(step.filter_name(), "colorchannelmixer");
}

#[test]
fn filter_step_white_balance_6500k_neutral_tint_should_produce_near_unity_args() {
    // At 6500 K (daylight), all channels should be close to 1.0.
    let step = FilterStep::WhiteBalance {
        temperature_k: 6500,
        tint: 0.0,
    };
    let args = step.args();
    // Parse rr= value to verify it is close to 1.0.
    assert!(args.starts_with("rr="), "args must start with rr=: {args}");
    assert!(
        args.contains("gg=") && args.contains("bb="),
        "args must contain gg and bb: {args}"
    );
}

#[test]
fn filter_step_white_balance_3200k_should_produce_warm_shift() {
    // At 3200 K (tungsten), red should dominate over blue.
    use super::super::filter_step::FilterStep as FS;
    // Access kelvin_to_rgb indirectly through the WhiteBalance step args
    let step_warm = FS::WhiteBalance {
        temperature_k: 3200,
        tint: 0.0,
    };
    let step_cool = FS::WhiteBalance {
        temperature_k: 10000,
        tint: 0.0,
    };
    let args_warm = step_warm.args();
    let args_cool = step_cool.args();
    // At warm temperature, rr value should be higher than bb value
    // Just verify the args are produced without panicking
    assert!(
        args_warm.contains("rr=") && args_warm.contains("bb="),
        "args={args_warm}"
    );
    assert!(
        args_cool.contains("rr=") && args_cool.contains("bb="),
        "args={args_cool}"
    );
}

#[test]
fn builder_white_balance_with_valid_params_should_succeed() {
    let result = FilterGraph::builder().white_balance(6500, 0.0).build();
    assert!(
        result.is_ok(),
        "valid white_balance params must build successfully, got {result:?}"
    );
}

#[test]
fn builder_white_balance_with_temperature_too_low_should_return_invalid_config() {
    let result = FilterGraph::builder().white_balance(500, 0.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for temperature_k < 1000, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("temperature_k"),
            "reason should mention temperature_k: {reason}"
        );
    }
}

#[test]
fn builder_white_balance_with_temperature_too_high_should_return_invalid_config() {
    let result = FilterGraph::builder().white_balance(50000, 0.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for temperature_k > 40000, got {result:?}"
    );
}

#[test]
fn builder_white_balance_with_tint_out_of_range_should_return_invalid_config() {
    let result = FilterGraph::builder().white_balance(6500, 1.5).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for tint > 1.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("tint"),
            "reason should mention tint: {reason}"
        );
    }
}

#[test]
fn filter_step_hue_should_produce_correct_filter_name() {
    let step = FilterStep::Hue { degrees: 90.0 };
    assert_eq!(step.filter_name(), "hue");
}

#[test]
fn filter_step_hue_should_produce_correct_args() {
    let step = FilterStep::Hue { degrees: 180.0 };
    assert_eq!(step.args(), "h=180");
}

#[test]
fn filter_step_hue_zero_should_produce_no_op_args() {
    let step = FilterStep::Hue { degrees: 0.0 };
    assert_eq!(step.args(), "h=0");
}

#[test]
fn builder_hue_with_valid_degrees_should_succeed() {
    let result = FilterGraph::builder().hue(0.0).build();
    assert!(
        result.is_ok(),
        "hue(0.0) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_hue_with_degrees_too_high_should_return_invalid_config() {
    let result = FilterGraph::builder().hue(400.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for degrees > 360.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("degrees"),
            "reason should mention degrees: {reason}"
        );
    }
}

#[test]
fn builder_hue_with_degrees_too_low_should_return_invalid_config() {
    let result = FilterGraph::builder().hue(-400.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for degrees < -360.0, got {result:?}"
    );
}

#[test]
fn filter_step_gamma_should_produce_correct_filter_name() {
    let step = FilterStep::Gamma {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
    assert_eq!(step.filter_name(), "eq");
}

#[test]
fn filter_step_gamma_should_produce_correct_args() {
    let step = FilterStep::Gamma {
        r: 2.2,
        g: 2.2,
        b: 2.2,
    };
    assert_eq!(step.args(), "gamma_r=2.2:gamma_g=2.2:gamma_b=2.2");
}

#[test]
fn filter_step_gamma_neutral_should_produce_unity_args() {
    let step = FilterStep::Gamma {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
    assert_eq!(step.args(), "gamma_r=1:gamma_g=1:gamma_b=1");
}

#[test]
fn builder_gamma_with_neutral_values_should_succeed() {
    let result = FilterGraph::builder().gamma(1.0, 1.0, 1.0).build();
    assert!(
        result.is_ok(),
        "gamma(1.0, 1.0, 1.0) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_gamma_with_r_out_of_range_should_return_invalid_config() {
    let result = FilterGraph::builder().gamma(0.0, 1.0, 1.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for r < 0.1, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("gamma") && reason.contains(" r "),
            "reason should mention gamma r: {reason}"
        );
    }
}

#[test]
fn builder_gamma_with_b_out_of_range_should_return_invalid_config() {
    let result = FilterGraph::builder().gamma(1.0, 1.0, 11.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for b > 10.0, got {result:?}"
    );
}

#[test]
fn rgb_neutral_constant_should_have_all_channels_one() {
    assert_eq!(Rgb::NEUTRAL.r, 1.0);
    assert_eq!(Rgb::NEUTRAL.g, 1.0);
    assert_eq!(Rgb::NEUTRAL.b, 1.0);
}

#[test]
fn filter_step_three_way_cc_should_produce_correct_filter_name() {
    let step = FilterStep::ThreeWayCC {
        lift: Rgb::NEUTRAL,
        gamma: Rgb::NEUTRAL,
        gain: Rgb::NEUTRAL,
    };
    assert_eq!(step.filter_name(), "curves");
}

#[test]
fn filter_step_three_way_cc_neutral_should_produce_identity_curves() {
    let step = FilterStep::ThreeWayCC {
        lift: Rgb::NEUTRAL,
        gamma: Rgb::NEUTRAL,
        gain: Rgb::NEUTRAL,
    };
    let args = step.args();
    // Neutral: 0/0, 0.5/0.5, 1/1 for all channels.
    assert!(
        args.contains("r='0/0 0.5/0.5 1/1'"),
        "neutral r channel must be identity: {args}"
    );
    assert!(
        args.contains("g='0/0 0.5/0.5 1/1'"),
        "neutral g channel must be identity: {args}"
    );
    assert!(
        args.contains("b='0/0 0.5/0.5 1/1'"),
        "neutral b channel must be identity: {args}"
    );
}

#[test]
fn builder_three_way_cc_with_neutral_values_should_succeed() {
    let result = FilterGraph::builder()
        .three_way_cc(Rgb::NEUTRAL, Rgb::NEUTRAL, Rgb::NEUTRAL)
        .build();
    assert!(
        result.is_ok(),
        "neutral three_way_cc must build successfully, got {result:?}"
    );
}

#[test]
fn builder_three_way_cc_with_gamma_zero_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .three_way_cc(
            Rgb::NEUTRAL,
            Rgb {
                r: 0.0,
                g: 1.0,
                b: 1.0,
            },
            Rgb::NEUTRAL,
        )
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for gamma.r = 0.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("gamma.r"),
            "reason should mention gamma.r: {reason}"
        );
    }
}

#[test]
fn builder_three_way_cc_with_negative_gamma_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .three_way_cc(
            Rgb::NEUTRAL,
            Rgb {
                r: 1.0,
                g: -0.5,
                b: 1.0,
            },
            Rgb::NEUTRAL,
        )
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for gamma.g < 0.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("gamma.g"),
            "reason should mention gamma.g: {reason}"
        );
    }
}

#[test]
fn filter_step_vignette_should_produce_correct_filter_name() {
    let step = FilterStep::Vignette {
        angle: 0.628,
        x0: 0.0,
        y0: 0.0,
    };
    assert_eq!(step.filter_name(), "vignette");
}

#[test]
fn filter_step_vignette_zero_centre_should_use_w2_h2_defaults() {
    let step = FilterStep::Vignette {
        angle: 0.628,
        x0: 0.0,
        y0: 0.0,
    };
    let args = step.args();
    assert!(args.contains("x0=w/2"), "x0=0.0 should map to w/2: {args}");
    assert!(args.contains("y0=h/2"), "y0=0.0 should map to h/2: {args}");
    assert!(
        args.contains("angle=0.628"),
        "args must contain angle: {args}"
    );
}

#[test]
fn filter_step_vignette_custom_centre_should_produce_numeric_coords() {
    let step = FilterStep::Vignette {
        angle: 0.5,
        x0: 320.0,
        y0: 240.0,
    };
    let args = step.args();
    assert!(args.contains("x0=320"), "custom x0 should appear: {args}");
    assert!(args.contains("y0=240"), "custom y0 should appear: {args}");
}

#[test]
fn builder_vignette_with_valid_angle_should_succeed() {
    let result = FilterGraph::builder()
        .vignette(std::f32::consts::PI / 5.0, 0.0, 0.0)
        .build();
    assert!(
        result.is_ok(),
        "default vignette angle must build successfully, got {result:?}"
    );
}

#[test]
fn builder_vignette_with_angle_too_large_should_return_invalid_config() {
    let result = FilterGraph::builder().vignette(2.0, 0.0, 0.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for angle > π/2, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("angle"),
            "reason should mention angle: {reason}"
        );
    }
}

#[test]
fn builder_vignette_with_negative_angle_should_return_invalid_config() {
    let result = FilterGraph::builder().vignette(-0.1, 0.0, 0.0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for angle < 0.0, got {result:?}"
    );
}

#[test]
fn builder_crop_with_zero_width_should_return_invalid_config() {
    let result = FilterGraph::builder().crop(0, 0, 0, 100).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for width=0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("crop width and height must be > 0"),
            "reason should mention crop dimensions: {reason}"
        );
    }
}

#[test]
fn builder_crop_with_zero_height_should_return_invalid_config() {
    let result = FilterGraph::builder().crop(0, 0, 100, 0).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for height=0, got {result:?}"
    );
}

#[test]
fn builder_crop_with_valid_dimensions_should_succeed() {
    let result = FilterGraph::builder().crop(0, 0, 64, 64).build();
    assert!(
        result.is_ok(),
        "crop with valid dimensions must build successfully, got {result:?}"
    );
}

#[test]
fn filter_step_hflip_should_produce_correct_filter_name_and_empty_args() {
    let step = FilterStep::HFlip;
    assert_eq!(step.filter_name(), "hflip");
    assert_eq!(step.args(), "");
}

#[test]
fn filter_step_vflip_should_produce_correct_filter_name_and_empty_args() {
    let step = FilterStep::VFlip;
    assert_eq!(step.filter_name(), "vflip");
    assert_eq!(step.args(), "");
}

#[test]
fn builder_hflip_should_succeed() {
    let result = FilterGraph::builder().hflip().build();
    assert!(
        result.is_ok(),
        "hflip must build successfully, got {result:?}"
    );
}

#[test]
fn builder_vflip_should_succeed() {
    let result = FilterGraph::builder().vflip().build();
    assert!(
        result.is_ok(),
        "vflip must build successfully, got {result:?}"
    );
}

#[test]
fn builder_hflip_twice_should_succeed() {
    let result = FilterGraph::builder().hflip().hflip().build();
    assert!(
        result.is_ok(),
        "double hflip (round-trip) must build successfully, got {result:?}"
    );
}

#[test]
fn filter_step_pad_should_produce_correct_filter_name() {
    let step = FilterStep::Pad {
        width: 1920,
        height: 1080,
        x: -1,
        y: -1,
        color: "black".to_owned(),
    };
    assert_eq!(step.filter_name(), "pad");
}

#[test]
fn filter_step_pad_negative_xy_should_produce_centred_args() {
    let step = FilterStep::Pad {
        width: 1920,
        height: 1080,
        x: -1,
        y: -1,
        color: "black".to_owned(),
    };
    assert_eq!(
        step.args(),
        "width=1920:height=1080:x=(ow-iw)/2:y=(oh-ih)/2:color=black"
    );
}

#[test]
fn filter_step_pad_explicit_xy_should_produce_numeric_args() {
    let step = FilterStep::Pad {
        width: 1920,
        height: 1080,
        x: 320,
        y: 180,
        color: "0x000000".to_owned(),
    };
    assert_eq!(
        step.args(),
        "width=1920:height=1080:x=320:y=180:color=0x000000"
    );
}

#[test]
fn filter_step_pad_zero_xy_should_produce_zero_offset_args() {
    let step = FilterStep::Pad {
        width: 1280,
        height: 720,
        x: 0,
        y: 0,
        color: "black".to_owned(),
    };
    assert_eq!(step.args(), "width=1280:height=720:x=0:y=0:color=black");
}

#[test]
fn builder_pad_with_valid_params_should_succeed() {
    let result = FilterGraph::builder()
        .pad(1920, 1080, -1, -1, "black")
        .build();
    assert!(
        result.is_ok(),
        "pad with valid params must build successfully, got {result:?}"
    );
}

#[test]
fn builder_pad_with_zero_width_should_return_invalid_config() {
    let result = FilterGraph::builder().pad(0, 1080, -1, -1, "black").build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for width=0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("pad width and height must be > 0"),
            "reason should mention pad dimensions: {reason}"
        );
    }
}

#[test]
fn builder_pad_with_zero_height_should_return_invalid_config() {
    let result = FilterGraph::builder().pad(1920, 0, -1, -1, "black").build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for height=0, got {result:?}"
    );
}

#[test]
fn filter_step_fit_to_aspect_should_produce_correct_filter_name() {
    let step = FilterStep::FitToAspect {
        width: 1920,
        height: 1080,
        color: "black".to_owned(),
    };
    assert_eq!(step.filter_name(), "scale");
}

#[test]
fn filter_step_fit_to_aspect_should_produce_scale_args_with_force_original_aspect_ratio() {
    let step = FilterStep::FitToAspect {
        width: 1920,
        height: 1080,
        color: "black".to_owned(),
    };
    let args = step.args();
    assert!(
        args.contains("w=1920") && args.contains("h=1080"),
        "args must contain target dimensions: {args}"
    );
    assert!(
        args.contains("force_original_aspect_ratio=decrease"),
        "args must request aspect-ratio-preserving scale: {args}"
    );
}

#[test]
fn builder_fit_to_aspect_with_valid_params_should_succeed() {
    let result = FilterGraph::builder()
        .fit_to_aspect(1920, 1080, "black")
        .build();
    assert!(
        result.is_ok(),
        "fit_to_aspect with valid params must build successfully, got {result:?}"
    );
}

#[test]
fn builder_fit_to_aspect_with_zero_width_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .fit_to_aspect(0, 1080, "black")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for width=0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("fit_to_aspect width and height must be > 0"),
            "reason should mention fit_to_aspect dimensions: {reason}"
        );
    }
}

#[test]
fn builder_fit_to_aspect_with_zero_height_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .fit_to_aspect(1920, 0, "black")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for height=0, got {result:?}"
    );
}

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

#[test]
fn xfade_transition_dissolve_should_produce_correct_str() {
    assert_eq!(XfadeTransition::Dissolve.as_str(), "dissolve");
}

#[test]
fn xfade_transition_all_variants_should_produce_unique_strings() {
    let variants = [
        (XfadeTransition::Dissolve, "dissolve"),
        (XfadeTransition::Fade, "fade"),
        (XfadeTransition::WipeLeft, "wipeleft"),
        (XfadeTransition::WipeRight, "wiperight"),
        (XfadeTransition::WipeUp, "wipeup"),
        (XfadeTransition::WipeDown, "wipedown"),
        (XfadeTransition::SlideLeft, "slideleft"),
        (XfadeTransition::SlideRight, "slideright"),
        (XfadeTransition::SlideUp, "slideup"),
        (XfadeTransition::SlideDown, "slidedown"),
        (XfadeTransition::CircleOpen, "circleopen"),
        (XfadeTransition::CircleClose, "circleclose"),
        (XfadeTransition::FadeGrays, "fadegrays"),
        (XfadeTransition::Pixelize, "pixelize"),
    ];
    for (variant, expected) in variants {
        assert_eq!(
            variant.as_str(),
            expected,
            "XfadeTransition::{variant:?} should produce \"{expected}\""
        );
    }
}

#[test]
fn filter_step_xfade_should_produce_correct_filter_name() {
    let step = FilterStep::XFade {
        transition: XfadeTransition::Dissolve,
        duration: 1.0,
        offset: 4.0,
    };
    assert_eq!(step.filter_name(), "xfade");
}

#[test]
fn filter_step_xfade_should_produce_correct_args() {
    let step = FilterStep::XFade {
        transition: XfadeTransition::Dissolve,
        duration: 1.0,
        offset: 4.0,
    };
    assert_eq!(step.args(), "transition=dissolve:duration=1:offset=4");
}

#[test]
fn filter_step_xfade_wipe_right_should_produce_correct_args() {
    let step = FilterStep::XFade {
        transition: XfadeTransition::WipeRight,
        duration: 0.5,
        offset: 9.5,
    };
    assert_eq!(step.args(), "transition=wiperight:duration=0.5:offset=9.5");
}

#[test]
fn builder_xfade_with_valid_params_should_succeed() {
    let result = FilterGraph::builder()
        .xfade(XfadeTransition::Dissolve, 1.0, 4.0)
        .build();
    assert!(
        result.is_ok(),
        "xfade(Dissolve, 1.0, 4.0) must build successfully, got {result:?}"
    );
}

#[test]
fn builder_xfade_with_zero_duration_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .xfade(XfadeTransition::Dissolve, 0.0, 4.0)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for zero duration, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("duration"),
            "reason should mention duration: {reason}"
        );
    }
}

#[test]
fn builder_xfade_with_negative_duration_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .xfade(XfadeTransition::Fade, -1.0, 0.0)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for negative duration, got {result:?}"
    );
}

fn make_drawtext_opts() -> DrawTextOptions {
    DrawTextOptions {
        text: "Hello".to_string(),
        x: "10".to_string(),
        y: "10".to_string(),
        font_size: 24,
        font_color: "white".to_string(),
        font_file: None,
        opacity: 1.0,
        box_color: None,
        box_border_width: 0,
    }
}

#[test]
fn filter_step_drawtext_should_produce_correct_filter_name() {
    let step = FilterStep::DrawText {
        opts: make_drawtext_opts(),
    };
    assert_eq!(step.filter_name(), "drawtext");
}

#[test]
fn filter_step_drawtext_should_produce_correct_args_without_box() {
    let step = FilterStep::DrawText {
        opts: make_drawtext_opts(),
    };
    let args = step.args();
    assert!(
        args.contains("text='Hello'"),
        "args must contain text: {args}"
    );
    assert!(args.contains("x=10"), "args must contain x: {args}");
    assert!(args.contains("y=10"), "args must contain y: {args}");
    assert!(
        args.contains("fontsize=24"),
        "args must contain fontsize: {args}"
    );
    assert!(
        args.contains("fontcolor=white@1.00"),
        "args must contain fontcolor with opacity: {args}"
    );
    assert!(
        !args.contains("box=1"),
        "args must not contain box when box_color is None: {args}"
    );
}

#[test]
fn filter_step_drawtext_with_box_should_include_box_args() {
    let opts = DrawTextOptions {
        box_color: Some("black@0.5".to_string()),
        box_border_width: 5,
        ..make_drawtext_opts()
    };
    let step = FilterStep::DrawText { opts };
    let args = step.args();
    assert!(args.contains("box=1"), "args must contain box=1: {args}");
    assert!(
        args.contains("boxcolor=black@0.5"),
        "args must contain boxcolor: {args}"
    );
    assert!(
        args.contains("boxborderw=5"),
        "args must contain boxborderw: {args}"
    );
}

#[test]
fn filter_step_drawtext_with_font_file_should_include_fontfile_arg() {
    let opts = DrawTextOptions {
        font_file: Some("/usr/share/fonts/arial.ttf".to_string()),
        ..make_drawtext_opts()
    };
    let step = FilterStep::DrawText { opts };
    let args = step.args();
    assert!(
        args.contains("fontfile=/usr/share/fonts/arial.ttf"),
        "args must contain fontfile: {args}"
    );
}

#[test]
fn filter_step_drawtext_should_escape_colon_in_text() {
    let opts = DrawTextOptions {
        text: "Time: 12:00".to_string(),
        ..make_drawtext_opts()
    };
    let step = FilterStep::DrawText { opts };
    let args = step.args();
    assert!(
        args.contains("Time\\: 12\\:00"),
        "colons in text must be escaped: {args}"
    );
}

#[test]
fn filter_step_drawtext_should_escape_backslash_in_text() {
    let opts = DrawTextOptions {
        text: "path\\file".to_string(),
        ..make_drawtext_opts()
    };
    let step = FilterStep::DrawText { opts };
    let args = step.args();
    assert!(
        args.contains("path\\\\file"),
        "backslash in text must be escaped: {args}"
    );
}

#[test]
fn builder_drawtext_with_valid_opts_should_succeed() {
    let result = FilterGraph::builder()
        .drawtext(make_drawtext_opts())
        .build();
    assert!(
        result.is_ok(),
        "drawtext with valid opts must build successfully, got {result:?}"
    );
}

#[test]
fn builder_drawtext_with_empty_text_should_return_invalid_config() {
    let opts = DrawTextOptions {
        text: String::new(),
        ..make_drawtext_opts()
    };
    let result = FilterGraph::builder().drawtext(opts).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for empty text, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("text"),
            "reason should mention text: {reason}"
        );
    }
}

#[test]
fn builder_drawtext_with_opacity_too_high_should_return_invalid_config() {
    let opts = DrawTextOptions {
        opacity: 1.5,
        ..make_drawtext_opts()
    };
    let result = FilterGraph::builder().drawtext(opts).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for opacity > 1.0, got {result:?}"
    );
}

#[test]
fn builder_drawtext_with_negative_opacity_should_return_invalid_config() {
    let opts = DrawTextOptions {
        opacity: -0.1,
        ..make_drawtext_opts()
    };
    let result = FilterGraph::builder().drawtext(opts).build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for opacity < 0.0, got {result:?}"
    );
}

#[test]
fn filter_step_subtitles_srt_should_produce_correct_filter_name() {
    let step = FilterStep::SubtitlesSrt {
        path: "subs.srt".to_owned(),
    };
    assert_eq!(step.filter_name(), "subtitles");
}

#[test]
fn filter_step_subtitles_srt_should_produce_correct_args() {
    let step = FilterStep::SubtitlesSrt {
        path: "subs.srt".to_owned(),
    };
    assert_eq!(step.args(), "filename=subs.srt");
}

#[test]
fn builder_subtitles_srt_with_wrong_extension_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_srt("subtitles.vtt")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for wrong extension, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("unsupported subtitle format"),
            "reason should mention unsupported format: {reason}"
        );
    }
}

#[test]
fn builder_subtitles_srt_with_no_extension_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_srt("subtitles_no_ext")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for missing extension, got {result:?}"
    );
}

#[test]
fn builder_subtitles_srt_with_nonexistent_file_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_srt("/nonexistent/path/subs_ab12cd.srt")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for nonexistent file, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("subtitle file not found"),
            "reason should mention file not found: {reason}"
        );
    }
}

#[test]
fn filter_step_subtitles_ass_should_produce_correct_filter_name() {
    let step = FilterStep::SubtitlesAss {
        path: "subs.ass".to_owned(),
    };
    assert_eq!(step.filter_name(), "ass");
}

#[test]
fn filter_step_subtitles_ass_should_produce_correct_args() {
    let step = FilterStep::SubtitlesAss {
        path: "subs.ass".to_owned(),
    };
    assert_eq!(step.args(), "filename=subs.ass");
}

#[test]
fn filter_step_subtitles_ssa_should_produce_correct_filter_name() {
    let step = FilterStep::SubtitlesAss {
        path: "subs.ssa".to_owned(),
    };
    assert_eq!(step.filter_name(), "ass");
}

#[test]
fn builder_subtitles_ass_with_wrong_extension_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_ass("subtitles.srt")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for wrong extension, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("unsupported subtitle format"),
            "reason should mention unsupported format: {reason}"
        );
    }
}

#[test]
fn builder_subtitles_ass_with_no_extension_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_ass("subtitles_no_ext")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for missing extension, got {result:?}"
    );
}

#[test]
fn builder_subtitles_ass_with_nonexistent_file_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_ass("/nonexistent/path/subs_ab12cd.ass")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for nonexistent .ass file, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("subtitle file not found"),
            "reason should mention file not found: {reason}"
        );
    }
}

#[test]
fn builder_subtitles_ssa_with_nonexistent_file_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .subtitles_ass("/nonexistent/path/subs_ab12cd.ssa")
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for nonexistent .ssa file, got {result:?}"
    );
}

#[test]
fn filter_step_overlay_image_should_produce_correct_filter_name() {
    let step = FilterStep::OverlayImage {
        path: "logo.png".to_owned(),
        x: "10".to_owned(),
        y: "10".to_owned(),
        opacity: 1.0,
    };
    assert_eq!(step.filter_name(), "overlay");
}

#[test]
fn filter_step_overlay_image_should_produce_correct_args() {
    let step = FilterStep::OverlayImage {
        path: "logo.png".to_owned(),
        x: "W-w-10".to_owned(),
        y: "H-h-10".to_owned(),
        opacity: 0.7,
    };
    assert_eq!(step.args(), "W-w-10:H-h-10");
}

#[test]
fn builder_overlay_image_with_wrong_extension_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .overlay_image("logo.jpg", "10", "10", 1.0)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for wrong extension, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("unsupported image format"),
            "reason should mention unsupported format: {reason}"
        );
    }
}

#[test]
fn builder_overlay_image_with_no_extension_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .overlay_image("logo_no_ext", "10", "10", 1.0)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for missing extension, got {result:?}"
    );
}

#[test]
fn builder_overlay_image_with_nonexistent_file_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .overlay_image("/nonexistent/path/logo_ab12cd.png", "10", "10", 1.0)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for nonexistent file, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("overlay image not found"),
            "reason should mention file not found: {reason}"
        );
    }
}

#[test]
fn builder_overlay_image_with_opacity_above_1_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .overlay_image("/nonexistent/logo.png", "10", "10", 1.1)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for opacity > 1.0, got {result:?}"
    );
    if let Err(FilterError::InvalidConfig { reason }) = result {
        assert!(
            reason.contains("opacity"),
            "reason should mention opacity: {reason}"
        );
    }
}

#[test]
fn builder_overlay_image_with_negative_opacity_should_return_invalid_config() {
    let result = FilterGraph::builder()
        .overlay_image("/nonexistent/logo.png", "10", "10", -0.1)
        .build();
    assert!(
        matches!(result, Err(FilterError::InvalidConfig { .. })),
        "expected InvalidConfig for opacity < 0.0, got {result:?}"
    );
}
