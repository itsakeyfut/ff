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
/// - `(*frame).time_base` is set by the filter framework inside
///   `av_buffersink_get_frame` and is valid for the frame's lifetime.
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

        // SAFETY: `(*raw_frame).time_base` is set by the filter framework when
        // av_buffersink_get_frame fills the frame.  `(*raw_frame).pts` is the
        // presentation timestamp in that time base.
        let pts = (*raw_frame).pts;
        let time_base = (*raw_frame).time_base;
        let tb_num = f64::from(time_base.num);
        let tb_den = f64::from(time_base.den);
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

// ── KeyframeEnumerator inner ──────────────────────────────────────────────────

/// Enumerates all keyframe PTS values for the given stream in `path`.
///
/// Processing flow:
/// 1. `avformat_open_input` + `avformat_find_stream_info`
/// 2. Locate the target stream (first video stream when `stream_index` is `None`)
/// 3. `av_read_frame` loop — no decoder is opened
/// 4. For packets on the target stream with `AV_PKT_FLAG_KEY` set, convert PTS
///    to [`Duration`] using the stream's time base
/// 5. `av_packet_unref` after each packet; `av_packet_free` + `avformat_close_input` on exit
///
/// # Safety
///
/// All raw pointer operations follow avformat ownership rules:
/// - `avformat_open_input` returns an owned `AVFormatContext*` freed via
///   `avformat_close_input` on every exit path.
/// - `av_packet_alloc` returns an owned `AVPacket*` freed via `av_packet_free`.
/// - `av_packet_unref` is called after every successful `av_read_frame`.
/// - Stream pointer access is guarded by bounds checks on `nb_streams`.
pub(super) unsafe fn enumerate_keyframes_unsafe(
    path: &Path,
    stream_index: Option<usize>,
) -> Result<Vec<Duration>, DecodeError> {
    // AV_PKT_FLAG_KEY = 1 (from FFmpeg's avcodec.h; not exported by ff_sys constants).
    const AV_PKT_FLAG_KEY: i32 = 1;

    // The `bail_ctx!` macro frees the format context before returning an error.
    // It must not be used before `format_ctx` is initialised.
    macro_rules! bail_ctx {
        ($ctx:ident, $reason:expr) => {{
            ff_sys::avformat::close_input(std::ptr::addr_of_mut!($ctx));
            return Err(DecodeError::AnalysisFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    // 1. Open input.
    let mut format_ctx =
        ff_sys::avformat::open_input(path).map_err(|code| DecodeError::AnalysisFailed {
            reason: format!("avformat_open_input failed code={code}"),
        })?;

    // 2. Find stream info.
    if let Err(code) = ff_sys::avformat::find_stream_info(format_ctx) {
        bail_ctx!(
            format_ctx,
            format!("avformat_find_stream_info failed code={code}")
        );
    }

    // 3. Locate the target stream.
    let nb_streams = (*format_ctx).nb_streams as usize;
    let target_stream: usize = if let Some(idx) = stream_index {
        if idx >= nb_streams {
            bail_ctx!(
                format_ctx,
                format!("stream_index {idx} out of range (file has {nb_streams} streams)")
            );
        }
        idx
    } else {
        // Select the first video stream.
        let mut found: Option<usize> = None;
        for i in 0..nb_streams {
            let stream = (*format_ctx).streams.add(i);
            let codecpar = (*(*stream)).codecpar;
            if (*codecpar).codec_type == ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO {
                found = Some(i);
                break;
            }
        }
        if let Some(i) = found {
            i
        } else {
            bail_ctx!(format_ctx, "no video stream found in file")
        }
    };

    // 4. Read the stream's time base for PTS → Duration conversion.
    let stream = (*format_ctx).streams.add(target_stream);
    let time_base = (*(*stream)).time_base;
    let tb_num = f64::from(time_base.num);
    let tb_den = f64::from(time_base.den);

    // 5. Allocate a reusable packet.
    let pkt = ff_sys::av_packet_alloc();
    if pkt.is_null() {
        bail_ctx!(format_ctx, "av_packet_alloc failed");
    }

    // 6. Read all packets; record the PTS of every keyframe on the target stream.
    // Stream indices in AVPacket are i32; the cast is bounded by nb_streams which
    // is u32, so values fit in i32 on all supported platforms.
    #[allow(clippy::cast_possible_wrap)]
    let target_i32 = target_stream as i32;
    let mut timestamps: Vec<Duration> = Vec::new();

    loop {
        // `av_read_frame` returns a negative code on EOF or error.
        let ret = ff_sys::av_read_frame(format_ctx, pkt);
        if ret < 0 {
            break;
        }

        if (*pkt).stream_index == target_i32 && (*pkt).flags & AV_PKT_FLAG_KEY != 0 {
            // Prefer PTS; fall back to DTS for streams that only write DTS on packets.
            let pts = if (*pkt).pts == ff_sys::AV_NOPTS_VALUE {
                (*pkt).dts
            } else {
                (*pkt).pts
            };
            if pts != ff_sys::AV_NOPTS_VALUE && tb_den > 0.0 {
                let secs = pts as f64 * tb_num / tb_den;
                if secs >= 0.0 {
                    timestamps.push(Duration::from_secs_f64(secs));
                }
            }
        }

        ff_sys::av_packet_unref(pkt);
    }

    // 7. Release resources.
    let mut pkt_ptr = pkt;
    ff_sys::av_packet_free(std::ptr::addr_of_mut!(pkt_ptr));
    ff_sys::avformat::close_input(std::ptr::addr_of_mut!(format_ctx));

    log::debug!(
        "keyframe enumeration complete keyframes={} stream_index={target_stream}",
        timestamps.len()
    );

    Ok(timestamps)
}
