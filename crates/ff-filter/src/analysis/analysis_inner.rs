//! `FFmpeg` filter graph internals for loudness analysis.
//!
//! All `unsafe` code lives here; [`super`] exposes a safe wrapper.

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::cast_possible_truncation)]

use std::ffi::CString;
use std::path::Path;

use crate::FilterError;
use crate::analysis::LoudnessResult;

/// Measures EBU R128 integrated loudness, loudness range, and true peak for
/// `path` using the filter graph:
///
/// `amovie=filename=path → ebur128=metadata=1:peak=true → abuffersink`
///
/// Drains all output frames, reading `lavfi.r128.*` metadata from each.
/// The last seen values for each key are returned.
///
/// # Safety
///
/// All raw pointer operations follow the avfilter ownership rules:
/// - `avfilter_graph_alloc()` returns an owned pointer freed via
///   `avfilter_graph_free()` on error or after draining.
/// - `avfilter_graph_create_filter()` adds contexts owned by the graph.
/// - `avfilter_link()` connects pads owned by the graph.
/// - `avfilter_graph_config()` finalises the graph.
/// - `av_frame_alloc()` / `av_frame_free()` manage frame lifetimes.
pub(super) unsafe fn measure_loudness_unsafe(path: &Path) -> Result<LoudnessResult, FilterError> {
    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::AnalysisFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let path_str = path.to_string_lossy();
    let amovie_args =
        CString::new(format!("filename={path_str}")).map_err(|_| FilterError::AnalysisFailed {
            reason: "path contains null byte".to_string(),
        })?;

    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::AnalysisFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    // 1. amovie source — reads the file directly, no external decoder needed.
    let amovie_filt = ff_sys::avfilter_get_by_name(c"amovie".as_ptr());
    if amovie_filt.is_null() {
        bail!(graph, "filter not found: amovie");
    }
    let mut src_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut src_ctx,
        amovie_filt,
        c"loudness_src".as_ptr(),
        amovie_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("amovie create_filter failed code={ret}"));
    }

    // 2. ebur128=metadata=1:peak=true — writes R128 stats to AVFrame::metadata.
    let ebur128_filt = ff_sys::avfilter_get_by_name(c"ebur128".as_ptr());
    if ebur128_filt.is_null() {
        bail!(graph, "filter not found: ebur128");
    }
    let mut meas_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut meas_ctx,
        ebur128_filt,
        c"loudness_ebur128".as_ptr(),
        c"metadata=1:peak=true".as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("ebur128 create_filter failed code={ret}"));
    }

    // 3. abuffersink
    let abuffersink_filt = ff_sys::avfilter_get_by_name(c"abuffersink".as_ptr());
    if abuffersink_filt.is_null() {
        bail!(graph, "filter not found: abuffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        abuffersink_filt,
        c"loudness_sink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(
            graph,
            format!("abuffersink create_filter failed code={ret}")
        );
    }

    // Link: src → ebur128 → sink
    let ret = ff_sys::avfilter_link(src_ctx, 0, meas_ctx, 0);
    if ret < 0 {
        bail!(
            graph,
            format!("avfilter_link src→ebur128 failed code={ret}")
        );
    }
    let ret = ff_sys::avfilter_link(meas_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(
            graph,
            format!("avfilter_link ebur128→sink failed code={ret}")
        );
    }

    // Configure the graph.
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // Drain all output frames; keep the last R128 metadata values seen.
    let mut integrated_lufs = f32::NEG_INFINITY;
    let mut lra_lu: f32 = 0.0;
    let mut true_peak_dbtp = f32::NEG_INFINITY;

    loop {
        let raw_frame = ff_sys::av_frame_alloc();
        if raw_frame.is_null() {
            break;
        }
        let ret = ff_sys::av_buffersink_get_frame(sink_ctx, raw_frame);
        if ret < 0 {
            let mut ptr = raw_frame;
            ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
            break;
        }
        // SAFETY: `(*raw_frame).metadata` is a valid `AVDictionary*` (may be null);
        // `av_dict_get` handles null dictionaries by returning null.
        read_f32_meta(raw_frame, c"lavfi.r128.I".as_ptr(), &mut integrated_lufs);
        read_f32_meta(raw_frame, c"lavfi.r128.LRA".as_ptr(), &mut lra_lu);
        read_f32_meta(
            raw_frame,
            c"lavfi.r128.true_peak".as_ptr(),
            &mut true_peak_dbtp,
        );
        let mut ptr = raw_frame;
        ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
    }

    let mut g = graph;
    ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));

    log::debug!(
        "loudness analysis complete integrated_lufs={integrated_lufs:.1} \
         lra_lu={lra_lu:.1} true_peak_dbtp={true_peak_dbtp:.1}"
    );

    Ok(LoudnessResult {
        integrated_lufs,
        lra: lra_lu,
        true_peak_dbtp,
    })
}

/// Reads a single `f32` metadata value from `frame` by `key`.
/// Updates `*out` only when the key is present and its value is parseable.
///
/// # Safety
///
/// - `frame` must point to a valid, fully-initialised `AVFrame`.
/// - `key` must be a null-terminated C string valid for the duration of the call.
unsafe fn read_f32_meta(
    frame: *mut ff_sys::AVFrame,
    key: *const std::os::raw::c_char,
    out: &mut f32,
) {
    let entry = ff_sys::av_dict_get((*frame).metadata, key, std::ptr::null(), 0);
    if !entry.is_null()
        && let Ok(s) = std::ffi::CStr::from_ptr((*entry).value).to_str()
        && let Ok(v) = s.parse::<f32>()
    {
        *out = v;
    }
}
