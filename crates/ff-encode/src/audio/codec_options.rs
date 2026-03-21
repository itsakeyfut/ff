//! Per-codec encoding options for [`AudioEncoderBuilder`](super::builder::AudioEncoderBuilder).
//!
//! Pass an [`AudioCodecOptions`] value to
//! `AudioEncoderBuilder::codec_options()` to control codec-specific behaviour.
//! Options are applied via `av_opt_set` / direct field assignment **before**
//! `avcodec_open2`.  Any option that the chosen encoder does not support is
//! logged as a warning and skipped — it never causes `build()` to return an
//! error.

/// Per-codec encoding options for audio.
///
/// The variant must match the codec passed to
/// `AudioEncoderBuilder::audio_codec()`.  A mismatch is silently ignored
/// (the options are not applied).
#[derive(Debug, Clone)]
pub enum AudioCodecOptions {
    /// Opus (libopus) encoding options.
    Opus(OpusOptions),
    /// AAC encoding options.
    Aac(AacOptions),
    /// MP3 (libmp3lame) encoding options.
    Mp3(Mp3Options),
    /// FLAC encoding options.
    Flac(FlacOptions),
}

// ── Opus ──────────────────────────────────────────────────────────────────────

/// Opus (libopus) per-codec options.
#[derive(Debug, Clone)]
pub struct OpusOptions {
    /// Encoder application mode, optimised for the content type.
    pub application: OpusApplication,
    /// Variable bit-rate mode.
    pub vbr: OpusVbr,
}

impl Default for OpusOptions {
    fn default() -> Self {
        Self {
            application: OpusApplication::Audio,
            vbr: OpusVbr::On,
        }
    }
}

/// Opus encoder application mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpusApplication {
    /// Optimised for general audio (music, speech mix). Default.
    #[default]
    Audio,
    /// Optimised for VoIP / speech clarity at low bitrates.
    VoIP,
    /// Minimum latency mode — disables lookahead.
    LowDelay,
}

impl OpusApplication {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::VoIP => "voip",
            Self::LowDelay => "lowdelay",
        }
    }
}

/// Opus variable bit-rate mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpusVbr {
    /// Constant bit-rate.
    Off,
    /// Variable bit-rate (recommended). Default.
    #[default]
    On,
    /// Constrained VBR — guaranteed never to exceed the target bitrate per packet.
    Constrained,
}

impl OpusVbr {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Constrained => "constrained",
        }
    }
}

// ── AAC ───────────────────────────────────────────────────────────────────────

/// AAC per-codec options.
#[derive(Debug, Clone)]
pub struct AacOptions {
    /// Enable the afterburner quality enhancement pass (libfdk_aac only).
    ///
    /// Increases quality at the cost of slightly higher CPU usage. Silently
    /// ignored when using the native `aac` encoder (logged as a warning).
    pub afterburner: bool,
}

impl Default for AacOptions {
    fn default() -> Self {
        Self { afterburner: true }
    }
}

// ── MP3 ───────────────────────────────────────────────────────────────────────

/// MP3 (libmp3lame) per-codec options.
#[derive(Debug, Clone)]
pub struct Mp3Options {
    /// VBR quality level (0–9). `0` = best quality, `9` = smallest file.
    ///
    /// Only takes effect when the builder is configured for VBR-style encoding.
    /// Silently ignored if `av_opt_set` does not accept the value.
    pub quality: u8,
}

impl Default for Mp3Options {
    fn default() -> Self {
        Self { quality: 4 }
    }
}

// ── FLAC ──────────────────────────────────────────────────────────────────────

/// FLAC per-codec options.
#[derive(Debug, Clone)]
pub struct FlacOptions {
    /// Compression level (0–12). `0` = fastest / largest, `12` = slowest / smallest.
    pub compression_level: u8,
}

impl Default for FlacOptions {
    fn default() -> Self {
        Self {
            compression_level: 5,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_application_should_return_correct_str() {
        assert_eq!(OpusApplication::Audio.as_str(), "audio");
        assert_eq!(OpusApplication::VoIP.as_str(), "voip");
        assert_eq!(OpusApplication::LowDelay.as_str(), "lowdelay");
    }

    #[test]
    fn opus_vbr_should_return_correct_str() {
        assert_eq!(OpusVbr::Off.as_str(), "off");
        assert_eq!(OpusVbr::On.as_str(), "on");
        assert_eq!(OpusVbr::Constrained.as_str(), "constrained");
    }

    #[test]
    fn opus_options_default_should_have_audio_application() {
        let opts = OpusOptions::default();
        assert_eq!(opts.application, OpusApplication::Audio);
        assert_eq!(opts.vbr, OpusVbr::On);
    }

    #[test]
    fn aac_options_default_should_have_afterburner_enabled() {
        let opts = AacOptions::default();
        assert!(opts.afterburner);
    }

    #[test]
    fn mp3_options_default_should_have_quality_4() {
        let opts = Mp3Options::default();
        assert_eq!(opts.quality, 4);
    }

    #[test]
    fn flac_options_default_should_have_compression_level_5() {
        let opts = FlacOptions::default();
        assert_eq!(opts.compression_level, 5);
    }

    #[test]
    fn audio_codec_options_enum_variants_are_accessible() {
        let _opus = AudioCodecOptions::Opus(OpusOptions::default());
        let _aac = AudioCodecOptions::Aac(AacOptions::default());
        let _mp3 = AudioCodecOptions::Mp3(Mp3Options::default());
        let _flac = AudioCodecOptions::Flac(FlacOptions::default());
    }
}
