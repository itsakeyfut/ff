//! Subtitle stream extraction.

use ff_format::stream::SubtitleStreamInfo;

use super::audio::extract_language;
use super::mapping::map_subtitle_codec;
use super::video::{extract_codec_name, extract_stream_duration};

/// Extracts all subtitle streams from an `AVFormatContext`.
///
/// This function iterates through all streams in the container and extracts
/// detailed information for each subtitle stream.
///
/// # Safety
///
/// The `ctx` pointer must be valid and `avformat_find_stream_info` must have been called.
pub(super) unsafe fn extract_subtitle_streams(
    ctx: *mut ff_sys::AVFormatContext,
) -> Vec<SubtitleStreamInfo> {
    // SAFETY: Caller guarantees ctx is valid and find_stream_info was called
    unsafe {
        let nb_streams = (*ctx).nb_streams;
        let streams_ptr = (*ctx).streams;

        if streams_ptr.is_null() || nb_streams == 0 {
            return Vec::new();
        }

        let mut subtitle_streams = Vec::new();

        for i in 0..nb_streams {
            // SAFETY: i < nb_streams, so this is within bounds
            let stream = *streams_ptr.add(i as usize);
            if stream.is_null() {
                continue;
            }

            let codecpar = (*stream).codecpar;
            if codecpar.is_null() {
                continue;
            }

            // Check if this is a subtitle stream
            if (*codecpar).codec_type != ff_sys::AVMediaType_AVMEDIA_TYPE_SUBTITLE {
                continue;
            }

            let stream_info = extract_single_subtitle_stream(stream, codecpar, i);
            subtitle_streams.push(stream_info);
        }

        subtitle_streams
    }
}

/// Extracts information from a single subtitle stream.
///
/// # Safety
///
/// Both `stream` and `codecpar` pointers must be valid.
unsafe fn extract_single_subtitle_stream(
    stream: *mut ff_sys::AVStream,
    codecpar: *mut ff_sys::AVCodecParameters,
    index: u32,
) -> SubtitleStreamInfo {
    // SAFETY: Caller guarantees pointers are valid
    unsafe {
        let codec_id = (*codecpar).codec_id;
        let codec = map_subtitle_codec(codec_id);
        let codec_name = extract_codec_name(codec_id);

        // disposition is a c_int bitmask; cast to u32 for bitwise AND with the u32 constant
        #[expect(
            clippy::cast_sign_loss,
            reason = "disposition is a non-negative bitmask"
        )]
        let forced = ((*stream).disposition as u32 & ff_sys::AV_DISPOSITION_FORCED) != 0;

        let duration = extract_stream_duration(stream);
        let language = extract_language(stream);
        let title = extract_stream_title(stream);

        let mut builder = SubtitleStreamInfo::builder()
            .index(index)
            .codec(codec)
            .codec_name(codec_name)
            .forced(forced);

        if let Some(d) = duration {
            builder = builder.duration(d);
        }
        if let Some(lang) = language {
            builder = builder.language(lang);
        }
        if let Some(t) = title {
            builder = builder.title(t);
        }

        builder.build()
    }
}

/// Extracts the "title" metadata tag from a stream's `AVDictionary`.
///
/// # Safety
///
/// The `stream` pointer must be valid.
pub(super) unsafe fn extract_stream_title(stream: *mut ff_sys::AVStream) -> Option<String> {
    use std::ffi::CStr;

    // SAFETY: Caller guarantees stream is valid
    unsafe {
        let metadata = (*stream).metadata;
        if metadata.is_null() {
            return None;
        }

        let key = c"title";
        let entry = ff_sys::av_dict_get(metadata, key.as_ptr(), std::ptr::null(), 0);

        if entry.is_null() {
            return None;
        }

        let value_ptr = (*entry).value;
        if value_ptr.is_null() {
            return None;
        }

        Some(CStr::from_ptr(value_ptr).to_string_lossy().into_owned())
    }
}
