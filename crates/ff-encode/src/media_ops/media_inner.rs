//! Unsafe FFmpeg calls for audio stream replacement (remux operations).

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::path::Path;

use crate::error::EncodeError;

/// Replace the audio stream of `video_input` with the audio from `audio_input`,
/// writing the combined result to `output`.
///
/// # Safety
///
/// All FFmpeg pointer invariants are maintained internally.  The public
/// `AudioReplacement::run` wraps this function safely.
pub(crate) fn run_audio_replacement(
    video_input: &Path,
    audio_input: &Path,
    output: &Path,
) -> Result<(), EncodeError> {
    // SAFETY: All pointers are validated (null-checked) before use; resources
    //         are freed on every exit path.
    unsafe { run_audio_replacement_unsafe(video_input, audio_input, output) }
}

unsafe fn run_audio_replacement_unsafe(
    video_input: &Path,
    audio_input: &Path,
    output: &Path,
) -> Result<(), EncodeError> {
    // ── Step 1: open video input ──────────────────────────────────────────────
    // SAFETY: video_input is a caller-supplied path; open_input returns Err on failure.
    let vid_ctx =
        ff_sys::avformat::open_input(video_input).map_err(EncodeError::from_ffmpeg_error)?;

    // ── Step 2: find stream info for video input ──────────────────────────────
    // SAFETY: vid_ctx is non-null (open_input succeeded).
    if let Err(e) = ff_sys::avformat::find_stream_info(vid_ctx) {
        let mut p = vid_ctx;
        ff_sys::avformat::close_input(&mut p);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // ── Step 3: locate the first video stream ─────────────────────────────────
    // SAFETY: nb_streams is the valid count; streams is a valid array of that length.
    let nb_vid_streams = (*vid_ctx).nb_streams as usize;
    let mut video_stream_idx: Option<usize> = None;
    for i in 0..nb_vid_streams {
        let stream = *(*vid_ctx).streams.add(i);
        if (*(*stream).codecpar).codec_type == ff_sys::AVMediaType_AVMEDIA_TYPE_VIDEO {
            video_stream_idx = Some(i);
            break;
        }
    }
    let Some(video_stream_idx) = video_stream_idx else {
        let mut p = vid_ctx;
        ff_sys::avformat::close_input(&mut p);
        return Err(EncodeError::MediaOperationFailed {
            reason: format!(
                "no video stream found in video input path={}",
                video_input.display()
            ),
        });
    };

    // ── Step 4: open audio input ──────────────────────────────────────────────
    // SAFETY: audio_input is a caller-supplied path; open_input returns Err on failure.
    let aud_ctx = match ff_sys::avformat::open_input(audio_input) {
        Ok(ctx) => ctx,
        Err(e) => {
            let mut p = vid_ctx;
            ff_sys::avformat::close_input(&mut p);
            return Err(EncodeError::from_ffmpeg_error(e));
        }
    };

    // ── Step 5: find stream info for audio input ──────────────────────────────
    // SAFETY: aud_ctx is non-null (open_input succeeded).
    if let Err(e) = ff_sys::avformat::find_stream_info(aud_ctx) {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::from_ffmpeg_error(e));
    }

    // ── Step 6: locate the first audio stream ─────────────────────────────────
    // SAFETY: nb_streams is the valid count; streams is a valid array of that length.
    let nb_aud_streams = (*aud_ctx).nb_streams as usize;
    let mut audio_stream_idx: Option<usize> = None;
    for i in 0..nb_aud_streams {
        let stream = *(*aud_ctx).streams.add(i);
        if (*(*stream).codecpar).codec_type == ff_sys::AVMediaType_AVMEDIA_TYPE_AUDIO {
            audio_stream_idx = Some(i);
            break;
        }
    }
    let Some(audio_stream_idx) = audio_stream_idx else {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::MediaOperationFailed {
            reason: format!(
                "no audio stream found in audio input path={}",
                audio_input.display()
            ),
        });
    };

    // ── Step 7: allocate output context ──────────────────────────────────────
    let Some(output_str) = output.to_str() else {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "output path is not valid UTF-8".to_string(),
        });
    };
    let Ok(c_output) = std::ffi::CString::new(output_str) else {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "output path contains null bytes".to_string(),
        });
    };

    let mut out_ctx: *mut ff_sys::AVFormatContext = std::ptr::null_mut();
    // SAFETY: c_output is a valid null-terminated C string.
    let ret = ff_sys::avformat_alloc_output_context2(
        &mut out_ctx,
        std::ptr::null_mut(),
        std::ptr::null(),
        c_output.as_ptr(),
    );
    if ret < 0 || out_ctx.is_null() {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }

    // ── Step 8: copy video stream parameters to output ────────────────────────
    // SAFETY: video_stream_idx < nb_vid_streams; streams is a valid array.
    let vid_in_stream = *(*vid_ctx).streams.add(video_stream_idx);
    // SAFETY: out_ctx is non-null (avformat_alloc_output_context2 succeeded).
    let vid_out_stream = ff_sys::avformat_new_stream(out_ctx, std::ptr::null());
    if vid_out_stream.is_null() {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        ff_sys::avformat_free_context(out_ctx);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "avformat_new_stream failed for video".to_string(),
        });
    }
    // SAFETY: both codecpar pointers are non-null (created by FFmpeg).
    let ret =
        ff_sys::avcodec_parameters_copy((*vid_out_stream).codecpar, (*vid_in_stream).codecpar);
    if ret < 0 {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        ff_sys::avformat_free_context(out_ctx);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }
    // Clear codec_tag so the muxer assigns the correct value for the container.
    (*(*vid_out_stream).codecpar).codec_tag = 0;

    // ── Step 9: copy audio stream parameters to output ────────────────────────
    // SAFETY: audio_stream_idx < nb_aud_streams; streams is a valid array.
    let aud_in_stream = *(*aud_ctx).streams.add(audio_stream_idx);
    // SAFETY: out_ctx is non-null (avformat_alloc_output_context2 succeeded).
    let aud_out_stream = ff_sys::avformat_new_stream(out_ctx, std::ptr::null());
    if aud_out_stream.is_null() {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        ff_sys::avformat_free_context(out_ctx);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "avformat_new_stream failed for audio".to_string(),
        });
    }
    // SAFETY: both codecpar pointers are non-null (created by FFmpeg).
    let ret =
        ff_sys::avcodec_parameters_copy((*aud_out_stream).codecpar, (*aud_in_stream).codecpar);
    if ret < 0 {
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        ff_sys::avformat_free_context(out_ctx);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }
    // Clear codec_tag so the muxer assigns the correct value for the container.
    (*(*aud_out_stream).codecpar).codec_tag = 0;

    // ── Step 10: open output file ─────────────────────────────────────────────
    // SAFETY: output is a valid path; WRITE opens the file for writing.
    let pb = match ff_sys::avformat::open_output(output, ff_sys::avformat::avio_flags::WRITE) {
        Ok(pb) => pb,
        Err(e) => {
            let mut pv = vid_ctx;
            ff_sys::avformat::close_input(&mut pv);
            let mut pa = aud_ctx;
            ff_sys::avformat::close_input(&mut pa);
            ff_sys::avformat_free_context(out_ctx);
            return Err(EncodeError::from_ffmpeg_error(e));
        }
    };
    // SAFETY: out_ctx is non-null; pb is a valid AVIOContext.
    (*out_ctx).pb = pb;

    // ── Step 11: write header ─────────────────────────────────────────────────
    // SAFETY: out_ctx is fully configured with streams and pb set.
    let ret = ff_sys::avformat_write_header(out_ctx, std::ptr::null_mut());
    if ret < 0 {
        // SAFETY: (*out_ctx).pb was set above and is non-null.
        ff_sys::avformat::close_output(std::ptr::addr_of_mut!((*out_ctx).pb));
        ff_sys::avformat_free_context(out_ctx);
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::from_ffmpeg_error(ret));
    }

    // Read time bases after avformat_write_header — the muxer may adjust them.
    // SAFETY: stream pointers remain valid for the lifetime of their parent contexts.
    let vid_in_tb = (*vid_in_stream).time_base;
    let aud_in_tb = (*aud_in_stream).time_base;
    let vid_out_tb = (*vid_out_stream).time_base;
    let aud_out_tb = (*aud_out_stream).time_base;

    log::debug!(
        "audio replacement header written \
         video_stream_idx={video_stream_idx} audio_stream_idx={audio_stream_idx}"
    );

    // ── Step 12: allocate packet ──────────────────────────────────────────────
    // SAFETY: av_packet_alloc never returns null in practice (aborts on OOM).
    let pkt = ff_sys::av_packet_alloc();
    if pkt.is_null() {
        ff_sys::av_write_trailer(out_ctx);
        ff_sys::avformat::close_output(std::ptr::addr_of_mut!((*out_ctx).pb));
        ff_sys::avformat_free_context(out_ctx);
        let mut pv = vid_ctx;
        ff_sys::avformat::close_input(&mut pv);
        let mut pa = aud_ctx;
        ff_sys::avformat::close_input(&mut pa);
        return Err(EncodeError::Ffmpeg {
            code: 0,
            message: "av_packet_alloc failed".to_string(),
        });
    }

    // ── Step 13: interleaved packet copy loop ─────────────────────────────────
    // Alternate between video and audio inputs; use av_interleaved_write_frame
    // so the muxer buffers and flushes packets in the correct timestamp order.
    let mut loop_err: Option<EncodeError> = None;
    let mut vid_eof = false;
    let mut aud_eof = false;

    'copy: loop {
        // Read one packet from the video input, forwarding only the target stream.
        if !vid_eof {
            // SAFETY: vid_ctx and pkt are valid non-null pointers.
            match ff_sys::avformat::read_frame(vid_ctx, pkt) {
                Err(e) if e == ff_sys::error_codes::EOF => {
                    vid_eof = true;
                }
                Err(e) => {
                    loop_err = Some(EncodeError::from_ffmpeg_error(e));
                    break 'copy;
                }
                Ok(()) => {
                    if (*pkt).stream_index as usize == video_stream_idx {
                        // SAFETY: pkt, vid_in_tb, vid_out_tb are valid plain-data values.
                        ff_sys::av_packet_rescale_ts(pkt, vid_in_tb, vid_out_tb);
                        (*pkt).stream_index = 0;
                        // SAFETY: out_ctx and pkt are valid.
                        let ret = ff_sys::av_interleaved_write_frame(out_ctx, pkt);
                        // av_interleaved_write_frame takes the packet's buf reference;
                        // unref to clear any remaining fields.
                        ff_sys::av_packet_unref(pkt);
                        if ret < 0 {
                            loop_err = Some(EncodeError::from_ffmpeg_error(ret));
                            break 'copy;
                        }
                    } else {
                        ff_sys::av_packet_unref(pkt);
                    }
                }
            }
        }

        // Read one packet from the audio input, forwarding only the target stream.
        if !aud_eof {
            // SAFETY: aud_ctx and pkt are valid non-null pointers.
            match ff_sys::avformat::read_frame(aud_ctx, pkt) {
                Err(e) if e == ff_sys::error_codes::EOF => {
                    aud_eof = true;
                }
                Err(e) => {
                    loop_err = Some(EncodeError::from_ffmpeg_error(e));
                    break 'copy;
                }
                Ok(()) => {
                    if (*pkt).stream_index as usize == audio_stream_idx {
                        // SAFETY: pkt, aud_in_tb, aud_out_tb are valid plain-data values.
                        ff_sys::av_packet_rescale_ts(pkt, aud_in_tb, aud_out_tb);
                        (*pkt).stream_index = 1;
                        // SAFETY: out_ctx and pkt are valid.
                        let ret = ff_sys::av_interleaved_write_frame(out_ctx, pkt);
                        ff_sys::av_packet_unref(pkt);
                        if ret < 0 {
                            loop_err = Some(EncodeError::from_ffmpeg_error(ret));
                            break 'copy;
                        }
                    } else {
                        ff_sys::av_packet_unref(pkt);
                    }
                }
            }
        }

        if vid_eof && aud_eof {
            break 'copy;
        }
    }

    // SAFETY: pkt was allocated by av_packet_alloc above and is still valid.
    let mut pkt_ptr = pkt;
    ff_sys::av_packet_free(&mut pkt_ptr);

    // ── Step 14: write trailer ────────────────────────────────────────────────
    // SAFETY: out_ctx is valid; write_header was called successfully.
    ff_sys::av_write_trailer(out_ctx);

    // ── Step 15: cleanup ──────────────────────────────────────────────────────
    // SAFETY: (*out_ctx).pb is non-null (opened above; still set after write_header).
    ff_sys::avformat::close_output(std::ptr::addr_of_mut!((*out_ctx).pb));
    // SAFETY: out_ctx is non-null and was allocated by avformat_alloc_output_context2.
    ff_sys::avformat_free_context(out_ctx);
    // SAFETY: vid_ctx and aud_ctx are non-null (open_input succeeded).
    let mut pv = vid_ctx;
    ff_sys::avformat::close_input(&mut pv);
    let mut pa = aud_ctx;
    ff_sys::avformat::close_input(&mut pa);

    log::info!("audio replaced output={}", output.display());

    match loop_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
