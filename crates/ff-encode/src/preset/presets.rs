//! Predefined export presets.

use super::{AudioEncoderConfig, ExportPreset, VideoEncoderConfig};
use crate::{AudioCodec, BitrateMode, VideoCodec};

/// YouTube 1080p preset: H.264 CRF 18, 1920×1080, 30 fps, AAC 192 kbps.
pub(super) fn youtube_1080p() -> ExportPreset {
    ExportPreset {
        name: "youtube_1080p".to_string(),
        video: VideoEncoderConfig {
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            fps: 30.0,
            bitrate_mode: BitrateMode::Crf(18),
            pixel_format: None,
            codec_options: None,
        },
        audio: AudioEncoderConfig {
            codec: AudioCodec::Aac,
            sample_rate: 48000,
            channels: 2,
            bitrate: 192_000,
        },
    }
}

/// YouTube 4K preset: H.265 CRF 20, 3840×2160, 30 fps, AAC 256 kbps.
pub(super) fn youtube_4k() -> ExportPreset {
    ExportPreset {
        name: "youtube_4k".to_string(),
        video: VideoEncoderConfig {
            codec: VideoCodec::H265,
            width: 3840,
            height: 2160,
            fps: 30.0,
            bitrate_mode: BitrateMode::Crf(20),
            pixel_format: None,
            codec_options: None,
        },
        audio: AudioEncoderConfig {
            codec: AudioCodec::Aac,
            sample_rate: 48000,
            channels: 2,
            bitrate: 256_000,
        },
    }
}
