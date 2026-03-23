//! Shared low-level packet-writing and encoder utilities for HLS and DASH muxers.
//!
//! This module provides:
//! - [`drain_encoder`]: drains encoded packets from a codec context and writes them to a mux context
//! - [`select_h264_encoder`]: picks the best available H.264 encoder
//! - [`open_aac_encoder`]: opens an AAC encoder context
//! - [`ffmpeg_err`]: maps an `FFmpeg` error code to [`StreamError::Ffmpeg`]

// This module is intentionally unsafe — it drives the FFmpeg C API directly.
#![allow(unsafe_code)]
// Rust 2024: Allow unsafe operations in unsafe functions for FFmpeg C API
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::ref_as_ptr)]

use ff_sys::{
    AVCodec, AVCodecContext, AVFormatContext, AVRational, av_interleaved_write_frame,
    av_packet_alloc, av_packet_free, av_packet_rescale_ts, av_packet_unref, av_rescale_q,
};

use crate::error::StreamError;

// ============================================================================
// Error helpers
// ============================================================================

/// Map an `FFmpeg` negative return code to [`StreamError::Ffmpeg`].
pub(crate) fn ffmpeg_err(code: i32) -> StreamError {
    StreamError::Ffmpeg {
        code,
        message: ff_sys::av_error_string(code),
    }
}

/// Build a [`StreamError::Ffmpeg`] from a plain message (no numeric code).
pub(crate) fn ffmpeg_err_msg(msg: &str) -> StreamError {
    StreamError::Ffmpeg {
        code: 0,
        message: msg.to_owned(),
    }
}

// ============================================================================
// Encoder selection helpers
// ============================================================================

/// Return the best available H.264 encoder.
///
/// Tries hardware encoders first (`h264_nvenc`, `h264_qsv`, `h264_amf`,
/// `h264_videotoolbox`), then software (`libx264`, `mpeg4`).
///
/// # Safety
///
/// Must be called after `ff_sys::ensure_initialized()`.
pub(crate) unsafe fn select_h264_encoder(log_prefix: &str) -> Option<*const AVCodec> {
    let candidates = [
        "h264_nvenc",
        "h264_qsv",
        "h264_amf",
        "h264_videotoolbox",
        "libx264",
        "mpeg4",
    ];
    for name in candidates {
        if let Ok(c_name) = std::ffi::CString::new(name)
            && let Some(codec) = ff_sys::avcodec::find_encoder_by_name(c_name.as_ptr())
        {
            log::info!("{log_prefix} selected video encoder encoder={name}");
            return Some(codec);
        }
    }
    None
}

/// Open an AAC audio encoder configured for `sample_rate` Hz and `nb_channels` channels.
///
/// Tries `aac` first, then `libfdk_aac`.
///
/// # Safety
///
/// Must be called after `ff_sys::ensure_initialized()`.
pub(crate) unsafe fn open_aac_encoder(
    sample_rate: i32,
    nb_channels: i32,
    bit_rate: i64,
    log_prefix: &str,
) -> Result<*mut AVCodecContext, StreamError> {
    let codec = ff_sys::avcodec::find_encoder_by_name(c"aac".as_ptr())
        .or_else(|| ff_sys::avcodec::find_encoder_by_name(c"libfdk_aac".as_ptr()))
        .ok_or_else(|| ffmpeg_err_msg("no AAC encoder available (tried aac, libfdk_aac)"))?;

    let mut ctx = ff_sys::avcodec::alloc_context3(codec).map_err(ffmpeg_err)?;

    (*ctx).sample_rate = sample_rate;
    (*ctx).sample_fmt = ff_sys::swresample::sample_format::FLTP;
    (*ctx).bit_rate = bit_rate;
    (*ctx).time_base.num = 1;
    (*ctx).time_base.den = sample_rate;
    ff_sys::swresample::channel_layout::set_default(&mut (*ctx).ch_layout, nb_channels);

    ff_sys::avcodec::open2(ctx, codec, std::ptr::null_mut()).map_err(|e| {
        ff_sys::avcodec::free_context(&mut ctx as *mut *mut _);
        ffmpeg_err(e)
    })?;

    log::info!(
        "{log_prefix} aac encoder opened \
         sample_rate={sample_rate} channels={nb_channels} bit_rate={bit_rate}"
    );
    Ok(ctx)
}

/// Drain all available encoded packets from `enc_ctx` into `out_ctx`.
///
/// For each packet received from the encoder:
/// 1. Overrides `pkt->duration` (before rescaling) with one frame's worth of
///    time expressed in `enc_ctx->time_base` units — computed from `frame_period`
///    at drain time.  Some encoders (e.g. mpeg4) lazily mutate their `time_base`
///    on the first `avcodec_send_frame` call, so `enc_ctx->time_base` must be
///    read here, not in the calling code.  The HLS/DASH muxers accumulate
///    `pkt->duration` to determine segment boundaries and `TARGETDURATION`; a
///    near-zero duration produces `#EXT-X-TARGETDURATION:0`.
/// 2. Rescales `pts`, `dts`, and `duration` from `enc_ctx->time_base` to the
///    output stream's `time_base` using `av_packet_rescale_ts`.
/// 3. Writes the packet with `av_interleaved_write_frame`.
///
/// # Parameters
///
/// - `frame_period`: rational duration of one encoder frame, expressed as a
///   fraction of a second (e.g. `{1, fps}` for video, `{frame_size, sample_rate}`
///   for audio).  Converted to `enc_ctx->time_base` units inside this function so
///   it is immune to lazy time-base mutations.
///
/// # Safety
///
/// - `enc_ctx` must be a valid, fully-opened `AVCodecContext` with at least
///   one call to `avcodec_send_frame` preceding this call.
/// - `out_ctx` must be a valid `AVFormatContext` whose header has been written.
/// - `stream_idx` must be a valid index into `out_ctx`'s stream array.
pub(crate) unsafe fn drain_encoder(
    enc_ctx: *mut AVCodecContext,
    out_ctx: *mut AVFormatContext,
    stream_idx: i32,
    log_prefix: &str,
    frame_period: AVRational,
) {
    let mut pkt = av_packet_alloc();
    if pkt.is_null() {
        return;
    }

    // SAFETY: out_ctx is valid and stream_idx is a valid stream index.
    let stream_tb = (*(*(*out_ctx).streams.add(stream_idx as usize))).time_base;
    // SAFETY: enc_ctx is a valid, open codec context.
    // Read enc_tb HERE — some encoders (mpeg4) mutate time_base lazily on first
    // send_frame, so the value may differ from what the caller observed earlier.
    let enc_tb = (*enc_ctx).time_base;

    // Compute the correct per-frame duration in enc_tb units using the live enc_tb.
    // av_rescale_q converts 1 unit of `frame_period` (e.g. 1/fps second) into enc_tb ticks.
    let frame_dur_enc_tb = av_rescale_q(1, frame_period, enc_tb);

    loop {
        match ff_sys::avcodec::receive_packet(enc_ctx, pkt) {
            Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                break;
            }
            Err(_) => break,
            Ok(()) => {}
        }

        // Always override duration with the correct per-frame value BEFORE rescaling.
        if frame_dur_enc_tb > 0 {
            (*pkt).duration = frame_dur_enc_tb;
        }

        // Rescale pts/dts/duration from encoder time_base to stream time_base.
        // SAFETY: pkt is valid; enc_tb and stream_tb are valid AVRational values.
        av_packet_rescale_ts(pkt, enc_tb, stream_tb);

        (*pkt).stream_index = stream_idx;
        let ret = av_interleaved_write_frame(out_ctx, pkt);
        av_packet_unref(pkt);
        if ret < 0 {
            log::warn!(
                "{log_prefix} av_interleaved_write_frame failed \
                 stream_index={stream_idx} error={}",
                ff_sys::av_error_string(ret)
            );
            break;
        }
    }

    av_packet_free(&mut pkt);
}
