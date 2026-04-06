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
        "youtube_1080p" | "youtube_4k" => {
            if preset.video.fps > 60.0 {
                return Err(EncodeError::PresetConstraintViolation {
                    preset: preset.name.clone(),
                    reason: "fps exceeds YouTube limit of 60".to_string(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{AudioEncoderConfig, ExportPreset, VideoEncoderConfig};
    use crate::{AudioCodec, BitrateMode, VideoCodec};

    fn youtube_1080p_with_fps(fps: f64) -> ExportPreset {
        ExportPreset {
            name: "youtube_1080p".to_string(),
            video: VideoEncoderConfig {
                codec: VideoCodec::H264,
                width: 1920,
                height: 1080,
                fps,
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

    #[test]
    fn youtube_preset_high_fps_should_fail_validation() {
        let preset = youtube_1080p_with_fps(120.0);
        let result = validate_preset(&preset);
        assert!(
            matches!(result, Err(EncodeError::PresetConstraintViolation { .. })),
            "expected PresetConstraintViolation for fps=120, got {result:?}"
        );
    }

    #[test]
    fn youtube_preset_60_fps_should_pass_validation() {
        let preset = youtube_1080p_with_fps(60.0);
        assert!(
            validate_preset(&preset).is_ok(),
            "expected validation to pass for fps=60"
        );
    }

    #[test]
    fn valid_preset_config_should_pass_validation() {
        let preset = ExportPreset::youtube_1080p();
        assert!(
            validate_preset(&preset).is_ok(),
            "expected default youtube_1080p preset to pass validation"
        );
    }
}
