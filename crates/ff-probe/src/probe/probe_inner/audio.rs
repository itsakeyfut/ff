//! Audio stream extraction.

use std::ffi::CStr;

use ff_format::channel::ChannelLayout;
use ff_format::stream::AudioStreamInfo;

use super::mapping::{map_audio_codec, map_sample_format};
use super::video::{extract_codec_name, extract_stream_bitrate, extract_stream_duration};

/// Extracts all audio streams from an `AVFormatContext`.
///
/// This function iterates through all streams in the container and extracts
/// detailed information for each audio stream.
///
/// # Safety
///
/// The `ctx` pointer must be valid and `avformat_find_stream_info` must have been called.
pub(super) unsafe fn extract_audio_streams(
    ctx: *mut ff_sys::AVFormatContext,
) -> Vec<AudioStreamInfo> {
    // SAFETY: Caller guarantees ctx is valid and find_stream_info was called
    unsafe {
        let nb_streams = (*ctx).nb_streams;
        let streams_ptr = (*ctx).streams;

        if streams_ptr.is_null() || nb_streams == 0 {
            return Vec::new();
        }

        let mut audio_streams = Vec::new();

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

            // Check if this is an audio stream
            if (*codecpar).codec_type != ff_sys::AVMediaType_AVMEDIA_TYPE_AUDIO {
                continue;
            }

            // Extract audio stream info
            let stream_info = extract_single_audio_stream(stream, codecpar, i);
            audio_streams.push(stream_info);
        }

        audio_streams
    }
}

/// Extracts information from a single audio stream.
///
/// # Safety
///
/// Both `stream` and `codecpar` pointers must be valid.
unsafe fn extract_single_audio_stream(
    stream: *mut ff_sys::AVStream,
    codecpar: *mut ff_sys::AVCodecParameters,
    index: u32,
) -> AudioStreamInfo {
    // SAFETY: Caller guarantees pointers are valid
    unsafe {
        // Extract codec info
        let codec_id = (*codecpar).codec_id;
        let codec = map_audio_codec(codec_id);
        let codec_name = extract_codec_name(codec_id);

        // Extract audio parameters
        #[expect(clippy::cast_sign_loss, reason = "sample_rate is always positive")]
        let sample_rate = (*codecpar).sample_rate as u32;

        // FFmpeg 5.1+ uses ch_layout, older versions use channels
        let channels = extract_channel_count(codecpar);

        // Extract channel layout
        let channel_layout = extract_channel_layout(codecpar, channels);

        // Extract sample format
        let sample_format = map_sample_format((*codecpar).format);

        // Extract bitrate
        let bitrate = extract_stream_bitrate(codecpar);

        // Extract duration if available
        let duration = extract_stream_duration(stream);

        // Extract language from stream metadata
        let language = extract_language(stream);

        // Build the AudioStreamInfo
        let mut builder = AudioStreamInfo::builder()
            .index(index)
            .codec(codec)
            .codec_name(codec_name)
            .sample_rate(sample_rate)
            .channels(channels)
            .channel_layout(channel_layout)
            .sample_format(sample_format);

        if let Some(d) = duration {
            builder = builder.duration(d);
        }

        if let Some(b) = bitrate {
            builder = builder.bitrate(b);
        }

        if let Some(lang) = language {
            builder = builder.language(lang);
        }

        builder.build()
    }
}

/// Extracts the channel count from `AVCodecParameters`.
///
/// `FFmpeg` 5.1+ uses `ch_layout.nb_channels`, older versions used `channels` directly.
///
/// Returns the actual channel count from `FFmpeg`. If the channel count is 0 (which
/// indicates uninitialized or unknown), returns 1 (mono) as a safe minimum.
///
/// # Safety
///
/// The `codecpar` pointer must be valid.
unsafe fn extract_channel_count(codecpar: *mut ff_sys::AVCodecParameters) -> u32 {
    // SAFETY: Caller guarantees codecpar is valid
    // FFmpeg 5.1+ uses ch_layout structure
    #[expect(clippy::cast_sign_loss, reason = "channel count is always positive")]
    let channels = unsafe { (*codecpar).ch_layout.nb_channels as u32 };

    // If channel count is 0 (uninitialized/unknown), use 1 (mono) as safe minimum
    if channels > 0 {
        channels
    } else {
        log::warn!(
            "channel_count is 0 (uninitialized), falling back to mono \
             fallback=1"
        );
        1
    }
}

/// Extracts the channel layout from `AVCodecParameters`.
///
/// # Safety
///
/// The `codecpar` pointer must be valid.
unsafe fn extract_channel_layout(
    codecpar: *mut ff_sys::AVCodecParameters,
    channels: u32,
) -> ChannelLayout {
    // SAFETY: Caller guarantees codecpar is valid
    // FFmpeg 5.1+ uses ch_layout structure with channel masks
    let ch_layout = unsafe { &(*codecpar).ch_layout };

    // Check if we have a specific channel layout mask
    // AV_CHANNEL_ORDER_NATIVE means we have a valid channel mask
    if ch_layout.order == ff_sys::AVChannelOrder_AV_CHANNEL_ORDER_NATIVE {
        // Map common FFmpeg channel masks to our ChannelLayout
        // These are AVChannelLayout masks for standard configurations
        // SAFETY: When order is AV_CHANNEL_ORDER_NATIVE, the mask field is valid
        let mask = unsafe { ch_layout.u.mask };
        match mask {
            // AV_CH_LAYOUT_MONO = 0x4 (front center)
            0x4 => ChannelLayout::Mono,
            // AV_CH_LAYOUT_STEREO = 0x3 (front left + front right)
            0x3 => ChannelLayout::Stereo,
            // AV_CH_LAYOUT_2_1 = 0x103 (stereo + LFE)
            0x103 => ChannelLayout::Stereo2_1,
            // AV_CH_LAYOUT_SURROUND = 0x7 (FL + FR + FC)
            0x7 => ChannelLayout::Surround3_0,
            // AV_CH_LAYOUT_QUAD = 0x33 (FL + FR + BL + BR)
            0x33 => ChannelLayout::Quad,
            // AV_CH_LAYOUT_5POINT0 = 0x37 (FL + FR + FC + BL + BR)
            0x37 => ChannelLayout::Surround5_0,
            // AV_CH_LAYOUT_5POINT1 = 0x3F (FL + FR + FC + LFE + BL + BR)
            0x3F => ChannelLayout::Surround5_1,
            // AV_CH_LAYOUT_6POINT1 = 0x13F (FL + FR + FC + LFE + BC + SL + SR)
            0x13F => ChannelLayout::Surround6_1,
            // AV_CH_LAYOUT_7POINT1 = 0x63F (FL + FR + FC + LFE + BL + BR + SL + SR)
            0x63F => ChannelLayout::Surround7_1,
            _ => {
                log::warn!(
                    "channel_layout mask has no mapping, deriving from channel count \
                     mask={mask} channels={channels}"
                );
                ChannelLayout::from_channels(channels)
            }
        }
    } else {
        log::warn!(
            "channel_layout order is not NATIVE, deriving from channel count \
             order={order} channels={channels}",
            order = ch_layout.order
        );
        ChannelLayout::from_channels(channels)
    }
}

/// Extracts the language tag from stream metadata.
///
/// # Safety
///
/// The `stream` pointer must be valid.
pub(super) unsafe fn extract_language(stream: *mut ff_sys::AVStream) -> Option<String> {
    // SAFETY: Caller guarantees stream is valid
    unsafe {
        let metadata = (*stream).metadata;
        if metadata.is_null() {
            return None;
        }

        // Look for "language" tag in the stream metadata
        let key = c"language";
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
