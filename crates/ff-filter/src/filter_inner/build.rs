//! Graph-building helpers: hardware acceleration and filter linking.

use super::{AUDIO_TIME_BASE_NUM, VIDEO_TIME_BASE_DEN, VIDEO_TIME_BASE_NUM};
use crate::error::FilterError;
use crate::graph::filter_step::FilterStep;
use crate::graph::types::{EqBand, HwAccel};

// ── Hardware acceleration helpers ─────────────────────────────────────────────

/// Map a [`HwAccel`] variant to the corresponding `AVHWDeviceType` constant.
pub(super) fn hw_accel_to_device_type(hw: HwAccel) -> ff_sys::AVHWDeviceType {
    match hw {
        HwAccel::Cuda => ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_CUDA,
        HwAccel::VideoToolbox => ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
        HwAccel::Vaapi => ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI,
    }
}

/// Create and link a named hardware filter (e.g., `hwupload_cuda`, `hwdownload`)
/// with no arguments.  Sets the filter context's `hw_device_ctx` to a new
/// reference obtained via `av_buffer_ref(hw_ctx)` so the filter owns its own ref.
///
/// # Safety
///
/// `graph`, `prev_ctx`, and `hw_ctx` must be valid non-null pointers.
/// `hw_ctx` must be a valid `AVBufferRef` wrapping an `AVHWDeviceContext`.
pub(super) unsafe fn create_hw_filter(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    filter_name: &std::ffi::CStr,
    instance_name: &std::ffi::CStr,
    hw_ctx: *mut ff_sys::AVBufferRef,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    // SAFETY: `avfilter_get_by_name` reads a valid null-terminated C string and
    // returns a borrowed, process-lifetime pointer (or null if not found).
    let filter = ff_sys::avfilter_get_by_name(filter_name.as_ptr());
    if filter.is_null() {
        log::warn!(
            "hw filter not found name={}",
            filter_name.to_str().unwrap_or("?")
        );
        return Err(FilterError::BuildFailed);
    }

    let mut ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut ctx,
        filter,
        instance_name.as_ptr(),
        std::ptr::null(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!(
            "hw filter creation failed name={} code={ret}",
            filter_name.to_str().unwrap_or("?")
        );
        return Err(FilterError::BuildFailed);
    }
    log::debug!(
        "hw filter added name={}",
        filter_name.to_str().unwrap_or("?")
    );

    // Give this filter context its own reference to the hardware device context.
    // SAFETY: `hw_ctx` is a valid `AVBufferRef`; `av_buffer_ref` returns a new
    // reference counted ref, or null on allocation failure.
    let filter_hw_ref = ff_sys::av_buffer_ref(hw_ctx);
    if filter_hw_ref.is_null() {
        log::warn!("av_buffer_ref failed for hw device context");
        return Err(FilterError::BuildFailed);
    }
    (*ctx).hw_device_ctx = filter_hw_ref;

    // SAFETY: `prev_ctx` and `ctx` belong to the same graph; pad indices are valid.
    let ret = ff_sys::avfilter_link(prev_ctx, 0, ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }

    Ok(ctx)
}

// ── Shared graph-building helper ──────────────────────────────────────────────

/// Create an `AVFilterContext` for `step`, link it after `prev_ctx`, and return
/// the new context pointer.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_and_link_step(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    step: &FilterStep,
    index: usize,
    prefix: &str,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let filter_name =
        std::ffi::CString::new(step.filter_name()).map_err(|_| FilterError::BuildFailed)?;
    let filter = ff_sys::avfilter_get_by_name(filter_name.as_ptr());
    if filter.is_null() {
        log::warn!("filter not found name={}", step.filter_name());
        return Err(FilterError::BuildFailed);
    }

    let step_name =
        std::ffi::CString::new(format!("{prefix}{index}")).map_err(|_| FilterError::BuildFailed)?;
    let step_args_str = step.args();
    let step_args =
        std::ffi::CString::new(step_args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;

    let mut step_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut step_ctx,
        filter,
        step_name.as_ptr(),
        step_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!(
            "filter creation failed name={} args={}",
            step.filter_name(),
            step.args()
        );
        return Err(FilterError::BuildFailed);
    }
    log::debug!(
        "filter added name={} args={}",
        step.filter_name(),
        step.args()
    );

    let ret = ff_sys::avfilter_link(prev_ctx, 0, step_ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }
    Ok(step_ctx)
}

/// Create and link a filter by explicit name and args strings, without a
/// `FilterStep`.  Used when the filter name cannot be determined statically
/// (e.g., `AudioDelay` dispatches to `adelay` or `atrim` depending on sign).
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_raw_filter_step(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    filter_name: &str,
    args: &str,
    index: usize,
    prefix: &str,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let filter_cname = std::ffi::CString::new(filter_name).map_err(|_| FilterError::BuildFailed)?;
    // SAFETY: `avfilter_get_by_name` reads a valid null-terminated C string.
    let filter = ff_sys::avfilter_get_by_name(filter_cname.as_ptr());
    if filter.is_null() {
        log::warn!("filter not found name={filter_name}");
        return Err(FilterError::BuildFailed);
    }

    let step_name =
        std::ffi::CString::new(format!("{prefix}{index}")).map_err(|_| FilterError::BuildFailed)?;
    let step_args = std::ffi::CString::new(args).map_err(|_| FilterError::BuildFailed)?;

    let mut step_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut step_ctx,
        filter,
        step_name.as_ptr(),
        step_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name={filter_name} args={args}");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name={filter_name} args={args}");

    // SAFETY: `prev_ctx` and `step_ctx` belong to the same graph; pad indices are valid.
    let ret = ff_sys::avfilter_link(prev_ctx, 0, step_ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }
    Ok(step_ctx)
}

/// Insert a `setpts=PTS-STARTPTS` filter immediately after a `trim` step so
/// that the output stream's timestamps start at zero.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_setpts_after_trim(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    trim_index: usize,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let setpts = ff_sys::avfilter_get_by_name(c"setpts".as_ptr());
    if setpts.is_null() {
        log::warn!("filter not found name=setpts");
        return Err(FilterError::BuildFailed);
    }

    let name = std::ffi::CString::new(format!("setpts{trim_index}"))
        .map_err(|_| FilterError::BuildFailed)?;

    let mut ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut ctx,
        setpts,
        name.as_ptr(),
        c"PTS-STARTPTS".as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name=setpts args=PTS-STARTPTS");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name=setpts args=PTS-STARTPTS");

    let ret = ff_sys::avfilter_link(prev_ctx, 0, ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }
    Ok(ctx)
}

/// Add a `pad` filter that centres the scaled frame on the target `width × height`
/// canvas, completing the scale-then-pad compound step for [`FilterStep::FitToAspect`].
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_fit_to_aspect_pad(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    width: u32,
    height: u32,
    color: &str,
    index: usize,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let pad_filter = ff_sys::avfilter_get_by_name(c"pad".as_ptr());
    if pad_filter.is_null() {
        log::warn!("filter not found name=pad (fit_to_aspect)");
        return Err(FilterError::BuildFailed);
    }

    let name =
        std::ffi::CString::new(format!("fitpad{index}")).map_err(|_| FilterError::BuildFailed)?;
    let args_str = format!("width={width}:height={height}:x=(ow-iw)/2:y=(oh-ih)/2:color={color}");
    let args = std::ffi::CString::new(args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;

    let mut ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut ctx,
        pad_filter,
        name.as_ptr(),
        args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name=pad args={args_str}");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name=pad args={args_str} index={index}");

    let ret = ff_sys::avfilter_link(prev_ctx, 0, ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }
    Ok(ctx)
}

// ── Speed (atempo chain) ──────────────────────────────────────────────────────

/// Decompose a speed `factor` into a chain of `atempo` values, each in [0.5, 2.0].
///
/// The `atempo` filter only accepts values in [0.5, 2.0] per instance.
/// Chaining multiple instances multiplies the effective speed factor:
///
/// - `factor = 4.0`  → `[2.0, 2.0]`       (2.0 × 2.0 = 4.0)
/// - `factor = 0.25` → `[0.5, 0.5]`       (0.5 × 0.5 = 0.25)
/// - `factor = 8.0`  → `[2.0, 2.0, 2.0]`  (2.0³ = 8.0)
/// - `factor = 1.5`  → `[1.5]`            (within range directly)
pub(super) fn decompose_atempo(factor: f64) -> Vec<f64> {
    let mut remaining = factor;
    let mut chain = Vec::new();
    while remaining > 2.0 {
        chain.push(2.0);
        remaining /= 2.0;
    }
    while remaining < 0.5 {
        chain.push(0.5);
        remaining /= 0.5;
    }
    chain.push(remaining);
    chain
}

/// Insert a chain of `atempo` filters for the audio path of a `Speed` step.
///
/// Creates as many `atempo` instances as needed (see [`decompose_atempo`]) and
/// links them in series after `prev_ctx`.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_atempo_chain(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    factor: f64,
    index: usize,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let atempo_filter = ff_sys::avfilter_get_by_name(c"atempo".as_ptr());
    if atempo_filter.is_null() {
        log::warn!("filter not found name=atempo (speed)");
        return Err(FilterError::BuildFailed);
    }

    let chain = decompose_atempo(factor);
    let mut ctx = prev_ctx;
    for (j, &val) in chain.iter().enumerate() {
        let name = std::ffi::CString::new(format!("atempo{index}_{j}"))
            .map_err(|_| FilterError::BuildFailed)?;
        let args_str = format!("{val}");
        let args =
            std::ffi::CString::new(args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;
        let mut atempo_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut atempo_ctx,
            atempo_filter,
            name.as_ptr(),
            args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            log::warn!("filter creation failed name=atempo args={val}");
            return Err(FilterError::BuildFailed);
        }
        log::debug!("filter added name=atempo args={val} index={index}_{j}");

        let ret = ff_sys::avfilter_link(ctx, 0, atempo_ctx, 0);
        if ret < 0 {
            return Err(FilterError::BuildFailed);
        }
        ctx = atempo_ctx;
    }
    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::decompose_atempo;

    #[test]
    fn decompose_atempo_should_return_single_value_for_factor_within_range() {
        assert_eq!(decompose_atempo(1.5), vec![1.5]);
    }

    #[test]
    fn decompose_atempo_should_return_single_value_for_factor_1() {
        assert_eq!(decompose_atempo(1.0), vec![1.0]);
    }

    #[test]
    fn decompose_atempo_should_chain_two_instances_for_factor_4() {
        assert_eq!(decompose_atempo(4.0), vec![2.0, 2.0]);
    }

    #[test]
    fn decompose_atempo_should_chain_three_instances_for_factor_8() {
        assert_eq!(decompose_atempo(8.0), vec![2.0, 2.0, 2.0]);
    }

    #[test]
    fn decompose_atempo_should_chain_two_instances_for_factor_0_25() {
        let chain = decompose_atempo(0.25);
        let product: f64 = chain.iter().product();
        assert!(
            (product - 0.25).abs() < 1e-9,
            "product should be ~0.25, got {product}, chain={chain:?}"
        );
        for &v in &chain {
            assert!(
                (0.5..=2.0).contains(&v),
                "each value must be in [0.5, 2.0], got {v}"
            );
        }
    }

    #[test]
    fn decompose_atempo_should_produce_values_all_within_valid_range() {
        for factor in [0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 4.0, 8.0, 16.0, 100.0] {
            let chain = decompose_atempo(factor);
            assert!(
                !chain.is_empty(),
                "chain must not be empty for factor={factor}"
            );
            let product: f64 = chain.iter().product();
            assert!(
                (product - factor).abs() < 1e-6,
                "product {product} must equal factor {factor}, chain={chain:?}"
            );
            for &v in &chain {
                assert!(
                    (0.5..=2.0).contains(&v),
                    "each value must be in [0.5, 2.0], got {v} for factor={factor}"
                );
            }
        }
    }
}

// ── Overlay image compound step ───────────────────────────────────────────────

/// Insert the compound `movie → lut → overlay` filter chain for an
/// [`FilterStep::OverlayImage`] step.
///
/// Unlike standard steps (which go through [`add_and_link_step`]), this step
/// creates three filter contexts internally:
///
/// 1. `movie` — loads the PNG from `path` as a self-contained video source
///    (no buffersrc input slot is consumed).
/// 2. `lut` — scales the alpha channel by `opacity` (`a = val * opacity`).
/// 3. `overlay` — composites the main stream (pad 0) with the image (pad 1)
///    at position `(x, y)`.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_overlay_image_step(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    path: &str,
    x: &str,
    y: &str,
    opacity: f32,
    index: usize,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    use std::ffi::CString;

    // 1. movie filter — self-contained PNG source, no buffersrc slot needed.
    let movie_filter = ff_sys::avfilter_get_by_name(c"movie".as_ptr());
    if movie_filter.is_null() {
        log::warn!("filter not found name=movie (overlay_image)");
        return Err(FilterError::BuildFailed);
    }
    let movie_name = CString::new(format!("movie{index}")).map_err(|_| FilterError::BuildFailed)?;
    let movie_args =
        CString::new(format!("filename={path}")).map_err(|_| FilterError::BuildFailed)?;
    let mut movie_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut movie_ctx,
        movie_filter,
        movie_name.as_ptr(),
        movie_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name=movie args=filename={path}");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name=movie args=filename={path} index={index}");

    // 2. lut filter — scale the alpha channel: a = val * opacity.
    //    For 8-bit RGBA the `lut` filter operates per-channel; `val` is the
    //    current pixel value (0–255) and the expression is evaluated per sample.
    let lut_filter = ff_sys::avfilter_get_by_name(c"lut".as_ptr());
    if lut_filter.is_null() {
        log::warn!("filter not found name=lut (overlay_image)");
        return Err(FilterError::BuildFailed);
    }
    let lut_name = CString::new(format!("lut{index}")).map_err(|_| FilterError::BuildFailed)?;
    let lut_args_str = format!("a=val*{opacity}");
    let lut_args = CString::new(lut_args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;
    let mut lut_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut lut_ctx,
        lut_filter,
        lut_name.as_ptr(),
        lut_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name=lut args={lut_args_str}");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name=lut args={lut_args_str} index={index}");

    // Link: movie → lut (alpha scaling).
    let ret = ff_sys::avfilter_link(movie_ctx, 0, lut_ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }

    // 3. overlay filter — composite main stream (pad 0) with image (pad 1).
    let overlay_filter = ff_sys::avfilter_get_by_name(c"overlay".as_ptr());
    if overlay_filter.is_null() {
        log::warn!("filter not found name=overlay (overlay_image)");
        return Err(FilterError::BuildFailed);
    }
    let overlay_name =
        CString::new(format!("step{index}")).map_err(|_| FilterError::BuildFailed)?;
    let overlay_args_str = format!("{x}:{y}");
    let overlay_args =
        CString::new(overlay_args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;
    let mut overlay_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut overlay_ctx,
        overlay_filter,
        overlay_name.as_ptr(),
        overlay_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name=overlay args={overlay_args_str}");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name=overlay args={overlay_args_str} index={index}");

    // Link: prev_ctx → overlay pad 0 (main video stream).
    let ret = ff_sys::avfilter_link(prev_ctx, 0, overlay_ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }

    // Link: lut → overlay pad 1 (image stream).
    let ret = ff_sys::avfilter_link(lut_ctx, 0, overlay_ctx, 1);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }

    Ok(overlay_ctx)
}

// ── buffersrc / buffersink arg-string helpers ──────────────────────────────────

/// Build the `args` string passed to `avfilter_graph_create_filter` when
/// creating a video `buffer` (buffersrc) context.
///
/// The format follows libavfilter's `buffer` filter parameter syntax:
/// `video_size=WxH:pix_fmt=N:time_base=NUM/DEN:pixel_aspect=1/1`.
pub(super) fn video_buffersrc_args(
    width: u32,
    height: u32,
    pix_fmt: std::os::raw::c_int,
) -> String {
    format!(
        "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect=1/1",
        width, height, pix_fmt, VIDEO_TIME_BASE_NUM, VIDEO_TIME_BASE_DEN,
    )
}

/// Build the `args` string passed to `avfilter_graph_create_filter` when
/// creating an audio `abuffer` (buffersrc) context.
///
/// The format follows libavfilter's `abuffer` filter parameter syntax:
/// `sample_rate=R:sample_fmt=FMT:channels=C:time_base=1/R`.
pub(super) fn audio_buffersrc_args(
    sample_rate: u32,
    sample_fmt_name: &str,
    channels: u32,
) -> String {
    format!(
        "sample_rate={}:sample_fmt={}:channels={}:time_base={}/{}",
        sample_rate, sample_fmt_name, channels, AUDIO_TIME_BASE_NUM, sample_rate,
    )
}

// ── Parametric EQ (multi-band chain) ─────────────────────────────────────────

/// Insert a chain of filter nodes for a [`FilterStep::ParametricEq`] step.
///
/// One node per band is created using the band's own filter name (e.g.,
/// `lowshelf`, `highshelf`, `equalizer`) and args, and each is linked in
/// series after `prev_ctx`.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
pub(super) unsafe fn add_parametric_eq_chain(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    bands: &[EqBand],
    index: usize,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let mut ctx = prev_ctx;
    for (j, band) in bands.iter().enumerate() {
        let filter_name =
            std::ffi::CString::new(band.filter_name()).map_err(|_| FilterError::BuildFailed)?;
        let filter = ff_sys::avfilter_get_by_name(filter_name.as_ptr());
        if filter.is_null() {
            log::warn!("filter not found name={}", band.filter_name());
            return Err(FilterError::BuildFailed);
        }

        let name = std::ffi::CString::new(format!("eq{index}_{j}"))
            .map_err(|_| FilterError::BuildFailed)?;
        let args_str = band.args();
        let args =
            std::ffi::CString::new(args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;

        let mut band_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut band_ctx,
            filter,
            name.as_ptr(),
            args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            log::warn!(
                "filter creation failed name={} args={}",
                band.filter_name(),
                args_str
            );
            return Err(FilterError::BuildFailed);
        }
        log::debug!(
            "filter added name={} args={} index={index} band={j}",
            band.filter_name(),
            args_str
        );

        let ret = ff_sys::avfilter_link(ctx, 0, band_ctx, 0);
        if ret < 0 {
            return Err(FilterError::BuildFailed);
        }
        ctx = band_ctx;
    }
    Ok(ctx)
}
