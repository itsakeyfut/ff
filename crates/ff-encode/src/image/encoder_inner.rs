//! Internal image encoder implementation.
//!
//! All `unsafe` FFmpeg calls are isolated here. The public API in `builder.rs`
//! is fully safe.

// Rust 2024: Allow unsafe operations in unsafe functions for FFmpeg C API
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::cast_possible_wrap)]

use std::ffi::CString;
use std::path::Path;
use std::ptr;

use ff_format::{PixelFormat, VideoFrame};
use ff_sys::{
    AVCodecID, AVCodecID_AV_CODEC_ID_BMP, AVCodecID_AV_CODEC_ID_MJPEG, AVCodecID_AV_CODEC_ID_PNG,
    AVCodecID_AV_CODEC_ID_TIFF, AVCodecID_AV_CODEC_ID_WEBP, AVFormatContext, AVPixelFormat,
    AVPixelFormat_AV_PIX_FMT_BGR24, AVPixelFormat_AV_PIX_FMT_RGB24,
    AVPixelFormat_AV_PIX_FMT_YUV420P, AVPixelFormat_AV_PIX_FMT_YUVJ420P, AVRational,
    av_frame_alloc, av_frame_free, av_interleaved_write_frame, av_packet_alloc, av_packet_free,
    av_packet_unref, av_write_trailer, avcodec, avformat, avformat_alloc_output_context2,
    avformat_free_context, avformat_new_stream, avformat_write_header, swscale,
};

use crate::EncodeError;

/// Maximum number of planes in AVFrame data/linesize arrays.
const MAX_PLANES: usize = 8;

/// Options forwarded from the builder to the encoder.
pub(super) struct ImageEncodeOptions {
    /// Override output width (pixels). `None` → use source frame width.
    pub(super) width: Option<u32>,
    /// Override output height (pixels). `None` → use source frame height.
    pub(super) height: Option<u32>,
    /// Quality 0–100 (100 = best). `None` → codec default.
    pub(super) quality: Option<u32>,
    /// Output pixel format override. `None` → codec-native default.
    pub(super) pixel_format: Option<PixelFormat>,
}

/// Return the `AVCodecID` for the given file extension.
///
/// This is `pub(super)` so `builder.rs` can call it for early validation.
pub(super) fn codec_from_extension(path: &Path) -> Result<AVCodecID, EncodeError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => Ok(AVCodecID_AV_CODEC_ID_MJPEG),
        "png" => Ok(AVCodecID_AV_CODEC_ID_PNG),
        "bmp" => Ok(AVCodecID_AV_CODEC_ID_BMP),
        "tif" | "tiff" => Ok(AVCodecID_AV_CODEC_ID_TIFF),
        "webp" => Ok(AVCodecID_AV_CODEC_ID_WEBP),
        "" => Err(EncodeError::InvalidConfig {
            reason: "no file extension".to_string(),
        }),
        e => Err(EncodeError::UnsupportedCodec {
            codec: e.to_string(),
        }),
    }
}

/// Return the preferred `AVPixelFormat` for the given codec.
fn preferred_pix_fmt(codec_id: AVCodecID) -> AVPixelFormat {
    match codec_id {
        x if x == AVCodecID_AV_CODEC_ID_MJPEG => AVPixelFormat_AV_PIX_FMT_YUVJ420P,
        x if x == AVCodecID_AV_CODEC_ID_PNG => AVPixelFormat_AV_PIX_FMT_RGB24,
        x if x == AVCodecID_AV_CODEC_ID_BMP => AVPixelFormat_AV_PIX_FMT_BGR24,
        x if x == AVCodecID_AV_CODEC_ID_TIFF => AVPixelFormat_AV_PIX_FMT_RGB24,
        x if x == AVCodecID_AV_CODEC_ID_WEBP => AVPixelFormat_AV_PIX_FMT_YUV420P,
        _ => AVPixelFormat_AV_PIX_FMT_RGB24,
    }
}

/// Map a `PixelFormat` enum value to the corresponding `AVPixelFormat` constant.
fn pixel_format_to_av(fmt: PixelFormat) -> AVPixelFormat {
    match fmt {
        PixelFormat::Yuv420p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P,
        PixelFormat::Yuv422p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P,
        PixelFormat::Yuv444p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P,
        PixelFormat::Rgb24 => ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24,
        PixelFormat::Bgr24 => ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24,
        PixelFormat::Rgba => ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA,
        PixelFormat::Bgra => ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA,
        PixelFormat::Gray8 => ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8,
        PixelFormat::Nv12 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV12,
        PixelFormat::Nv21 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV21,
        PixelFormat::Yuv420p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE,
        PixelFormat::P010le => ff_sys::AVPixelFormat_AV_PIX_FMT_P010LE,
        _ => ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24,
    }
}

/// Apply a quality value (0–100, 100 = best) to the codec context.
///
/// Must be called after the codec context fields are set but before
/// `avcodec_open2`.
///
/// # Safety
///
/// `codec_ctx` must be a valid, non-null pointer to an allocated
/// `AVCodecContext` whose `priv_data` is valid (guaranteed after
/// `avcodec_alloc_context3`).
unsafe fn apply_quality(codec_ctx: *mut ff_sys::AVCodecContext, codec_id: AVCodecID, quality: u32) {
    let q = quality.min(100);

    if codec_id == AVCodecID_AV_CODEC_ID_MJPEG {
        // Map 0–100 (100 = best) → MJPEG qscale 1–31 (1 = best, 31 = worst).
        let qscale = (1 + (100 - q) * 30 / 100) as i32;
        (*codec_ctx).qmin = qscale;
        (*codec_ctx).qmax = qscale;
        log::info!("MJPEG quality applied quality={q} qscale={qscale}");
    } else if codec_id == AVCodecID_AV_CODEC_ID_PNG {
        // Map 0–100 → compression_level 0–9 (9 = maximum compression).
        let level = q * 9 / 100;
        if (*codec_ctx).priv_data.is_null() {
            log::warn!("PNG compression_level: priv_data is null, skipping quality={q}");
            return;
        }
        let Ok(key) = CString::new("compression_level") else {
            return;
        };
        let Ok(val) = CString::new(level.to_string()) else {
            return;
        };
        // SAFETY: priv_data is non-null; key/val are valid NUL-terminated strings.
        let ret = ff_sys::av_opt_set((*codec_ctx).priv_data, key.as_ptr(), val.as_ptr(), 0);
        if ret < 0 {
            log::warn!(
                "av_opt_set compression_level failed, ignoring \
                 quality={q} error={}",
                ff_sys::av_error_string(ret)
            );
        } else {
            log::info!("PNG compression_level applied quality={q} level={level}");
        }
    } else if codec_id == AVCodecID_AV_CODEC_ID_WEBP {
        // Direct 0–100 mapping for WebP quality.
        if (*codec_ctx).priv_data.is_null() {
            log::warn!("WebP quality: priv_data is null, skipping quality={q}");
            return;
        }
        let Ok(key) = CString::new("quality") else {
            return;
        };
        let Ok(val) = CString::new(q.to_string()) else {
            return;
        };
        // SAFETY: priv_data is non-null; key/val are valid NUL-terminated strings.
        let ret = ff_sys::av_opt_set((*codec_ctx).priv_data, key.as_ptr(), val.as_ptr(), 0);
        if ret < 0 {
            log::warn!(
                "av_opt_set quality failed for WebP, ignoring \
                 quality={q} error={}",
                ff_sys::av_error_string(ret)
            );
        } else {
            log::info!("WebP quality applied quality={q}");
        }
    } else {
        log::warn!(
            "quality option has no effect for this codec \
             codec_id={codec_id} quality={q}"
        );
    }
}

/// Encode a single `VideoFrame` and write it to `path`.
///
/// # Safety
///
/// Caller must ensure `path` is a valid file path. All FFmpeg resources
/// allocated inside this function are freed before returning.
pub(super) unsafe fn encode_image(
    path: &Path,
    frame: &VideoFrame,
    opts: &ImageEncodeOptions,
) -> Result<(), EncodeError> {
    ff_sys::ensure_initialized();

    let codec_id = codec_from_extension(path)?;

    // Resolve output dimensions — fall back to source frame dimensions if unset.
    let dst_width = opts.width.unwrap_or_else(|| frame.width());
    let dst_height = opts.height.unwrap_or_else(|| frame.height());

    // Resolve output pixel format — fall back to codec-native default if unset.
    let pix_fmt = opts
        .pixel_format
        .map_or_else(|| preferred_pix_fmt(codec_id), pixel_format_to_av);

    // ── Step 1: Build the output format context ───────────────────────────────
    let c_path = CString::new(path.to_str().ok_or_else(|| EncodeError::CannotCreateFile {
        path: path.to_path_buf(),
    })?)
    .map_err(|_| EncodeError::CannotCreateFile {
        path: path.to_path_buf(),
    })?;

    let mut format_ctx: *mut AVFormatContext = ptr::null_mut();
    let ret = avformat_alloc_output_context2(
        &mut format_ctx,
        ptr::null_mut(),
        ptr::null(),
        c_path.as_ptr(),
    );
    if ret < 0 || format_ctx.is_null() {
        return Err(EncodeError::Ffmpeg(format!(
            "Cannot create output context: {}",
            ff_sys::av_error_string(ret)
        )));
    }

    // ── Step 2: Add one video stream ─────────────────────────────────────────
    let stream = avformat_new_stream(format_ctx, ptr::null());
    if stream.is_null() {
        avformat_free_context(format_ctx);
        return Err(EncodeError::Ffmpeg(
            "Cannot create output stream".to_string(),
        ));
    }

    // ── Step 3: Find the encoder ──────────────────────────────────────────────
    let codec = avcodec::find_encoder(codec_id).ok_or_else(|| {
        avformat_free_context(format_ctx);
        EncodeError::UnsupportedCodec {
            codec: format!("codec_id={codec_id}"),
        }
    })?;

    // ── Step 4: Allocate codec context ────────────────────────────────────────
    let codec_ctx = avcodec::alloc_context3(codec).map_err(|e| {
        avformat_free_context(format_ctx);
        EncodeError::from_ffmpeg_error(e)
    })?;

    // ── Step 5: Set codec context fields ─────────────────────────────────────
    (*codec_ctx).width = dst_width as i32;
    (*codec_ctx).height = dst_height as i32;
    (*codec_ctx).time_base = AVRational { num: 1, den: 1 };
    (*codec_ctx).pix_fmt = pix_fmt;

    // ── Step 5b: Apply quality (before open2) ────────────────────────────────
    if let Some(q) = opts.quality {
        // SAFETY: codec_ctx is non-null and was just allocated with alloc_context3.
        apply_quality(codec_ctx, codec_id, q);
    }

    // ── Step 6: Open the codec ────────────────────────────────────────────────
    if let Err(e) = avcodec::open2(codec_ctx, codec, ptr::null_mut()) {
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // ── Step 7: Copy codec parameters to stream ───────────────────────────────
    // SAFETY: stream and codec_ctx are non-null and valid at this point.
    let par = (*stream).codecpar;
    (*par).codec_id = codec_id;
    (*par).codec_type = ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO;
    (*par).width = (*codec_ctx).width;
    (*par).height = (*codec_ctx).height;
    (*par).format = pix_fmt;

    // ── Step 8: Open output file ──────────────────────────────────────────────
    let io_ctx = avformat::open_output(path, avformat::avio_flags::WRITE).map_err(|e| {
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        EncodeError::from_ffmpeg_error(e)
    })?;
    (*format_ctx).pb = io_ctx;

    // ── Step 9: Write the file header ─────────────────────────────────────────
    let ret = avformat_write_header(format_ctx, ptr::null_mut());
    if ret < 0 {
        avformat::close_output(&mut ((*format_ctx).pb as *mut _));
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }

    // ── Step 10: Allocate destination AVFrame ─────────────────────────────────
    let dst_frame = av_frame_alloc();
    if dst_frame.is_null() {
        avformat::close_output(&mut ((*format_ctx).pb as *mut _));
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::Ffmpeg(
            "Cannot allocate destination frame".to_string(),
        ));
    }

    (*dst_frame).format = pix_fmt;
    (*dst_frame).width = dst_width as i32;
    (*dst_frame).height = dst_height as i32;

    let ret = ff_sys::av_frame_get_buffer(dst_frame, 0);
    if ret < 0 {
        av_frame_free(&mut (dst_frame as *mut _));
        avformat::close_output(&mut ((*format_ctx).pb as *mut _));
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }

    // ── Step 11: Fill dst_frame (convert / resize if needed) ─────────────────
    let src_fmt = pixel_format_to_av(frame.format());
    let needs_conversion =
        src_fmt != pix_fmt || frame.width() != dst_width || frame.height() != dst_height;

    if needs_conversion {
        // Use swscale for pixel format conversion and/or resize.
        let sws_ctx = swscale::get_context(
            frame.width() as i32,
            frame.height() as i32,
            src_fmt,
            dst_width as i32,
            dst_height as i32,
            pix_fmt,
            swscale::scale_flags::BILINEAR,
        )
        .map_err(|e| {
            av_frame_free(&mut (dst_frame as *mut _));
            avformat::close_output(&mut ((*format_ctx).pb as *mut _));
            avcodec::free_context(&mut (codec_ctx as *mut _));
            avformat_free_context(format_ctx);
            EncodeError::from_ffmpeg_error(e)
        })?;

        let mut src_data: [*const u8; MAX_PLANES] = [ptr::null(); MAX_PLANES];
        let mut src_linesize: [i32; MAX_PLANES] = [0; MAX_PLANES];

        for (i, plane) in frame.planes().iter().enumerate() {
            if i < MAX_PLANES {
                src_data[i] = plane.data().as_ptr();
                src_linesize[i] = frame.strides()[i] as i32;
            }
        }

        let scale_result = swscale::scale(
            sws_ctx,
            src_data.as_ptr(),
            src_linesize.as_ptr(),
            0,
            frame.height() as i32,
            (*dst_frame).data.as_mut_ptr().cast_const(),
            (*dst_frame).linesize.as_mut_ptr(),
        );
        swscale::free_context(sws_ctx);

        if let Err(e) = scale_result {
            av_frame_free(&mut (dst_frame as *mut _));
            avformat::close_output(&mut ((*format_ctx).pb as *mut _));
            avcodec::free_context(&mut (codec_ctx as *mut _));
            avformat_free_context(format_ctx);
            return Err(EncodeError::from_ffmpeg_error(e));
        }
    } else {
        // Direct plane copy — same format and dimensions.
        for (i, plane) in frame.planes().iter().enumerate() {
            if i >= MAX_PLANES || (*dst_frame).data[i].is_null() {
                break;
            }
            let src_stride = frame.strides()[i];
            let dst_stride = (*dst_frame).linesize[i] as usize;
            let plane_data = plane.data();

            if src_stride == dst_stride {
                std::ptr::copy_nonoverlapping(
                    plane_data.as_ptr(),
                    (*dst_frame).data[i],
                    plane_data.len(),
                );
            } else {
                // Stride mismatch: copy row by row.
                let row_bytes = src_stride.min(dst_stride);
                let num_rows = plane_data.len() / src_stride;
                for row in 0..num_rows {
                    std::ptr::copy_nonoverlapping(
                        plane_data[row * src_stride..].as_ptr(),
                        (*dst_frame).data[i].add(row * dst_stride),
                        row_bytes,
                    );
                }
            }
        }
    }

    // ── Step 12: Set PTS ──────────────────────────────────────────────────────
    (*dst_frame).pts = 0;

    // ── Step 13: Send frame to encoder ────────────────────────────────────────
    let packet = av_packet_alloc();
    if packet.is_null() {
        av_frame_free(&mut (dst_frame as *mut _));
        avformat::close_output(&mut ((*format_ctx).pb as *mut _));
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::Ffmpeg("Cannot allocate packet".to_string()));
    }

    let send_result = avcodec::send_frame(codec_ctx, dst_frame);
    av_frame_free(&mut (dst_frame as *mut _));

    if let Err(e) = send_result {
        av_packet_free(&mut (packet as *mut _));
        avformat::close_output(&mut ((*format_ctx).pb as *mut _));
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // ── Step 14: Receive encoded packets ──────────────────────────────────────
    loop {
        match avcodec::receive_packet(codec_ctx, packet) {
            Ok(()) => {
                (*packet).stream_index = 0;
                let ret = av_interleaved_write_frame(format_ctx, packet);
                av_packet_unref(packet);
                if ret < 0 {
                    av_packet_free(&mut (packet as *mut _));
                    avformat::close_output(&mut ((*format_ctx).pb as *mut _));
                    avcodec::free_context(&mut (codec_ctx as *mut _));
                    avformat_free_context(format_ctx);
                    return Err(EncodeError::from_ffmpeg_error(ret));
                }
            }
            Err(e) if e == ff_sys::error_codes::EAGAIN || e == ff_sys::error_codes::EOF => {
                break;
            }
            Err(e) => {
                av_packet_free(&mut (packet as *mut _));
                avformat::close_output(&mut ((*format_ctx).pb as *mut _));
                avcodec::free_context(&mut (codec_ctx as *mut _));
                avformat_free_context(format_ctx);
                return Err(EncodeError::from_ffmpeg_error(e));
            }
        }
    }

    // ── Step 15: Flush encoder ────────────────────────────────────────────────
    if let Err(e) = avcodec::send_frame(codec_ctx, ptr::null()) {
        av_packet_free(&mut (packet as *mut _));
        avformat::close_output(&mut ((*format_ctx).pb as *mut _));
        avcodec::free_context(&mut (codec_ctx as *mut _));
        avformat_free_context(format_ctx);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // ── Step 16: Drain remaining packets after flush ──────────────────────────
    loop {
        match avcodec::receive_packet(codec_ctx, packet) {
            Ok(()) => {
                (*packet).stream_index = 0;
                let ret = av_interleaved_write_frame(format_ctx, packet);
                av_packet_unref(packet);
                if ret < 0 {
                    av_packet_free(&mut (packet as *mut _));
                    avformat::close_output(&mut ((*format_ctx).pb as *mut _));
                    avcodec::free_context(&mut (codec_ctx as *mut _));
                    avformat_free_context(format_ctx);
                    return Err(EncodeError::from_ffmpeg_error(ret));
                }
            }
            Err(e) if e == ff_sys::error_codes::EOF || e == ff_sys::error_codes::EAGAIN => {
                break;
            }
            Err(e) => {
                av_packet_free(&mut (packet as *mut _));
                avformat::close_output(&mut ((*format_ctx).pb as *mut _));
                avcodec::free_context(&mut (codec_ctx as *mut _));
                avformat_free_context(format_ctx);
                return Err(EncodeError::from_ffmpeg_error(e));
            }
        }
    }

    // ── Step 17: Write trailer ────────────────────────────────────────────────
    av_write_trailer(format_ctx);

    // ── Step 18: Cleanup ──────────────────────────────────────────────────────
    av_packet_free(&mut (packet as *mut _));
    avformat::close_output(&mut ((*format_ctx).pb as *mut _));
    avcodec::free_context(&mut (codec_ctx as *mut _));
    avformat_free_context(format_ctx);

    log::info!(
        "Image encoded successfully path={} src={}x{} dst={}x{}",
        path.display(),
        frame.width(),
        frame.height(),
        dst_width,
        dst_height,
    );

    Ok(())
}
