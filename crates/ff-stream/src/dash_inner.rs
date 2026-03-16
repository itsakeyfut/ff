//! Internal DASH muxing implementation using `FFmpeg` directly.
//!
//! This module implements the decode → encode → DASH-mux loop that powers
//! [`DashOutput::write`](crate::dash::DashOutput::write).  All `unsafe` code is
//! isolated here; `dash.rs` is purely safe Rust.

// This module is intentionally unsafe — it drives the FFmpeg C API directly.
#![allow(unsafe_code)]
// Rust 2024: Allow unsafe operations in unsafe functions for FFmpeg C API
#![allow(unsafe_op_in_unsafe_fn)]
// FFmpeg C API frequently requires raw pointer casting and borrows-as-ptr
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::too_many_lines)]
// `&mut ptr` to get `*mut *mut T` is the standard FFmpeg double-pointer pattern
#![allow(clippy::borrow_as_ptr)]
// `&mut foo as *mut *mut _` is the standard way to pass double-pointers in FFmpeg
#![allow(clippy::ref_as_ptr)]

use std::ffi::CString;
use std::path::Path;
use std::ptr;

use ff_sys::{
    AVCodecContext, AVFormatContext, AVFrame, AVPictureType_AV_PICTURE_TYPE_I,
    AVPictureType_AV_PICTURE_TYPE_NONE, AVPixelFormat, AVPixelFormat_AV_PIX_FMT_YUV420P,
    SwrContext, SwsContext, av_frame_alloc, av_frame_free, av_frame_get_buffer, av_frame_unref,
    av_interleaved_write_frame, av_opt_set, av_packet_alloc, av_packet_free, av_packet_unref,
    av_write_trailer, avformat_alloc_output_context2, avformat_free_context, avformat_new_stream,
    avformat_write_header,
};

use crate::error::StreamError;

// ============================================================================
// Helper: map an FFmpeg error code to StreamError::Ffmpeg
// ============================================================================

fn ffmpeg_err(code: i32) -> StreamError {
    StreamError::Ffmpeg {
        code,
        message: ff_sys::av_error_string(code),
    }
}

fn ffmpeg_err_msg(msg: &str) -> StreamError {
    StreamError::Ffmpeg {
        code: 0,
        message: msg.to_owned(),
    }
}

// ============================================================================
// Public entry point (safe wrapper)
// ============================================================================

/// Write a DASH segmented stream for the given input file.
///
/// Creates `output_dir/manifest.mpd` and initialization/media segment files
/// (`init-stream0.m4s`, `chunk-stream0-NNNNN.m4s`, …).
///
/// # Errors
///
/// Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation fails, or
/// [`StreamError::Io`] when directory creation fails.
pub(crate) fn write_dash(
    input_path: &str,
    output_dir: &str,
    segment_duration_secs: f64,
) -> Result<(), StreamError> {
    std::fs::create_dir_all(output_dir)?;
    // SAFETY: All FFmpeg resources are allocated and freed within this call.
    unsafe { write_dash_unsafe(input_path, output_dir, segment_duration_secs) }
}

// ============================================================================
// Unsafe implementation
// ============================================================================

unsafe fn write_dash_unsafe(
    input_path: &str,
    output_dir: &str,
    segment_duration_secs: f64,
) -> Result<(), StreamError> {
    ff_sys::ensure_initialized();

    // ── 1. Open input ─────────────────────────────────────────────────────────
    let mut input_ctx = ff_sys::avformat::open_input(Path::new(input_path)).map_err(ffmpeg_err)?;

    ff_sys::avformat::find_stream_info(input_ctx).map_err(|e| {
        ff_sys::avformat::close_input(&mut input_ctx);
        ffmpeg_err(e)
    })?;

    // ── 2. Locate video and audio streams ─────────────────────────────────────
    let nb_streams = (*input_ctx).nb_streams as usize;
    let mut video_stream_idx: i32 = -1;
    let mut audio_stream_idx: i32 = -1;

    for i in 0..nb_streams {
        let stream = *(*input_ctx).streams.add(i);
        let codec_type = (*(*stream).codecpar).codec_type;
        if codec_type == ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO && video_stream_idx < 0 {
            video_stream_idx = i as i32;
        } else if codec_type == ff_sys::AVMediaType_AVMEDIA_TYPE_AUDIO && audio_stream_idx < 0 {
            audio_stream_idx = i as i32;
        }
    }

    if video_stream_idx < 0 {
        ff_sys::avformat::close_input(&mut input_ctx);
        return Err(StreamError::InvalidConfig {
            reason: "input file contains no video stream".into(),
        });
    }

    // ── 3. Read video stream properties ──────────────────────────────────────
    let video_stream = *(*input_ctx).streams.add(video_stream_idx as usize);
    let video_codecpar = (*video_stream).codecpar;
    let enc_width = (*video_codecpar).width;
    let enc_height = (*video_codecpar).height;
    let video_fps = {
        let r = (*video_stream).avg_frame_rate;
        if r.den > 0 && r.num > 0 {
            r.num as f64 / r.den as f64
        } else {
            30.0
        }
    };
    let fps_int = video_fps.round().max(1.0) as i32;

    // Compute keyframe interval from segment duration and fps
    let keyframe_interval = (segment_duration_secs * fps_int as f64).round().max(1.0) as u32;

    // ── 4. Open input video decoder ────────────────────────────────────────────
    let vid_codec_id = (*video_codecpar).codec_id;
    let vid_decoder = ff_sys::avcodec::find_decoder(vid_codec_id)
        .ok_or_else(|| ffmpeg_err_msg("no video decoder available for input stream"))?;

    let mut vid_dec_ctx = ff_sys::avcodec::alloc_context3(vid_decoder).map_err(ffmpeg_err)?;

    ff_sys::avcodec::parameters_to_context(vid_dec_ctx, video_codecpar).map_err(|e| {
        ff_sys::avcodec::free_context(&mut vid_dec_ctx as *mut *mut _);
        ff_sys::avformat::close_input(&mut input_ctx);
        ffmpeg_err(e)
    })?;

    ff_sys::avcodec::open2(vid_dec_ctx, vid_decoder, ptr::null_mut()).map_err(|e| {
        ff_sys::avcodec::free_context(&mut vid_dec_ctx as *mut *mut _);
        ff_sys::avformat::close_input(&mut input_ctx);
        ffmpeg_err(e)
    })?;

    // ── 5. Open input audio decoder (optional) ────────────────────────────────
    let mut aud_dec_ctx: *mut AVCodecContext = ptr::null_mut();
    let mut aud_sample_rate: i32 = 44100;
    let mut aud_nb_channels: i32 = 2;

    if audio_stream_idx >= 0 {
        let audio_stream = *(*input_ctx).streams.add(audio_stream_idx as usize);
        let audio_codecpar = (*audio_stream).codecpar;
        let aud_codec_id = (*audio_codecpar).codec_id;

        if let Some(aud_decoder) = ff_sys::avcodec::find_decoder(aud_codec_id) {
            if let Ok(ctx) = ff_sys::avcodec::alloc_context3(aud_decoder) {
                aud_dec_ctx = ctx;
                if ff_sys::avcodec::parameters_to_context(aud_dec_ctx, audio_codecpar).is_ok()
                    && ff_sys::avcodec::open2(aud_dec_ctx, aud_decoder, ptr::null_mut()).is_ok()
                {
                    aud_sample_rate = (*aud_dec_ctx).sample_rate;
                    aud_nb_channels = (*aud_dec_ctx).ch_layout.nb_channels;
                    log::info!(
                        "dash audio decoder opened sample_rate={aud_sample_rate} \
                         channels={aud_nb_channels}"
                    );
                } else {
                    ff_sys::avcodec::free_context(&mut aud_dec_ctx as *mut *mut _);
                    aud_dec_ctx = ptr::null_mut();
                    audio_stream_idx = -1;
                    log::warn!("dash audio decoder open failed, skipping audio");
                }
            } else {
                audio_stream_idx = -1;
                log::warn!("dash audio decoder alloc failed, skipping audio");
            }
        } else {
            audio_stream_idx = -1;
            log::warn!("dash no audio decoder found, skipping audio");
        }
    }

    // ── 6. Allocate DASH output context ───────────────────────────────────────
    let manifest_path = format!("{output_dir}/manifest.mpd");
    let c_manifest = CString::new(manifest_path.as_str())
        .map_err(|_| ffmpeg_err_msg("manifest path contains null byte"))?;
    let c_dash = c"dash";

    let mut out_ctx: *mut AVFormatContext = ptr::null_mut();
    let ret = avformat_alloc_output_context2(
        &mut out_ctx,
        ptr::null_mut(),
        c_dash.as_ptr(),
        c_manifest.as_ptr(),
    );
    if ret < 0 || out_ctx.is_null() {
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        return Err(ffmpeg_err(ret));
    }

    // ── 7. Set DASH muxer options ──────────────────────────────────────────────
    let seg_duration_str = format!("{}", segment_duration_secs as u32);
    if let Ok(c_seg_dur) = CString::new(seg_duration_str.as_str()) {
        let ret = av_opt_set(
            (*out_ctx).priv_data,
            c"seg_duration".as_ptr(),
            c_seg_dur.as_ptr(),
            0,
        );
        if ret < 0 {
            log::warn!(
                "dash seg_duration option not supported, using default \
                 requested={seg_duration_str} error={}",
                ff_sys::av_error_string(ret)
            );
        }
    }

    // ── 8. Open H.264 video encoder ───────────────────────────────────────────
    let vid_enc_codec = select_h264_encoder().ok_or_else(|| {
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        ffmpeg_err_msg("no H.264 encoder available (tried h264_nvenc, h264_qsv, h264_amf, h264_videotoolbox, libx264, mpeg4)")
    })?;

    let mut vid_enc_ctx = ff_sys::avcodec::alloc_context3(vid_enc_codec).map_err(|e| {
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        ffmpeg_err(e)
    })?;

    (*vid_enc_ctx).width = enc_width;
    (*vid_enc_ctx).height = enc_height;
    (*vid_enc_ctx).time_base.num = 1;
    (*vid_enc_ctx).time_base.den = fps_int;
    (*vid_enc_ctx).framerate.num = fps_int;
    (*vid_enc_ctx).framerate.den = 1;
    (*vid_enc_ctx).pix_fmt = AVPixelFormat_AV_PIX_FMT_YUV420P;
    (*vid_enc_ctx).bit_rate = 2_000_000;

    ff_sys::avcodec::open2(vid_enc_ctx, vid_enc_codec, ptr::null_mut()).map_err(|e| {
        ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        ffmpeg_err(e)
    })?;

    // ── 9. Add video output stream ────────────────────────────────────────────
    let vid_out_stream = avformat_new_stream(out_ctx, vid_enc_codec);
    if vid_out_stream.is_null() {
        ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        return Err(ffmpeg_err_msg("cannot create video output stream"));
    }
    (*vid_out_stream).time_base = (*vid_enc_ctx).time_base;
    let vid_out_stream_idx = ((*out_ctx).nb_streams - 1) as i32;

    if !(*vid_out_stream).codecpar.is_null() {
        (*(*vid_out_stream).codecpar).codec_id = (*vid_enc_ctx).codec_id;
        (*(*vid_out_stream).codecpar).codec_type = ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO;
        (*(*vid_out_stream).codecpar).width = (*vid_enc_ctx).width;
        (*(*vid_out_stream).codecpar).height = (*vid_enc_ctx).height;
        (*(*vid_out_stream).codecpar).format = (*vid_enc_ctx).pix_fmt;
    }

    // ── 10. Open AAC audio encoder and add audio stream (optional) ────────────
    let mut aud_enc_ctx: *mut AVCodecContext = ptr::null_mut();
    let mut aud_out_stream_idx: i32 = -1;
    let mut swr_ctx: *mut SwrContext = ptr::null_mut();

    if audio_stream_idx >= 0 {
        match open_aac_encoder(aud_sample_rate, aud_nb_channels) {
            Ok(ctx) => {
                aud_enc_ctx = ctx;
                let aud_out_stream = avformat_new_stream(out_ctx, ptr::null());
                if aud_out_stream.is_null() {
                    ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
                    log::warn!("dash cannot create audio output stream, skipping audio");
                    audio_stream_idx = -1;
                } else {
                    (*aud_out_stream).time_base.num = 1;
                    (*aud_out_stream).time_base.den = aud_sample_rate;
                    aud_out_stream_idx = ((*out_ctx).nb_streams - 1) as i32;

                    if !(*aud_out_stream).codecpar.is_null() {
                        (*(*aud_out_stream).codecpar).codec_id = (*aud_enc_ctx).codec_id;
                        (*(*aud_out_stream).codecpar).codec_type =
                            ff_sys::AVMediaType_AVMEDIA_TYPE_AUDIO;
                        (*(*aud_out_stream).codecpar).sample_rate = (*aud_enc_ctx).sample_rate;
                        (*(*aud_out_stream).codecpar).format = (*aud_enc_ctx).sample_fmt;
                        let _ = ff_sys::swresample::channel_layout::copy(
                            &mut (*(*aud_out_stream).codecpar).ch_layout,
                            &(*aud_enc_ctx).ch_layout,
                        );
                    }

                    // Set up resampler: decoded audio → FLTP at aud_sample_rate
                    let enc_ch_layout = &(*aud_enc_ctx).ch_layout;
                    let enc_sample_fmt = (*aud_enc_ctx).sample_fmt;
                    let enc_sample_rate = (*aud_enc_ctx).sample_rate;
                    let dec_ch_layout = &(*aud_dec_ctx).ch_layout;
                    let dec_sample_fmt = (*aud_dec_ctx).sample_fmt;
                    let dec_sample_rate = (*aud_dec_ctx).sample_rate;

                    if let Ok(ctx) = ff_sys::swresample::alloc_set_opts2(
                        enc_ch_layout,
                        enc_sample_fmt,
                        enc_sample_rate,
                        dec_ch_layout,
                        dec_sample_fmt,
                        dec_sample_rate,
                    ) {
                        if ff_sys::swresample::init(ctx).is_ok() {
                            swr_ctx = ctx;
                        } else {
                            let mut swr_tmp = ctx;
                            ff_sys::swresample::free(&mut swr_tmp);
                            ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
                            log::warn!("dash swr init failed, skipping audio");
                            audio_stream_idx = -1;
                        }
                    } else {
                        ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
                        log::warn!("dash swr alloc failed, skipping audio");
                        audio_stream_idx = -1;
                    }
                }
            }
            Err(e) => {
                log::warn!("dash aac encoder unavailable: {e}, skipping audio");
                audio_stream_idx = -1;
            }
        }
    }

    // ── 11. Open output file and write header ─────────────────────────────────
    let pb = ff_sys::avformat::open_output(
        Path::new(&manifest_path),
        ff_sys::avformat::avio_flags::WRITE,
    )
    .map_err(|e| {
        cleanup_encoders(vid_enc_ctx, aud_enc_ctx, swr_ctx);
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        ffmpeg_err(e)
    })?;
    (*out_ctx).pb = pb;

    let ret = avformat_write_header(out_ctx, ptr::null_mut());
    if ret < 0 {
        ff_sys::avformat::close_output(&mut (*out_ctx).pb);
        cleanup_encoders(vid_enc_ctx, aud_enc_ctx, swr_ctx);
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        return Err(ffmpeg_err(ret));
    }

    // Close pb now so the DASH muxer can manage its own avio handles for
    // segment files without hitting a locked-file error on Windows.
    ff_sys::avformat::close_output(&mut (*out_ctx).pb);

    log::info!(
        "dash output context ready width={enc_width} height={enc_height} fps={video_fps:.1} \
         audio={}",
        audio_stream_idx >= 0
    );

    // ── 12. Allocate frame and packet buffers ──────────────────────────────────
    let mut pkt = av_packet_alloc();
    if pkt.is_null() {
        av_write_trailer(out_ctx);
        ff_sys::avformat::close_output(&mut (*out_ctx).pb);
        cleanup_encoders(vid_enc_ctx, aud_enc_ctx, swr_ctx);
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        return Err(ffmpeg_err_msg("cannot allocate packet"));
    }

    let vid_dec_frame = av_frame_alloc();
    let vid_enc_frame = av_frame_alloc();
    let aud_dec_frame = av_frame_alloc();
    let aud_enc_frame = av_frame_alloc();

    if vid_dec_frame.is_null()
        || vid_enc_frame.is_null()
        || aud_dec_frame.is_null()
        || aud_enc_frame.is_null()
    {
        free_frames(vid_dec_frame, vid_enc_frame, aud_dec_frame, aud_enc_frame);
        av_packet_free(&mut pkt);
        av_write_trailer(out_ctx);
        ff_sys::avformat::close_output(&mut (*out_ctx).pb);
        cleanup_encoders(vid_enc_ctx, aud_enc_ctx, swr_ctx);
        cleanup_output_ctx(out_ctx);
        cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);
        return Err(ffmpeg_err_msg("cannot allocate frame"));
    }

    // ── 13. Decode–encode loop ─────────────────────────────────────────────────
    let mut video_frame_count: u64 = 0;
    let mut audio_sample_count: i64 = 0;
    let mut sws_ctx: *mut SwsContext = ptr::null_mut();
    let mut last_src_fmt: Option<AVPixelFormat> = None;
    let mut last_src_w: Option<i32> = None;
    let mut last_src_h: Option<i32> = None;

    loop {
        match ff_sys::avformat::read_frame(input_ctx, pkt) {
            Err(e) if e == ff_sys::error_codes::EOF => break,
            Err(_e) => {
                // Non-EOF read errors: continue to try next packet
                av_packet_unref(pkt);
                continue;
            }
            Ok(()) => {}
        }

        let stream_idx = (*pkt).stream_index;

        if stream_idx == video_stream_idx {
            // ── Video path ────────────────────────────────────────────────────
            if ff_sys::avcodec::send_packet(vid_dec_ctx, pkt).is_err() {
                av_packet_unref(pkt);
                continue;
            }
            av_packet_unref(pkt);

            loop {
                match ff_sys::avcodec::receive_frame(vid_dec_ctx, vid_dec_frame) {
                    Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                        break;
                    }
                    Err(_) => break,
                    Ok(()) => {}
                }

                // Force keyframe at intervals
                (*vid_dec_frame).pict_type =
                    if video_frame_count.is_multiple_of(u64::from(keyframe_interval)) {
                        AVPictureType_AV_PICTURE_TYPE_I
                    } else {
                        AVPictureType_AV_PICTURE_TYPE_NONE
                    };

                // Convert decoded frame to YUV420P at encoder dimensions
                let src_fmt = (*vid_dec_frame).format;
                let src_w = (*vid_dec_frame).width;
                let src_h = (*vid_dec_frame).height;

                // Recreate SwsContext when source properties change
                if last_src_fmt != Some(src_fmt)
                    || last_src_w != Some(src_w)
                    || last_src_h != Some(src_h)
                {
                    if !sws_ctx.is_null() {
                        ff_sys::swscale::free_context(sws_ctx);
                        sws_ctx = ptr::null_mut();
                    }
                    if let Ok(ctx) = ff_sys::swscale::get_context(
                        src_w,
                        src_h,
                        src_fmt,
                        enc_width,
                        enc_height,
                        AVPixelFormat_AV_PIX_FMT_YUV420P,
                        ff_sys::swscale::scale_flags::BILINEAR,
                    ) {
                        sws_ctx = ctx;
                        last_src_fmt = Some(src_fmt);
                        last_src_w = Some(src_w);
                        last_src_h = Some(src_h);
                    } else {
                        av_frame_unref(vid_dec_frame);
                        continue;
                    }
                }

                // Prepare encoder frame
                (*vid_enc_frame).format = AVPixelFormat_AV_PIX_FMT_YUV420P;
                (*vid_enc_frame).width = enc_width;
                (*vid_enc_frame).height = enc_height;
                (*vid_enc_frame).pts = video_frame_count as i64;

                let buf_ret = av_frame_get_buffer(vid_enc_frame, 0);
                if buf_ret < 0 {
                    av_frame_unref(vid_dec_frame);
                    continue;
                }

                // Scale decoded frame into encoder frame
                let scale_ok = ff_sys::swscale::scale(
                    sws_ctx,
                    (*vid_dec_frame).data.as_ptr() as *const *const u8,
                    (*vid_dec_frame).linesize.as_ptr(),
                    0,
                    src_h,
                    (*vid_enc_frame).data.as_mut_ptr().cast_const(),
                    (*vid_enc_frame).linesize.as_mut_ptr(),
                );

                if scale_ok.is_ok()
                    && ff_sys::avcodec::send_frame(vid_enc_ctx, vid_enc_frame).is_ok()
                {
                    drain_encoder(vid_enc_ctx, out_ctx, vid_out_stream_idx);
                }

                av_frame_unref(vid_enc_frame);
                av_frame_unref(vid_dec_frame);
                video_frame_count += 1;
            }
        } else if stream_idx == audio_stream_idx && !aud_dec_ctx.is_null() {
            // ── Audio path ────────────────────────────────────────────────────
            if ff_sys::avcodec::send_packet(aud_dec_ctx, pkt).is_err() {
                av_packet_unref(pkt);
                continue;
            }
            av_packet_unref(pkt);

            loop {
                match ff_sys::avcodec::receive_frame(aud_dec_ctx, aud_dec_frame) {
                    Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                        break;
                    }
                    Err(_) => break,
                    Ok(()) => {}
                }

                let enc_frame_size = if (*aud_enc_ctx).frame_size > 0 {
                    (*aud_enc_ctx).frame_size
                } else {
                    (*aud_dec_frame).nb_samples
                };

                (*aud_enc_frame).format = (*aud_enc_ctx).sample_fmt;
                (*aud_enc_frame).sample_rate = (*aud_enc_ctx).sample_rate;
                (*aud_enc_frame).nb_samples = enc_frame_size;
                let _ = ff_sys::swresample::channel_layout::copy(
                    &mut (*aud_enc_frame).ch_layout,
                    &(*aud_enc_ctx).ch_layout,
                );

                let buf_ret = av_frame_get_buffer(aud_enc_frame, 0);
                if buf_ret < 0 {
                    av_frame_unref(aud_dec_frame);
                    continue;
                }

                let in_data = (*aud_dec_frame).data.as_ptr() as *const *const u8;
                let in_samples = (*aud_dec_frame).nb_samples;

                let samples_out = ff_sys::swresample::convert(
                    swr_ctx,
                    (*aud_enc_frame).data.as_mut_ptr(),
                    enc_frame_size,
                    in_data,
                    in_samples,
                );

                if let Ok(n) = samples_out
                    && n > 0
                {
                    (*aud_enc_frame).nb_samples = n;
                    (*aud_enc_frame).pts = audio_sample_count;
                    if ff_sys::avcodec::send_frame(aud_enc_ctx, aud_enc_frame).is_ok() {
                        drain_encoder(aud_enc_ctx, out_ctx, aud_out_stream_idx);
                    }
                    audio_sample_count += i64::from(n);
                }

                av_frame_unref(aud_enc_frame);
                av_frame_unref(aud_dec_frame);
            }
        } else {
            av_packet_unref(pkt);
        }
    }

    // ── 14. Flush encoders ────────────────────────────────────────────────────
    let _ = ff_sys::avcodec::send_frame(vid_enc_ctx, ptr::null());
    drain_encoder(vid_enc_ctx, out_ctx, vid_out_stream_idx);

    if !aud_enc_ctx.is_null() {
        // Flush resampler
        if !swr_ctx.is_null() {
            let enc_frame_size = if (*aud_enc_ctx).frame_size > 0 {
                (*aud_enc_ctx).frame_size
            } else {
                1024
            };
            (*aud_enc_frame).format = (*aud_enc_ctx).sample_fmt;
            (*aud_enc_frame).sample_rate = (*aud_enc_ctx).sample_rate;
            (*aud_enc_frame).nb_samples = enc_frame_size;
            let _ = ff_sys::swresample::channel_layout::copy(
                &mut (*aud_enc_frame).ch_layout,
                &(*aud_enc_ctx).ch_layout,
            );
            if av_frame_get_buffer(aud_enc_frame, 0) == 0 {
                if let Ok(n) = ff_sys::swresample::convert(
                    swr_ctx,
                    (*aud_enc_frame).data.as_mut_ptr(),
                    enc_frame_size,
                    ptr::null(),
                    0,
                ) && n > 0
                {
                    (*aud_enc_frame).nb_samples = n;
                    (*aud_enc_frame).pts = audio_sample_count;
                    if ff_sys::avcodec::send_frame(aud_enc_ctx, aud_enc_frame).is_ok() {
                        drain_encoder(aud_enc_ctx, out_ctx, aud_out_stream_idx);
                    }
                }
                av_frame_unref(aud_enc_frame);
            }
        }
        let _ = ff_sys::avcodec::send_frame(aud_enc_ctx, ptr::null());
        drain_encoder(aud_enc_ctx, out_ctx, aud_out_stream_idx);
    }

    // ── 15. Finalize ──────────────────────────────────────────────────────────
    av_write_trailer(out_ctx);
    // pb was already closed after avformat_write_header; skip double-close.

    // ── Cleanup ───────────────────────────────────────────────────────────────
    free_frames(vid_dec_frame, vid_enc_frame, aud_dec_frame, aud_enc_frame);
    av_packet_free(&mut pkt);

    if !sws_ctx.is_null() {
        ff_sys::swscale::free_context(sws_ctx);
    }

    cleanup_encoders(vid_enc_ctx, aud_enc_ctx, swr_ctx);
    cleanup_output_ctx(out_ctx);
    cleanup_decoders(vid_dec_ctx, aud_dec_ctx, &mut input_ctx);

    log::info!(
        "dash write complete video_frames={video_frame_count} \
         audio_samples={audio_sample_count}"
    );

    Ok(())
}

// ============================================================================
// Helper: drain all available encoded packets into the output muxer
// ============================================================================

unsafe fn drain_encoder(
    enc_ctx: *mut AVCodecContext,
    out_ctx: *mut AVFormatContext,
    stream_idx: i32,
) {
    let mut pkt = av_packet_alloc();
    if pkt.is_null() {
        return;
    }

    loop {
        match ff_sys::avcodec::receive_packet(enc_ctx, pkt) {
            Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                break;
            }
            Err(_) => break,
            Ok(()) => {}
        }

        (*pkt).stream_index = stream_idx;
        let ret = av_interleaved_write_frame(out_ctx, pkt);
        av_packet_unref(pkt);
        if ret < 0 {
            log::warn!(
                "dash av_interleaved_write_frame failed \
                 stream_index={stream_idx} error={}",
                ff_sys::av_error_string(ret)
            );
            break;
        }
    }

    av_packet_free(&mut pkt);
}

// ============================================================================
// Helper: select best available H.264 encoder
// ============================================================================

unsafe fn select_h264_encoder() -> Option<*const ff_sys::AVCodec> {
    let candidates = [
        "h264_nvenc",
        "h264_qsv",
        "h264_amf",
        "h264_videotoolbox",
        "libx264",
        "mpeg4",
    ];
    for name in candidates {
        if let Ok(c_name) = CString::new(name)
            && let Some(codec) = ff_sys::avcodec::find_encoder_by_name(c_name.as_ptr())
        {
            log::info!("dash selected video encoder encoder={name}");
            return Some(codec);
        }
    }
    None
}

// ============================================================================
// Helper: open AAC encoder
// ============================================================================

unsafe fn open_aac_encoder(
    sample_rate: i32,
    nb_channels: i32,
) -> Result<*mut AVCodecContext, StreamError> {
    let codec = ff_sys::avcodec::find_encoder_by_name(c"aac".as_ptr())
        .or_else(|| ff_sys::avcodec::find_encoder_by_name(c"libfdk_aac".as_ptr()))
        .ok_or_else(|| ffmpeg_err_msg("no AAC encoder available"))?;

    let mut ctx = ff_sys::avcodec::alloc_context3(codec).map_err(ffmpeg_err)?;

    (*ctx).sample_rate = sample_rate;
    (*ctx).sample_fmt = ff_sys::swresample::sample_format::FLTP;
    (*ctx).bit_rate = 192_000;
    (*ctx).time_base.num = 1;
    (*ctx).time_base.den = sample_rate;
    ff_sys::swresample::channel_layout::set_default(&mut (*ctx).ch_layout, nb_channels);

    ff_sys::avcodec::open2(ctx, codec, ptr::null_mut()).map_err(|e| {
        ff_sys::avcodec::free_context(&mut ctx as *mut *mut _);
        ffmpeg_err(e)
    })?;

    log::info!("dash aac encoder opened sample_rate={sample_rate} channels={nb_channels}");
    Ok(ctx)
}

// ============================================================================
// Cleanup helpers (safe to call with null pointers)
// ============================================================================

unsafe fn cleanup_decoders(
    mut vid_dec_ctx: *mut AVCodecContext,
    mut aud_dec_ctx: *mut AVCodecContext,
    input_ctx: *mut *mut AVFormatContext,
) {
    if !vid_dec_ctx.is_null() {
        ff_sys::avcodec::free_context(&mut vid_dec_ctx as *mut *mut _);
    }
    if !aud_dec_ctx.is_null() {
        ff_sys::avcodec::free_context(&mut aud_dec_ctx as *mut *mut _);
    }
    ff_sys::avformat::close_input(input_ctx);
}

unsafe fn cleanup_encoders(
    mut vid_enc_ctx: *mut AVCodecContext,
    mut aud_enc_ctx: *mut AVCodecContext,
    mut swr_ctx: *mut SwrContext,
) {
    if !vid_enc_ctx.is_null() {
        ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
    }
    if !aud_enc_ctx.is_null() {
        ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
    }
    if !swr_ctx.is_null() {
        ff_sys::swresample::free(&mut swr_ctx);
    }
}

unsafe fn cleanup_output_ctx(mut out_ctx: *mut AVFormatContext) {
    if !out_ctx.is_null() {
        avformat_free_context(out_ctx);
        out_ctx = ptr::null_mut();
        let _ = out_ctx; // suppress unused warning
    }
}

unsafe fn free_frames(
    mut vid_dec: *mut AVFrame,
    mut vid_enc: *mut AVFrame,
    mut aud_dec: *mut AVFrame,
    mut aud_enc: *mut AVFrame,
) {
    if !vid_dec.is_null() {
        av_frame_free(&mut vid_dec as *mut *mut _);
    }
    if !vid_enc.is_null() {
        av_frame_free(&mut vid_enc as *mut *mut _);
    }
    if !aud_dec.is_null() {
        av_frame_free(&mut aud_dec as *mut *mut _);
    }
    if !aud_enc.is_null() {
        av_frame_free(&mut aud_enc as *mut *mut _);
    }
}
