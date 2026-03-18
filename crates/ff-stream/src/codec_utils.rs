//! Shared low-level packet-writing utilities for HLS and DASH muxers.
//!
//! This module provides [`drain_encoder`], which drains encoded packets from a
//! codec context, rescales their timestamps to the output stream's time base,
//! and writes them to the mux context via `av_interleaved_write_frame`.

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
    AVCodecContext, AVFormatContext, AVRational, av_interleaved_write_frame, av_packet_alloc,
    av_packet_free, av_packet_rescale_ts, av_packet_unref, av_rescale_q,
};

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
