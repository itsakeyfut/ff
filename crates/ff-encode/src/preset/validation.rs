//! Preset constraint validation.
//!
//! Checks platform-specific rules before encoding begins. Validation is
//! fail-fast: the first violated constraint returns an immediate `Err`.

use super::ExportPreset;
use crate::EncodeError;

/// Validates a preset against platform-specific constraints.
///
/// Returns [`EncodeError::PresetConstraintViolation`] on the first violated rule.
pub(super) fn validate_preset(preset: &ExportPreset) -> Result<(), EncodeError> {
    match preset.name.as_str() {
        "youtube_1080p" | "youtube_4k" => validate_youtube(preset)?,
        "instagram_square" => validate_instagram_square(preset)?,
        "instagram_reels" => validate_instagram_reels(preset)?,
        _ => {}
    }
    Ok(())
}

fn validate_youtube(preset: &ExportPreset) -> Result<(), EncodeError> {
    if let Some(ref v) = preset.video
        && let Some(fps) = v.fps
        && fps > 60.0
    {
        return Err(EncodeError::PresetConstraintViolation {
            preset: preset.name.clone(),
            reason: "fps exceeds YouTube limit of 60".to_string(),
        });
    }
    Ok(())
}

fn validate_instagram_square(preset: &ExportPreset) -> Result<(), EncodeError> {
    if let Some(ref v) = preset.video
        && let (Some(w), Some(h)) = (v.width, v.height)
        && w != h
    {
        return Err(EncodeError::PresetConstraintViolation {
            preset: preset.name.clone(),
            reason: format!("instagram_square requires 1:1 aspect ratio, got {w}x{h}"),
        });
    }
    Ok(())
}

fn validate_instagram_reels(preset: &ExportPreset) -> Result<(), EncodeError> {
    if let Some(ref v) = preset.video {
        // 9:16 portrait: width * 16 == height * 9
        if let (Some(w), Some(h)) = (v.width, v.height)
            && w * 16 != h * 9
        {
            return Err(EncodeError::PresetConstraintViolation {
                preset: preset.name.clone(),
                reason: format!("instagram_reels requires 9:16 aspect ratio, got {w}x{h}"),
            });
        }
        if let Some(fps) = v.fps
            && fps > 60.0
        {
            return Err(EncodeError::PresetConstraintViolation {
                preset: preset.name.clone(),
                reason: "fps exceeds Instagram Reels limit of 60".to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{AudioEncoderConfig, ExportPreset, VideoEncoderConfig};
    use crate::{AudioCodec, BitrateMode, VideoCodec};

    fn youtube_with_fps(fps: f64) -> ExportPreset {
        ExportPreset {
            name: "youtube_1080p".to_string(),
            video: Some(VideoEncoderConfig {
                codec: VideoCodec::H264,
                width: Some(1920),
                height: Some(1080),
                fps: Some(fps),
                bitrate_mode: BitrateMode::Crf(18),
                pixel_format: None,
                codec_options: None,
            }),
            audio: AudioEncoderConfig {
                codec: AudioCodec::Aac,
                sample_rate: 48000,
                channels: 2,
                bitrate: 192_000,
            },
        }
    }

    fn instagram_square_with_resolution(w: u32, h: u32) -> ExportPreset {
        ExportPreset {
            name: "instagram_square".to_string(),
            video: Some(VideoEncoderConfig {
                codec: VideoCodec::H264,
                width: Some(w),
                height: Some(h),
                fps: Some(30.0),
                bitrate_mode: BitrateMode::Crf(23),
                pixel_format: None,
                codec_options: None,
            }),
            audio: AudioEncoderConfig {
                codec: AudioCodec::Aac,
                sample_rate: 48000,
                channels: 2,
                bitrate: 128_000,
            },
        }
    }

    #[test]
    fn youtube_preset_high_fps_should_fail_validation() {
        let result = validate_preset(&youtube_with_fps(120.0));
        assert!(
            matches!(result, Err(EncodeError::PresetConstraintViolation { .. })),
            "expected PresetConstraintViolation for fps=120, got {result:?}"
        );
    }

    #[test]
    fn youtube_preset_60_fps_should_pass_validation() {
        assert!(validate_preset(&youtube_with_fps(60.0)).is_ok());
    }

    #[test]
    fn valid_preset_config_should_pass_validation() {
        assert!(validate_preset(&ExportPreset::youtube_1080p()).is_ok());
    }

    #[test]
    fn instagram_square_wrong_aspect_should_fail_validation() {
        // 1920×1080 is 16:9, not 1:1
        let result = validate_preset(&instagram_square_with_resolution(1920, 1080));
        assert!(
            matches!(result, Err(EncodeError::PresetConstraintViolation { .. })),
            "expected PresetConstraintViolation for 16:9 on instagram_square, got {result:?}"
        );
    }

    #[test]
    fn instagram_square_correct_aspect_should_pass_validation() {
        assert!(validate_preset(&instagram_square_with_resolution(1080, 1080)).is_ok());
    }

    #[test]
    fn instagram_reels_preset_should_pass_validation() {
        assert!(validate_preset(&ExportPreset::instagram_reels()).is_ok());
    }
}
