//! Graph-building helpers: hardware acceleration and filter linking.

use super::{AUDIO_TIME_BASE_NUM, VIDEO_TIME_BASE_DEN, VIDEO_TIME_BASE_NUM};
use crate::error::FilterError;
use crate::graph::filter_step::FilterStep;
use crate::graph::types::HwAccel;

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
