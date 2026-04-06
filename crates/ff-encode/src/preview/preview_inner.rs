//! Unsafe FFmpeg filter graph calls for preview generation.
//!
//! All `unsafe` code is isolated here; [`super`] exposes safe wrappers.
//!
//! Entry points:
//! - [`generate_sprite_sheet_unsafe`] — filter graph + PNG encode for sprite sheets

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::CString;
use std::path::Path;
use std::ptr;

use ff_sys::{
    AVCodecID_AV_CODEC_ID_PNG, AVRational, av_buffersink_get_frame, av_frame_alloc, av_frame_free,
    av_interleaved_write_frame, av_packet_alloc, av_packet_free, av_packet_unref, av_write_trailer,
    avcodec, avfilter_get_by_name, avfilter_graph_alloc, avfilter_graph_config,
    avfilter_graph_create_filter, avfilter_graph_free, avfilter_link, avformat,
    avformat_alloc_output_context2, avformat_free_context, avformat_new_stream,
    avformat_write_header,
};

use crate::EncodeError;

/// Probes the video at `path` and returns its duration in seconds.
///
/// # Safety
///
/// All FFmpeg pointers are null-checked and freed on every exit path.
unsafe fn probe_video_duration_secs(path: &Path) -> Result<f64, EncodeError> {
    let fmt_ctx = avformat::open_input(path).map_err(EncodeError::from_ffmpeg_error)?;

    if let Err(e) = avformat::find_stream_info(fmt_ctx) {
        let mut p = fmt_ctx;
        avformat::close_input(&mut p);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // SAFETY: fmt_ctx is non-null (open_input succeeded).
    let duration_av = (*fmt_ctx).duration;
    let mut p = fmt_ctx;
    avformat::close_input(&mut p);

    if duration_av <= 0 {
        return Err(EncodeError::MediaOperationFailed {
            reason: "cannot determine video duration".to_string(),
        });
    }

    // AV_TIME_BASE = 1_000_000 (microseconds); precision loss is acceptable for duration.
    #[allow(clippy::cast_precision_loss)]
    let secs = duration_av as f64 / 1_000_000.0;
    Ok(secs)
}

/// Generates a sprite sheet PNG from `input`, writing to `output`.
///
/// Filter chain:
/// `movie=filename={input} → fps={N}/{duration} → scale={fw}:{fh} →
///  tile={cols}x{rows}:padding=0:margin=0 → buffersink`
///
/// The `tile` filter accumulates `cols * rows` frames and emits one composite
/// frame, which is then encoded as PNG.
///
/// # Safety
///
/// All raw pointer operations follow avfilter and avcodec ownership rules.
/// Every allocation is freed on every exit path via the `bail!` macro or
/// explicit cleanup at the end of the function.
pub(super) unsafe fn generate_sprite_sheet_unsafe(
    input: &Path,
    cols: u32,
    rows: u32,
    frame_width: u32,
    frame_height: u32,
    output: &Path,
) -> Result<(), EncodeError> {
    // ── Step 1: probe duration ────────────────────────────────────────────────
    let duration_secs = probe_video_duration_secs(input)?;
    let n = cols * rows;
    // FPS needed to sample exactly N frames across the full duration.
    let fps_arg = format!("{n}/{duration_secs:.6}");

    // ── Step 2: build filter graph ────────────────────────────────────────────
    macro_rules! bail {
        ($graph:expr, $reason:expr) => {{
            let mut g = $graph;
            avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(EncodeError::MediaOperationFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let path_str = input.to_string_lossy();
    let movie_args = CString::new(format!("filename={path_str}")).map_err(|_| {
        EncodeError::MediaOperationFailed {
            reason: "input path contains null byte".to_string(),
        }
    })?;
    let fps_cstr =
        CString::new(fps_arg.as_str()).map_err(|_| EncodeError::MediaOperationFailed {
            reason: "fps arg contains null byte".to_string(),
        })?;
    let scale_args = CString::new(format!("{frame_width}:{frame_height}")).map_err(|_| {
        EncodeError::MediaOperationFailed {
            reason: "scale args contain null byte".to_string(),
        }
    })?;
    let tile_args = CString::new(format!("{cols}x{rows}:padding=0:margin=0")).map_err(|_| {
        EncodeError::MediaOperationFailed {
            reason: "tile args contain null byte".to_string(),
        }
    })?;

    let graph = avfilter_graph_alloc();
    if graph.is_null() {
        return Err(EncodeError::MediaOperationFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    // 1. movie source
    let movie_filt = avfilter_get_by_name(c"movie".as_ptr());
    if movie_filt.is_null() {
        bail!(graph, "filter not found: movie");
    }
    let mut movie_ctx: *mut ff_sys::AVFilterContext = ptr::null_mut();
    let ret = avfilter_graph_create_filter(
        &raw mut movie_ctx,
        movie_filt,
        c"sprite_movie".as_ptr(),
        movie_args.as_ptr(),
        ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("movie create_filter failed code={ret}"));
    }

    // 2. fps filter
    let fps_filt = avfilter_get_by_name(c"fps".as_ptr());
    if fps_filt.is_null() {
        bail!(graph, "filter not found: fps");
    }
    let mut fps_ctx: *mut ff_sys::AVFilterContext = ptr::null_mut();
    let ret = avfilter_graph_create_filter(
        &raw mut fps_ctx,
        fps_filt,
        c"sprite_fps".as_ptr(),
        fps_cstr.as_ptr(),
        ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("fps create_filter failed code={ret}"));
    }

    // 3. scale filter
    let scale_filt = avfilter_get_by_name(c"scale".as_ptr());
    if scale_filt.is_null() {
        bail!(graph, "filter not found: scale");
    }
    let mut scale_ctx: *mut ff_sys::AVFilterContext = ptr::null_mut();
    let ret = avfilter_graph_create_filter(
        &raw mut scale_ctx,
        scale_filt,
        c"sprite_scale".as_ptr(),
        scale_args.as_ptr(),
        ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("scale create_filter failed code={ret}"));
    }

    // 4. tile filter
    let tile_filt = avfilter_get_by_name(c"tile".as_ptr());
    if tile_filt.is_null() {
        bail!(graph, "filter not found: tile");
    }
    let mut tile_ctx: *mut ff_sys::AVFilterContext = ptr::null_mut();
    let ret = avfilter_graph_create_filter(
        &raw mut tile_ctx,
        tile_filt,
        c"sprite_tile".as_ptr(),
        tile_args.as_ptr(),
        ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("tile create_filter failed code={ret}"));
    }

    // 5. buffersink
    let buffersink_filt = avfilter_get_by_name(c"buffersink".as_ptr());
    if buffersink_filt.is_null() {
        bail!(graph, "filter not found: buffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = ptr::null_mut();
    let ret = avfilter_graph_create_filter(
        &raw mut sink_ctx,
        buffersink_filt,
        c"sprite_sink".as_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("buffersink create_filter failed code={ret}"));
    }

    // Links: movie → fps → scale → tile → buffersink
    let ret = avfilter_link(movie_ctx, 0, fps_ctx, 0);
    if ret < 0 {
        bail!(graph, format!("avfilter_link movie→fps failed code={ret}"));
    }
    let ret = avfilter_link(fps_ctx, 0, scale_ctx, 0);
    if ret < 0 {
        bail!(graph, format!("avfilter_link fps→scale failed code={ret}"));
    }
    let ret = avfilter_link(scale_ctx, 0, tile_ctx, 0);
    if ret < 0 {
        bail!(graph, format!("avfilter_link scale→tile failed code={ret}"));
    }
    let ret = avfilter_link(tile_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(
            graph,
            format!("avfilter_link tile→buffersink failed code={ret}")
        );
    }

    // Configure graph
    let ret = avfilter_graph_config(graph, ptr::null_mut());
    if ret < 0 {
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // ── Step 3: pull one output frame from the tile filter ────────────────────
    let tile_frame = av_frame_alloc();
    if tile_frame.is_null() {
        bail!(graph, "av_frame_alloc failed for tile frame");
    }

    let ret = av_buffersink_get_frame(sink_ctx, tile_frame);
    let got_frame = ret >= 0;

    if !got_frame {
        let mut f = tile_frame;
        av_frame_free(std::ptr::addr_of_mut!(f));
        bail!(graph, "tile filter produced no output frame");
    }

    // ── Step 4: encode the tile frame as PNG ──────────────────────────────────
    let encode_result =
        encode_frame_as_png(tile_frame, output, cols, rows, frame_width, frame_height);

    // Cleanup filter graph and frame regardless of encode result.
    let mut f = tile_frame;
    av_frame_free(std::ptr::addr_of_mut!(f));
    let mut g = graph;
    avfilter_graph_free(std::ptr::addr_of_mut!(g));

    encode_result?;

    log::info!(
        "sprite sheet generated cols={cols} rows={rows} output={}",
        output.display()
    );

    Ok(())
}

/// Encodes a raw `*mut AVFrame` as a PNG file at `output`.
///
/// # Safety
///
/// `frame` must be a valid, non-null frame produced by the tile filter.
/// All allocations are freed on every exit path.
unsafe fn encode_frame_as_png(
    frame: *mut ff_sys::AVFrame,
    output: &Path,
    cols: u32,
    rows: u32,
    frame_width: u32,
    frame_height: u32,
) -> Result<(), EncodeError> {
    let _ = (cols, rows, frame_width, frame_height); // used via frame dimensions

    let width = (*frame).width;
    let height = (*frame).height;
    let pix_fmt = (*frame).format;

    // ── Allocate output format context ────────────────────────────────────────
    let mut fmt_ctx: *mut ff_sys::AVFormatContext = ptr::null_mut();
    let c_path = CString::new(
        output
            .to_str()
            .ok_or_else(|| EncodeError::CannotCreateFile {
                path: output.to_path_buf(),
            })?,
    )
    .map_err(|_| EncodeError::CannotCreateFile {
        path: output.to_path_buf(),
    })?;

    // Try the dedicated "apng" muxer first; fall back to auto-detection.
    let mut ret = avformat_alloc_output_context2(
        &mut fmt_ctx,
        ptr::null_mut(),
        c"apng".as_ptr(),
        c_path.as_ptr(),
    );
    if ret < 0 || fmt_ctx.is_null() {
        ret = avformat_alloc_output_context2(
            &mut fmt_ctx,
            ptr::null_mut(),
            ptr::null(),
            c_path.as_ptr(),
        );
    }
    if ret < 0 || fmt_ctx.is_null() {
        return Err(EncodeError::from_ffmpeg_error(ret));
    }

    // ── Create video stream ───────────────────────────────────────────────────
    let stream = avformat_new_stream(fmt_ctx, ptr::null());
    if stream.is_null() {
        avformat_free_context(fmt_ctx);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "avformat_new_stream failed".to_string(),
        });
    }

    // ── Find and open PNG encoder ─────────────────────────────────────────────
    let codec = avcodec::find_encoder(AVCodecID_AV_CODEC_ID_PNG).ok_or_else(|| {
        EncodeError::UnsupportedCodec {
            codec: "png".to_string(),
        }
    })?;

    let codec_ctx = match avcodec::alloc_context3(codec) {
        Ok(ctx) => ctx,
        Err(e) => {
            avformat_free_context(fmt_ctx);
            return Err(EncodeError::from_ffmpeg_error(e));
        }
    };

    // SAFETY: codec_ctx is non-null (alloc_context3 succeeded).
    (*codec_ctx).width = width;
    (*codec_ctx).height = height;
    (*codec_ctx).time_base = AVRational { num: 1, den: 1 };
    (*codec_ctx).pix_fmt = pix_fmt;

    if let Err(e) = avcodec::open2(codec_ctx, codec, ptr::null_mut()) {
        avcodec::free_context(&mut { codec_ctx });
        avformat_free_context(fmt_ctx);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // Copy codec parameters to stream.
    // SAFETY: stream and codec_ctx are non-null.
    let par = (*stream).codecpar;
    (*par).codec_id = AVCodecID_AV_CODEC_ID_PNG;
    (*par).codec_type = ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO;
    (*par).width = width;
    (*par).height = height;
    (*par).format = pix_fmt;

    // ── Open output IO and write header ───────────────────────────────────────
    let io_ctx = match avformat::open_output(output, avformat::avio_flags::WRITE) {
        Ok(ctx) => ctx,
        Err(e) => {
            let mut cc = codec_ctx;
            avcodec::free_context(&mut cc);
            avformat_free_context(fmt_ctx);
            return Err(EncodeError::from_ffmpeg_error(e));
        }
    };
    (*fmt_ctx).pb = io_ctx;

    let ret = avformat_write_header(fmt_ctx, ptr::null_mut());
    if ret < 0 {
        avformat::close_output(&mut (*fmt_ctx).pb);
        let mut cc = codec_ctx;
        avcodec::free_context(&mut cc);
        avformat_free_context(fmt_ctx);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }

    // ── Allocate packet ───────────────────────────────────────────────────────
    let packet = av_packet_alloc();
    if packet.is_null() {
        av_write_trailer(fmt_ctx);
        avformat::close_output(&mut (*fmt_ctx).pb);
        let mut cc = codec_ctx;
        avcodec::free_context(&mut cc);
        avformat_free_context(fmt_ctx);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "av_packet_alloc failed".to_string(),
        });
    }

    // ── Encode: send frame → flush → drain packets ────────────────────────────
    (*frame).pts = 0;

    let encode_result = (|| -> Result<(), EncodeError> {
        avcodec::send_frame(codec_ctx, frame).map_err(EncodeError::from_ffmpeg_error)?;
        drain_packets(codec_ctx, fmt_ctx, packet, false)?;
        avcodec::send_frame(codec_ctx, ptr::null()).map_err(EncodeError::from_ffmpeg_error)?;
        drain_packets(codec_ctx, fmt_ctx, packet, true)?;
        Ok(())
    })();

    av_write_trailer(fmt_ctx);
    avformat::close_output(&mut (*fmt_ctx).pb);
    av_packet_free(&mut { packet });
    avcodec::free_context(&mut { codec_ctx });
    avformat_free_context(fmt_ctx);

    encode_result
}

/// Drains encoded packets from `codec_ctx` and writes them to `fmt_ctx`.
///
/// When `until_eof` is `true`, loops until `AVERROR_EOF`; otherwise also
/// stops on `AVERROR(EAGAIN)`.
///
/// # Safety
///
/// `codec_ctx`, `fmt_ctx`, and `packet` must all be valid non-null pointers.
unsafe fn drain_packets(
    codec_ctx: *mut ff_sys::AVCodecContext,
    fmt_ctx: *mut ff_sys::AVFormatContext,
    packet: *mut ff_sys::AVPacket,
    until_eof: bool,
) -> Result<(), EncodeError> {
    loop {
        match avcodec::receive_packet(codec_ctx, packet) {
            Ok(()) => {
                (*packet).stream_index = 0;
                let ret = av_interleaved_write_frame(fmt_ctx, packet);
                av_packet_unref(packet);
                if ret < 0 {
                    return Err(EncodeError::from_ffmpeg_error(ret));
                }
            }
            Err(e) if e == ff_sys::error_codes::EOF => break,
            Err(e) if !until_eof && e == ff_sys::error_codes::EAGAIN => break,
            Err(e) => return Err(EncodeError::from_ffmpeg_error(e)),
        }
    }
    Ok(())
}
