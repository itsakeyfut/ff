//! `FFmpeg` filter graph internals for whole-file video effects.
//!
//! All `unsafe` code lives here; [`super`] exposes safe wrappers.
//!
//! Current `unsafe` entry points:
//! - [`analyze_vidstab_unsafe`] — motion analysis via `vidstabdetect` filter

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::CString;
use std::path::Path;

use crate::FilterError;
use crate::effects::stabilizer::AnalyzeOptions;

/// Runs `vidstabdetect` motion analysis on `input`, writing the transform data
/// to `output_trf`.
///
/// Builds and drains the filter graph:
/// `movie=filename={input} → vidstabdetect=... → nullsink`
///
/// The `.trf` file is written as a side effect by `vidstabdetect` as frames
/// pass through it.  `avfilter_graph_request_oldest` drives the graph one
/// frame at a time until EOF.
///
/// # Safety
///
/// All raw pointer operations follow the avfilter ownership rules:
/// - `avfilter_graph_alloc()` returns an owned pointer freed via
///   `avfilter_graph_free()` on every exit path (bail! or normal).
/// - `avfilter_graph_create_filter()` adds filter contexts owned by the graph.
/// - `avfilter_link()` connects pads owned by the graph.
/// - `avfilter_graph_config()` finalises the graph.
/// - All `CString` values are kept alive for the duration of the graph build.
pub(super) unsafe fn analyze_vidstab_unsafe(
    input: &Path,
    output_trf: &Path,
    opts: &AnalyzeOptions,
) -> Result<(), FilterError> {
    macro_rules! bail {
        ($graph:ident, $code:expr, $msg:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::Ffmpeg {
                code: $code,
                message: $msg.to_string(),
            });
        }};
    }

    // Pre-flight: check that vidstabdetect is available in this FFmpeg build.
    // SAFETY: c"vidstabdetect" is a valid null-terminated C string literal.
    let vidstab_filt = ff_sys::avfilter_get_by_name(c"vidstabdetect".as_ptr());
    if vidstab_filt.is_null() {
        return Err(FilterError::Ffmpeg {
            code: 0,
            message: "vidstabdetect filter not available in this FFmpeg build".to_string(),
        });
    }

    // Build CString args before allocating the graph.
    let input_str = input.to_string_lossy();
    let trf_str = output_trf.to_string_lossy();

    let movie_args =
        CString::new(format!("filename={input_str}")).map_err(|_| FilterError::Ffmpeg {
            code: 0,
            message: "input path contains null byte".to_string(),
        })?;

    let shakiness = opts.shakiness.clamp(1, 10);
    let accuracy = opts.accuracy.clamp(1, 15);
    let stepsize = opts.stepsize.clamp(1, 32);

    let vidstab_args = CString::new(format!(
        "shakiness={shakiness}:accuracy={accuracy}:stepsize={stepsize}:result={trf_str}"
    ))
    .map_err(|_| FilterError::Ffmpeg {
        code: 0,
        message: "trf path contains null byte".to_string(),
    })?;

    // Allocate the filter graph.
    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::Ffmpeg {
            code: 0,
            message: "avfilter_graph_alloc failed".to_string(),
        });
    }

    // 1. movie source — reads the input file directly through lavfi.
    let movie_filt = ff_sys::avfilter_get_by_name(c"movie".as_ptr());
    if movie_filt.is_null() {
        bail!(graph, 0, "filter not found: movie");
    }
    let mut src_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut src_ctx,
        movie_filt,
        c"vidstab_src".as_ptr(),
        movie_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, ret, format!("movie create_filter failed code={ret}"));
    }

    // 2. vidstabdetect — writes transform data to the .trf file as a side effect.
    log::debug!(
        "filter added name=vidstabdetect args={}",
        vidstab_args.to_string_lossy()
    );
    let mut vstab_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut vstab_ctx,
        vidstab_filt,
        c"vidstab_detect".as_ptr(),
        vidstab_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(
            graph,
            ret,
            format!("vidstabdetect create_filter failed code={ret}")
        );
    }

    // 3. buffersink — drains output frames; vidstabdetect writes the .trf file
    //    as frames pass through it, so the sink content is discarded.
    let buffersink_filt = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
    if buffersink_filt.is_null() {
        bail!(graph, 0, "filter not found: buffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        buffersink_filt,
        c"vidstab_sink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(
            graph,
            ret,
            format!("buffersink create_filter failed code={ret}")
        );
    }

    // Link: movie → vidstabdetect → buffersink
    let ret = ff_sys::avfilter_link(src_ctx, 0, vstab_ctx, 0);
    if ret < 0 {
        bail!(
            graph,
            ret,
            format!("avfilter_link movie→vidstabdetect failed code={ret}")
        );
    }
    let ret = ff_sys::avfilter_link(vstab_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(
            graph,
            ret,
            format!("avfilter_link vidstabdetect→buffersink failed code={ret}")
        );
    }

    // Configure the graph — opens the input file via the movie filter.
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        bail!(
            graph,
            ret,
            format!(
                "avfilter_graph_config failed code={ret} message={}",
                ff_sys::av_error_string(ret)
            )
        );
    }

    // Drain all frames.  av_buffersink_get_frame pulls one frame per call;
    // vidstabdetect writes the .trf file incrementally as each frame passes
    // through it.  We discard the frame data — only the side effect matters.
    loop {
        let raw_frame = ff_sys::av_frame_alloc();
        if raw_frame.is_null() {
            break;
        }
        // SAFETY: sink_ctx is a valid buffersink context; raw_frame is a
        // freshly allocated AVFrame owned by this scope.
        let ret = ff_sys::av_buffersink_get_frame(sink_ctx, raw_frame);
        let mut ptr = raw_frame;
        ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
        if ret < 0 {
            // AVERROR_EOF or AVERROR(EAGAIN): all frames processed.
            break;
        }
    }

    let mut g = graph;
    ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));

    log::info!(
        "stabilization analysis complete trf={}",
        output_trf.display()
    );

    Ok(())
}
