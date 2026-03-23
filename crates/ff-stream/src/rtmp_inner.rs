//! Internal RTMP state — all `unsafe` `FFmpeg` calls live here.
//!
//! [`RtmpInner`] owns all raw `FFmpeg` contexts and the RTMP network
//! connection. It is created by [`crate::rtmp::RtmpOutput::build`] and driven
//! by the safe wrappers in [`crate::rtmp`].
//!
//! Unlike the HLS/DASH muxers, the RTMP connection (`out_ctx->pb`) is kept
//! open for the entire session and is only closed after [`av_write_trailer`]
//! in [`RtmpInner::flush_and_close`].

// This module is intentionally unsafe — it drives the FFmpeg C API directly.
#![allow(unsafe_code)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::ref_as_ptr)]
#![allow(clippy::too_many_lines)]

use std::ffi::CString;
use std::path::Path;
use std::ptr;

use ff_format::{AudioFrame, PixelFormat, SampleFormat, VideoFrame};
use ff_sys::{
    AVCodecContext, AVFormatContext, AVFrame, AVPixelFormat, AVPixelFormat_AV_PIX_FMT_NONE,
    AVPixelFormat_AV_PIX_FMT_YUV420P, AVRational, AVSampleFormat, SwrContext, SwsContext,
    av_frame_alloc, av_frame_free, av_frame_get_buffer, av_frame_unref, av_rescale_q,
    av_write_trailer, avformat_alloc_output_context2, avformat_free_context, avformat_new_stream,
    avformat_write_header,
};

use crate::codec_utils::{drain_encoder, ffmpeg_err, ffmpeg_err_msg, open_aac_encoder};
use crate::error::StreamError;

// ============================================================================
// Pixel-format conversion (local — ff-format has no FFmpeg dependency)
// ============================================================================

fn pixel_format_to_av(fmt: PixelFormat) -> AVPixelFormat {
    match fmt {
        PixelFormat::Yuv420p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P,
        PixelFormat::Yuv422p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P,
        PixelFormat::Yuv444p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P,
        PixelFormat::Rgb24 => ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24,
        PixelFormat::Bgr24 => ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24,
        PixelFormat::Rgba => ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA,
        PixelFormat::Bgra => ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA,
        PixelFormat::Nv12 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV12,
        PixelFormat::Nv21 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV21,
        PixelFormat::Yuv420p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE,
        PixelFormat::Yuv422p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P10LE,
        PixelFormat::Yuv444p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P10LE,
        PixelFormat::Yuva444p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUVA444P10LE,
        PixelFormat::P010le => ff_sys::AVPixelFormat_AV_PIX_FMT_P010LE,
        PixelFormat::Gray8 => ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8,
        PixelFormat::Gbrpf32le => ff_sys::AVPixelFormat_AV_PIX_FMT_GBRPF32LE,
        PixelFormat::Other(_) | _ => AVPixelFormat_AV_PIX_FMT_NONE,
    }
}

fn sample_format_to_av(fmt: SampleFormat) -> AVSampleFormat {
    match fmt {
        SampleFormat::U8 => ff_sys::swresample::sample_format::U8,
        SampleFormat::I16 => ff_sys::swresample::sample_format::S16,
        SampleFormat::I32 => ff_sys::swresample::sample_format::S32,
        SampleFormat::F32 => ff_sys::swresample::sample_format::FLT,
        SampleFormat::F64 => ff_sys::swresample::sample_format::DBL,
        SampleFormat::U8p => ff_sys::swresample::sample_format::U8P,
        SampleFormat::I16p => ff_sys::swresample::sample_format::S16P,
        SampleFormat::I32p => ff_sys::swresample::sample_format::S32P,
        SampleFormat::F32p => ff_sys::swresample::sample_format::FLTP,
        SampleFormat::F64p => ff_sys::swresample::sample_format::DBLP,
        SampleFormat::Other(_) | _ => ff_sys::swresample::sample_format::NONE,
    }
}

// ============================================================================
// RtmpInner
// ============================================================================

/// Owns all raw `FFmpeg` contexts and the RTMP network connection.
///
/// Created by [`RtmpInner::open`]; consumed by [`RtmpInner::flush_and_close`].
/// After `flush_and_close` returns, calling any other method is undefined behaviour;
/// the safe wrapper in `rtmp.rs` prevents this via the `finished` guard.
pub(crate) struct RtmpInner {
    out_ctx: *mut AVFormatContext,
    vid_enc_ctx: *mut AVCodecContext,
    aud_enc_ctx: *mut AVCodecContext,
    /// Null until first `push_audio` call; recreated if input format changes.
    swr_ctx: *mut SwrContext,
    /// Null until swscale is needed; recreated if source dimensions/format change.
    sws_ctx: *mut SwsContext,
    vid_enc_frame: *mut AVFrame,
    aud_enc_frame: *mut AVFrame,
    vid_out_stream_idx: i32,
    aud_out_stream_idx: i32,
    video_frame_count: u64,
    audio_pts: i64,
    fps_int: i32,
    enc_width: i32,
    enc_height: i32,
    /// AAC encoder `frame_size` (typically 1024); set after `avcodec_open2`.
    aud_frame_size: i32,
    aud_sample_rate: i32,
    url: String,
    /// Tracks the last swscale source so we can detect changes.
    last_sws_src_fmt: Option<AVPixelFormat>,
    last_sws_src_w: Option<i32>,
    last_sws_src_h: Option<i32>,
    /// Tracks the last swr input so we can detect format changes.
    last_swr_in_fmt: Option<AVSampleFormat>,
    last_swr_in_rate: Option<i32>,
    last_swr_in_channels: Option<i32>,
}

// SAFETY: RtmpInner exclusively owns all FFmpeg contexts.
// FFmpeg contexts are not safe for concurrent access, but ownership transfer
// between threads is safe (there is no shared state).
unsafe impl Send for RtmpInner {}

impl RtmpInner {
    /// Open the `FFmpeg` context and establish the RTMP connection.
    ///
    /// # Parameters
    ///
    /// - `url`: RTMP ingest URL (e.g. `rtmp://ingest.example.com/live/key`).
    /// - `enc_width`, `enc_height`, `fps_int`: video encoder dimensions and frame rate.
    /// - `video_bitrate`: video encoder bit rate in bits/s.
    /// - `aud_sample_rate`, `aud_channels`, `aud_bitrate`: audio encoder parameters.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        url: &str,
        enc_width: i32,
        enc_height: i32,
        fps_int: i32,
        video_bitrate: u64,
        aud_sample_rate: i32,
        aud_channels: i32,
        aud_bitrate: i64,
    ) -> Result<Self, StreamError> {
        // SAFETY: All FFmpeg resources are managed within this function; the
        // returned RtmpInner takes exclusive ownership of every pointer.
        unsafe {
            Self::open_unsafe(
                url,
                enc_width,
                enc_height,
                fps_int,
                video_bitrate,
                aud_sample_rate,
                aud_channels,
                aud_bitrate,
            )
        }
    }

    /// Encode and mux one video frame.
    pub(crate) fn push_video(&mut self, frame: &VideoFrame) -> Result<(), StreamError> {
        // SAFETY: self was initialised by open() and is not yet finished.
        unsafe { self.push_video_unsafe(frame) }
    }

    /// Encode and mux one audio frame.
    pub(crate) fn push_audio(&mut self, frame: &AudioFrame) {
        // SAFETY: self was initialised by open() and is not yet finished.
        unsafe {
            self.push_audio_unsafe(frame);
        }
    }

    /// Flush both encoders, write the FLV trailer, and close the RTMP connection.
    /// Consumes `self`.
    pub(crate) fn flush_and_close(mut self) {
        // SAFETY: self was initialised by open(); flush_and_close is called once.
        unsafe {
            self.flush_and_close_unsafe();
        }
    }

    // ── Private unsafe implementations ───────────────────────────────────────

    #[allow(unsafe_op_in_unsafe_fn)]
    #[allow(clippy::too_many_arguments)]
    unsafe fn open_unsafe(
        url: &str,
        enc_width: i32,
        enc_height: i32,
        fps_int: i32,
        video_bitrate: u64,
        aud_sample_rate: i32,
        aud_channels: i32,
        aud_bitrate: i64,
    ) -> Result<Self, StreamError> {
        ff_sys::ensure_initialized();

        // ── 1. Allocate FLV output context with RTMP URL ───────────────────
        let c_url = CString::new(url).map_err(|_| ffmpeg_err_msg("RTMP URL contains null byte"))?;

        let mut out_ctx: *mut AVFormatContext = ptr::null_mut();
        let ret = avformat_alloc_output_context2(
            &mut out_ctx,
            ptr::null_mut(),
            c"flv".as_ptr(),
            c_url.as_ptr(),
        );
        if ret < 0 || out_ctx.is_null() {
            return Err(ffmpeg_err(ret));
        }

        // ── 2. Open H.264 video encoder ────────────────────────────────────
        let vid_enc_codec = crate::codec_utils::select_h264_encoder("rtmp").ok_or_else(|| {
            avformat_free_context(out_ctx);
            ffmpeg_err_msg(
                "no H.264 encoder available \
                     (tried h264_nvenc, h264_qsv, h264_amf, h264_videotoolbox, libx264, mpeg4)",
            )
        })?;

        let mut vid_enc_ctx = ff_sys::avcodec::alloc_context3(vid_enc_codec).map_err(|e| {
            avformat_free_context(out_ctx);
            ffmpeg_err(e)
        })?;

        (*vid_enc_ctx).width = enc_width;
        (*vid_enc_ctx).height = enc_height;
        (*vid_enc_ctx).pix_fmt = AVPixelFormat_AV_PIX_FMT_YUV420P;
        (*vid_enc_ctx).time_base.num = 1;
        (*vid_enc_ctx).time_base.den = fps_int;
        (*vid_enc_ctx).framerate.num = fps_int;
        (*vid_enc_ctx).framerate.den = 1;
        // GOP size of 2 s gives a reasonable keyframe interval for RTMP.
        (*vid_enc_ctx).gop_size = fps_int * 2;
        (*vid_enc_ctx).bit_rate = video_bitrate as i64;

        ff_sys::avcodec::open2(vid_enc_ctx, vid_enc_codec, ptr::null_mut()).map_err(|e| {
            ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
            avformat_free_context(out_ctx);
            ffmpeg_err(e)
        })?;

        // ── 3. Add video output stream ─────────────────────────────────────
        let vid_out_stream = avformat_new_stream(out_ctx, vid_enc_codec);
        if vid_out_stream.is_null() {
            ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
            avformat_free_context(out_ctx);
            return Err(ffmpeg_err_msg("cannot create video output stream"));
        }
        (*vid_out_stream).time_base = (*vid_enc_ctx).time_base;
        let vid_out_stream_idx = ((*out_ctx).nb_streams - 1) as i32;

        // SAFETY: vid_out_stream and vid_enc_ctx are valid; avcodec_open2 has been called.
        ff_sys::avcodec::parameters_from_context((*vid_out_stream).codecpar, vid_enc_ctx).map_err(
            |e| {
                ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
                avformat_free_context(out_ctx);
                ffmpeg_err(e)
            },
        )?;

        // ── 4. Open AAC audio encoder ──────────────────────────────────────
        let mut aud_enc_ctx = open_aac_encoder(aud_sample_rate, aud_channels, aud_bitrate, "rtmp")
            .inspect_err(|_| {
                ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
                avformat_free_context(out_ctx);
            })?;

        let aud_frame_size = if (*aud_enc_ctx).frame_size > 0 {
            (*aud_enc_ctx).frame_size
        } else {
            1024
        };

        // ── 5. Add audio output stream ─────────────────────────────────────
        let aud_out_stream = avformat_new_stream(out_ctx, ptr::null());
        if aud_out_stream.is_null() {
            ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
            ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
            avformat_free_context(out_ctx);
            return Err(ffmpeg_err_msg("cannot create audio output stream"));
        }
        (*aud_out_stream).time_base.num = 1;
        (*aud_out_stream).time_base.den = aud_sample_rate;
        let aud_out_stream_idx = ((*out_ctx).nb_streams - 1) as i32;

        // SAFETY: aud_out_stream and aud_enc_ctx are valid.
        if ff_sys::avcodec::parameters_from_context((*aud_out_stream).codecpar, aud_enc_ctx)
            .is_err()
        {
            log::warn!("rtmp audio stream codecpar copy failed");
        }

        // ── 6. Allocate encoder frames ─────────────────────────────────────
        let vid_enc_frame = av_frame_alloc();
        let aud_enc_frame = av_frame_alloc();

        if vid_enc_frame.is_null() || aud_enc_frame.is_null() {
            if !vid_enc_frame.is_null() {
                av_frame_free(&mut (vid_enc_frame as *mut _));
            }
            if !aud_enc_frame.is_null() {
                av_frame_free(&mut (aud_enc_frame as *mut _));
            }
            ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
            ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
            avformat_free_context(out_ctx);
            return Err(ffmpeg_err_msg("cannot allocate encoder frames"));
        }

        // ── 7. Open RTMP connection and write FLV header ───────────────────
        // Unlike HLS/DASH, RTMP uses a persistent network connection.
        // We use avio_open via avformat::open_output with the URL as the path.
        // SAFETY: url is a valid null-terminated C string (validated above).
        let pb = ff_sys::avformat::open_output(Path::new(url), ff_sys::avformat::avio_flags::WRITE)
            .map_err(|e| {
                av_frame_free(&mut (aud_enc_frame as *mut _));
                av_frame_free(&mut (vid_enc_frame as *mut _));
                ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
                ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
                avformat_free_context(out_ctx);
                ffmpeg_err(e)
            })?;
        (*out_ctx).pb = pb;

        let ret = avformat_write_header(out_ctx, ptr::null_mut());
        if ret < 0 {
            ff_sys::avformat::close_output(&mut (*out_ctx).pb);
            av_frame_free(&mut (aud_enc_frame as *mut _));
            av_frame_free(&mut (vid_enc_frame as *mut _));
            ff_sys::avcodec::free_context(&mut aud_enc_ctx as *mut *mut _);
            ff_sys::avcodec::free_context(&mut vid_enc_ctx as *mut *mut _);
            avformat_free_context(out_ctx);
            return Err(ffmpeg_err(ret));
        }

        // NOTE: pb is intentionally kept open. RTMP is a persistent TCP connection;
        // closing pb here would terminate the stream.

        log::info!(
            "rtmp output opened url={url} video={enc_width}x{enc_height}@{fps_int}fps \
             bitrate={video_bitrate}bps"
        );

        Ok(Self {
            out_ctx,
            vid_enc_ctx,
            aud_enc_ctx,
            swr_ctx: ptr::null_mut(),
            sws_ctx: ptr::null_mut(),
            vid_enc_frame,
            aud_enc_frame,
            vid_out_stream_idx,
            aud_out_stream_idx,
            video_frame_count: 0,
            audio_pts: 0,
            fps_int,
            enc_width,
            enc_height,
            aud_frame_size,
            aud_sample_rate,
            url: url.to_owned(),
            last_sws_src_fmt: None,
            last_sws_src_w: None,
            last_sws_src_h: None,
            last_swr_in_fmt: None,
            last_swr_in_rate: None,
            last_swr_in_channels: None,
        })
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn push_video_unsafe(&mut self, frame: &VideoFrame) -> Result<(), StreamError> {
        let src_fmt = pixel_format_to_av(frame.format());
        let src_w = frame.width() as i32;
        let src_h = frame.height() as i32;
        let needs_conversion = src_fmt != AVPixelFormat_AV_PIX_FMT_YUV420P
            || src_w != self.enc_width
            || src_h != self.enc_height
            || src_fmt == AVPixelFormat_AV_PIX_FMT_NONE;

        if needs_conversion {
            // (Re)create SwsContext when source properties change.
            if self.last_sws_src_fmt != Some(src_fmt)
                || self.last_sws_src_w != Some(src_w)
                || self.last_sws_src_h != Some(src_h)
            {
                if !self.sws_ctx.is_null() {
                    ff_sys::swscale::free_context(self.sws_ctx);
                    self.sws_ctx = ptr::null_mut();
                }
                match ff_sys::swscale::get_context(
                    src_w,
                    src_h,
                    src_fmt,
                    self.enc_width,
                    self.enc_height,
                    AVPixelFormat_AV_PIX_FMT_YUV420P,
                    ff_sys::swscale::scale_flags::BILINEAR,
                ) {
                    Ok(ctx) => {
                        self.sws_ctx = ctx;
                        self.last_sws_src_fmt = Some(src_fmt);
                        self.last_sws_src_w = Some(src_w);
                        self.last_sws_src_h = Some(src_h);
                    }
                    Err(_) => {
                        return Err(ffmpeg_err_msg(
                            "rtmp swscale context creation failed for video frame",
                        ));
                    }
                }
            }

            (*self.vid_enc_frame).format = AVPixelFormat_AV_PIX_FMT_YUV420P;
            (*self.vid_enc_frame).width = self.enc_width;
            (*self.vid_enc_frame).height = self.enc_height;
            self.set_vid_enc_pts();

            let buf_ret = av_frame_get_buffer(self.vid_enc_frame, 0);
            if buf_ret < 0 {
                av_frame_unref(self.vid_enc_frame);
                return Err(ffmpeg_err(buf_ret));
            }

            // Build source plane/linesize arrays from the VideoFrame.
            let planes = frame.planes();
            let strides = frame.strides();
            let mut src_data = [ptr::null::<u8>(); 8];
            let mut src_linesize = [0i32; 8];
            for (i, (plane, &stride)) in planes.iter().zip(strides.iter()).enumerate().take(8) {
                src_data[i] = plane.as_ref().as_ptr();
                src_linesize[i] = stride as i32;
            }

            ff_sys::swscale::scale(
                self.sws_ctx,
                src_data.as_ptr(),
                src_linesize.as_ptr(),
                0,
                src_h,
                (*self.vid_enc_frame).data.as_mut_ptr().cast_const(),
                (*self.vid_enc_frame).linesize.as_mut_ptr(),
            )
            .map_err(|_| ffmpeg_err_msg("rtmp swscale conversion failed"))?;
        } else {
            // Same format and dimensions — copy planes directly.
            (*self.vid_enc_frame).format = AVPixelFormat_AV_PIX_FMT_YUV420P;
            (*self.vid_enc_frame).width = self.enc_width;
            (*self.vid_enc_frame).height = self.enc_height;
            self.set_vid_enc_pts();

            let buf_ret = av_frame_get_buffer(self.vid_enc_frame, 0);
            if buf_ret < 0 {
                av_frame_unref(self.vid_enc_frame);
                return Err(ffmpeg_err(buf_ret));
            }

            // Copy Y/U/V planes line-by-line.
            let planes = frame.planes();
            let strides = frame.strides();
            for (plane_idx, (src_plane, &src_stride)) in
                planes.iter().zip(strides.iter()).enumerate().take(3)
            {
                let dst_stride = (*self.vid_enc_frame).linesize[plane_idx] as usize;
                let dst_ptr = (*self.vid_enc_frame).data[plane_idx];
                if dst_ptr.is_null() {
                    continue;
                }
                let rows = if plane_idx == 0 {
                    self.enc_height as usize
                } else {
                    (self.enc_height as usize).div_ceil(2)
                };
                let copy_width = dst_stride.min(src_stride);
                let src_bytes: &[u8] = src_plane.as_ref();
                for row in 0..rows {
                    let src_off = row * src_stride;
                    let dst_off = row * dst_stride;
                    if src_off + copy_width > src_bytes.len() {
                        break;
                    }
                    // SAFETY: dst_ptr + dst_off is within the allocated frame buffer.
                    ptr::copy_nonoverlapping(
                        src_bytes.as_ptr().add(src_off),
                        dst_ptr.add(dst_off),
                        copy_width,
                    );
                }
            }
        }

        if ff_sys::avcodec::send_frame(self.vid_enc_ctx, self.vid_enc_frame).is_ok() {
            // SAFETY: vid_enc_ctx and out_ctx are valid; vid_out_stream_idx is valid.
            drain_encoder(
                self.vid_enc_ctx,
                self.out_ctx,
                self.vid_out_stream_idx,
                "rtmp",
                AVRational {
                    num: 1,
                    den: self.fps_int,
                },
            );
        }

        av_frame_unref(self.vid_enc_frame);
        self.video_frame_count += 1;
        Ok(())
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn push_audio_unsafe(&mut self, frame: &AudioFrame) {
        let in_fmt = sample_format_to_av(frame.format());
        let in_rate = frame.sample_rate() as i32;
        let in_channels = frame.channels() as i32;

        // (Re)create SwrContext when input parameters change.
        if self.last_swr_in_fmt != Some(in_fmt)
            || self.last_swr_in_rate != Some(in_rate)
            || self.last_swr_in_channels != Some(in_channels)
        {
            if !self.swr_ctx.is_null() {
                let mut swr_tmp = self.swr_ctx;
                ff_sys::swresample::free(&mut swr_tmp);
                self.swr_ctx = ptr::null_mut();
            }

            let in_layout = ff_sys::swresample::channel_layout::with_channels(in_channels);
            let enc_ch_layout = &(*self.aud_enc_ctx).ch_layout;

            if let Ok(ctx) = ff_sys::swresample::alloc_set_opts2(
                enc_ch_layout,
                ff_sys::swresample::sample_format::FLTP,
                self.aud_sample_rate,
                &in_layout,
                in_fmt,
                in_rate,
            ) {
                if ff_sys::swresample::init(ctx).is_ok() {
                    self.swr_ctx = ctx;
                    self.last_swr_in_fmt = Some(in_fmt);
                    self.last_swr_in_rate = Some(in_rate);
                    self.last_swr_in_channels = Some(in_channels);
                } else {
                    let mut swr_tmp = ctx;
                    ff_sys::swresample::free(&mut swr_tmp);
                    log::warn!("rtmp swr init failed, dropping audio frame");
                    return;
                }
            } else {
                log::warn!("rtmp swr alloc failed, dropping audio frame");
                return;
            }
        }

        // Prepare the encoder frame.
        (*self.aud_enc_frame).format = ff_sys::swresample::sample_format::FLTP;
        (*self.aud_enc_frame).sample_rate = self.aud_sample_rate;
        (*self.aud_enc_frame).nb_samples = self.aud_frame_size;
        let _ = ff_sys::swresample::channel_layout::copy(
            &mut (*self.aud_enc_frame).ch_layout,
            &(*self.aud_enc_ctx).ch_layout,
        );

        let buf_ret = av_frame_get_buffer(self.aud_enc_frame, 0);
        if buf_ret < 0 {
            av_frame_unref(self.aud_enc_frame);
            return;
        }

        // Build input plane pointers from the AudioFrame.
        let planes = frame.planes();
        let mut in_data = [ptr::null::<u8>(); 8];
        for (i, plane) in planes.iter().enumerate().take(8) {
            in_data[i] = plane.as_ptr();
        }

        let samples_out = ff_sys::swresample::convert(
            self.swr_ctx,
            (*self.aud_enc_frame).data.as_mut_ptr(),
            self.aud_frame_size,
            in_data.as_ptr(),
            frame.samples() as i32,
        );

        if let Ok(n) = samples_out
            && n > 0
        {
            (*self.aud_enc_frame).nb_samples = n;
            (*self.aud_enc_frame).pts = self.audio_pts;
            if ff_sys::avcodec::send_frame(self.aud_enc_ctx, self.aud_enc_frame).is_ok() {
                let aud_frame_period = AVRational {
                    num: (*self.aud_enc_ctx).frame_size,
                    den: (*self.aud_enc_ctx).sample_rate,
                };
                // SAFETY: aud_enc_ctx and out_ctx are valid; aud_out_stream_idx is valid.
                drain_encoder(
                    self.aud_enc_ctx,
                    self.out_ctx,
                    self.aud_out_stream_idx,
                    "rtmp",
                    aud_frame_period,
                );
            }
            self.audio_pts += i64::from(n);
        }

        av_frame_unref(self.aud_enc_frame);
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn flush_and_close_unsafe(&mut self) {
        // ── Flush video encoder ───────────────────────────────────────────────
        let _ = ff_sys::avcodec::send_frame(self.vid_enc_ctx, ptr::null());
        drain_encoder(
            self.vid_enc_ctx,
            self.out_ctx,
            self.vid_out_stream_idx,
            "rtmp",
            AVRational {
                num: 1,
                den: self.fps_int,
            },
        );

        // ── Flush audio encoder ───────────────────────────────────────────────
        if !self.aud_enc_ctx.is_null() && self.aud_out_stream_idx >= 0 {
            // Drain any remaining resampler buffered samples.
            if !self.swr_ctx.is_null() {
                (*self.aud_enc_frame).format = ff_sys::swresample::sample_format::FLTP;
                (*self.aud_enc_frame).sample_rate = self.aud_sample_rate;
                (*self.aud_enc_frame).nb_samples = self.aud_frame_size;
                let _ = ff_sys::swresample::channel_layout::copy(
                    &mut (*self.aud_enc_frame).ch_layout,
                    &(*self.aud_enc_ctx).ch_layout,
                );

                if av_frame_get_buffer(self.aud_enc_frame, 0) == 0 {
                    if let Ok(n) = ff_sys::swresample::convert(
                        self.swr_ctx,
                        (*self.aud_enc_frame).data.as_mut_ptr(),
                        self.aud_frame_size,
                        ptr::null(),
                        0,
                    ) && n > 0
                    {
                        (*self.aud_enc_frame).nb_samples = n;
                        (*self.aud_enc_frame).pts = self.audio_pts;
                        if ff_sys::avcodec::send_frame(self.aud_enc_ctx, self.aud_enc_frame).is_ok()
                        {
                            let aud_frame_period = AVRational {
                                num: (*self.aud_enc_ctx).frame_size,
                                den: (*self.aud_enc_ctx).sample_rate,
                            };
                            drain_encoder(
                                self.aud_enc_ctx,
                                self.out_ctx,
                                self.aud_out_stream_idx,
                                "rtmp",
                                aud_frame_period,
                            );
                        }
                    }
                    av_frame_unref(self.aud_enc_frame);
                }
            }

            // Flush the AAC encoder itself.
            let _ = ff_sys::avcodec::send_frame(self.aud_enc_ctx, ptr::null());
            let aud_frame_period = AVRational {
                num: (*self.aud_enc_ctx).frame_size,
                den: (*self.aud_enc_ctx).sample_rate,
            };
            drain_encoder(
                self.aud_enc_ctx,
                self.out_ctx,
                self.aud_out_stream_idx,
                "rtmp",
                aud_frame_period,
            );
        }

        // ── Write FLV trailer ─────────────────────────────────────────────────
        av_write_trailer(self.out_ctx);

        // Close the RTMP connection. Unlike HLS/DASH, pb was kept open throughout
        // the session and must be explicitly closed after av_write_trailer.
        if !(*self.out_ctx).pb.is_null() {
            ff_sys::avformat::close_output(&mut (*self.out_ctx).pb);
        }

        log::info!("rtmp output finished url={}", self.url);

        // Zero all pointers so Drop does not double-free.
        self.free_all();
    }

    // SAFETY: Callers must ensure self is not used again after this call.
    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn set_vid_enc_pts(&mut self) {
        (*self.vid_enc_frame).pts = av_rescale_q(
            self.video_frame_count as i64,
            AVRational {
                num: 1,
                den: self.fps_int,
            },
            (*self.vid_enc_ctx).time_base,
        );
    }

    /// Free all owned `FFmpeg` contexts and zero the pointers.
    ///
    /// # Safety
    ///
    /// Each pointer is checked for null before freeing. After this call all
    /// pointers are null, so a second call is a no-op.
    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn free_all(&mut self) {
        if !self.sws_ctx.is_null() {
            ff_sys::swscale::free_context(self.sws_ctx);
            self.sws_ctx = ptr::null_mut();
        }
        if !self.swr_ctx.is_null() {
            let mut swr_tmp = self.swr_ctx;
            ff_sys::swresample::free(&mut swr_tmp);
            self.swr_ctx = ptr::null_mut();
        }
        if !self.vid_enc_frame.is_null() {
            av_frame_free(&mut (self.vid_enc_frame as *mut _));
            self.vid_enc_frame = ptr::null_mut();
        }
        if !self.aud_enc_frame.is_null() {
            av_frame_free(&mut (self.aud_enc_frame as *mut _));
            self.aud_enc_frame = ptr::null_mut();
        }
        if !self.aud_enc_ctx.is_null() {
            ff_sys::avcodec::free_context(&mut self.aud_enc_ctx as *mut *mut _);
            self.aud_enc_ctx = ptr::null_mut();
        }
        if !self.vid_enc_ctx.is_null() {
            ff_sys::avcodec::free_context(&mut self.vid_enc_ctx as *mut *mut _);
            self.vid_enc_ctx = ptr::null_mut();
        }
        if !self.out_ctx.is_null() {
            // Close pb if still open (Drop path, flush_and_close was not called).
            if !(*self.out_ctx).pb.is_null() {
                ff_sys::avformat::close_output(&mut (*self.out_ctx).pb);
            }
            avformat_free_context(self.out_ctx);
            self.out_ctx = ptr::null_mut();
        }
    }
}

impl Drop for RtmpInner {
    fn drop(&mut self) {
        // SAFETY: free_all checks each pointer for null before freeing.
        // flush_and_close already zeroed all pointers on the success path,
        // so this is a safe no-op in the normal case.
        unsafe {
            self.free_all();
        }
    }
}
