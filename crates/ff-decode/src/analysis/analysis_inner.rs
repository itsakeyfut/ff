//! Inner implementation details for media analysis tools.
//!
//! Analysis tools that require direct `FFmpeg` calls (filter graphs,
//! packet-level access) add their `unsafe` implementation here.
//! `WaveformAnalyzer` uses only the safe [`crate::AudioDecoder`] API
//! and therefore has no code in that section.

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::too_many_lines)]

use std::ffi::CString;
use std::path::Path;
use std::time::Duration;

use crate::DecodeError;

// ── SceneDetector inner ───────────────────────────────────────────────────────

/// Detects scene changes in `path` using the filter graph:
///
/// `movie=filename=path → select=gt(scene\,threshold) → buffersink`
///
/// Drains all output frames and returns their PTS as [`Duration`] values.
/// Only frames that survive the `select` filter (i.e. whose scene score
/// exceeds `threshold`) appear in the output.
///
/// # Safety
///
/// All raw pointer operations follow the avfilter ownership rules:
/// - `avfilter_graph_alloc()` returns an owned pointer freed via
///   `avfilter_graph_free()` on error or after draining.
/// - `avfilter_graph_create_filter()` adds contexts owned by the graph.
/// - `avfilter_link()` connects pads owned by the graph.
/// - `avfilter_graph_config()` finalises the graph.
/// - `av_frame_alloc()` / `av_frame_free()` manage per-frame lifetimes.
/// - The buffersink's input link (accessed via `(*sink_ctx).inputs`) is
///   valid after `avfilter_graph_config` succeeds.
pub(super) unsafe fn detect_scenes_unsafe(
    path: &Path,
    threshold: f64,
) -> Result<Vec<Duration>, DecodeError> {
    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(DecodeError::AnalysisFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let path_str = path.to_string_lossy();
    let movie_args =
        CString::new(format!("filename={path_str}")).map_err(|_| DecodeError::AnalysisFailed {
            reason: "path contains null byte".to_string(),
        })?;

    // select=gt(scene\,{threshold}) — the backslash escapes the comma inside
    // the filter option string so FFmpeg's option parser treats it as a literal
    // comma rather than a separator between filter options.
    let select_args = CString::new(format!("gt(scene\\,{threshold:.6})")).map_err(|_| {
        DecodeError::AnalysisFailed {
            reason: "select args contained null byte".to_string(),
        }
    })?;

    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(DecodeError::AnalysisFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    // 1. movie source — reads the video file directly.
    let movie_filt = ff_sys::avfilter_get_by_name(c"movie".as_ptr());
    if movie_filt.is_null() {
        bail!(graph, "filter not found: movie");
    }
    let mut src_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut src_ctx,
        movie_filt,
        c"scene_src".as_ptr(),
        movie_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("movie create_filter failed code={ret}"));
    }

    // 2. select=gt(scene\,threshold) — passes only scene-change frames.
    let select_filt = ff_sys::avfilter_get_by_name(c"select".as_ptr());
    if select_filt.is_null() {
        bail!(graph, "filter not found: select");
    }
    let mut sel_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sel_ctx,
        select_filt,
        c"scene_select".as_ptr(),
        select_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("select create_filter failed code={ret}"));
    }

    // 3. buffersink — collects output frames.
    let buffersink_filt = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
    if buffersink_filt.is_null() {
        bail!(graph, "filter not found: buffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        buffersink_filt,
        c"scene_sink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("buffersink create_filter failed code={ret}"));
    }

    // Link: src → select → sink
    let ret = ff_sys::avfilter_link(src_ctx, 0, sel_ctx, 0);
    if ret < 0 {
        bail!(graph, format!("avfilter_link src→select failed code={ret}"));
    }
    let ret = ff_sys::avfilter_link(sel_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(
            graph,
            format!("avfilter_link select→sink failed code={ret}")
        );
    }

    // Configure the graph.
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // Read the output time base from the buffersink's input link.
    // SAFETY: After avfilter_graph_config succeeds, sink_ctx->inputs[0] is a
    // valid, non-null AVFilterLink* owned by the graph.
    let time_base = (*(*(*sink_ctx).inputs)).time_base;
    let tb_num = f64::from(time_base.num);
    let tb_den = f64::from(time_base.den);

    // Drain all output frames; each frame that exits the select filter
    // represents a detected scene change.
    let mut timestamps: Vec<Duration> = Vec::new();

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

        let pts = (*raw_frame).pts;
        if pts != ff_sys::AV_NOPTS_VALUE && tb_den > 0.0 {
            let secs = pts as f64 * tb_num / tb_den;
            if secs >= 0.0 {
                timestamps.push(Duration::from_secs_f64(secs));
            }
        }

        let mut ptr = raw_frame;
        ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
    }

    let mut g = graph;
    ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));

    log::debug!(
        "scene detection complete scenes={} threshold={threshold:.4}",
        timestamps.len()
    );

    Ok(timestamps)
}
