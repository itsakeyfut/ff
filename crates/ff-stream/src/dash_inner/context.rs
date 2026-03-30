//! `AVFormatContext` DASH setup/cleanup helpers and the `RenditionState` type.

use std::ptr;

use ff_sys::{AVCodecContext, AVFormatContext, AVFrame, AVPixelFormat, SwrContext, SwsContext};
use ff_sys::{av_frame_free, avformat_free_context};

// ============================================================================
// Per-rendition encoder state for ABR
// ============================================================================

/// Per-rendition encoder state for the ABR DASH mux loop.
pub(super) struct RenditionState {
    pub(super) vid_enc_ctx: *mut AVCodecContext,
    pub(super) vid_out_stream_idx: i32,
    pub(super) enc_width: i32,
    pub(super) enc_height: i32,
    pub(super) sws_ctx: *mut SwsContext,
    pub(super) last_src_fmt: Option<AVPixelFormat>,
    pub(super) last_src_w: Option<i32>,
    pub(super) last_src_h: Option<i32>,
}

// ============================================================================
// Cleanup helpers (safe to call with null pointers)
// ============================================================================

pub(super) unsafe fn cleanup_decoders(
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

pub(super) unsafe fn cleanup_encoders(
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

pub(super) unsafe fn cleanup_output_ctx(mut out_ctx: *mut AVFormatContext) {
    if !out_ctx.is_null() {
        avformat_free_context(out_ctx);
        out_ctx = ptr::null_mut();
        let _ = out_ctx; // suppress unused warning
    }
}

pub(super) unsafe fn free_frames(
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

/// Free all encoder contexts and `SwsContext`s in `states`.
///
/// Safe to call at any point after the Vec starts being populated.
pub(super) unsafe fn cleanup_renditions(states: &mut Vec<RenditionState>) {
    for state in states.iter_mut() {
        if !state.vid_enc_ctx.is_null() {
            ff_sys::avcodec::free_context(&mut state.vid_enc_ctx as *mut *mut _);
        }
        if !state.sws_ctx.is_null() {
            ff_sys::swscale::free_context(state.sws_ctx);
            state.sws_ctx = ptr::null_mut();
        }
    }
    states.clear();
}
