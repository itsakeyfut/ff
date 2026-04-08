//! Audio stream settings for [`VideoEncoderBuilder`].

use super::VideoEncoderBuilder;
use crate::AudioCodec;

impl VideoEncoderBuilder {
    /// Configure audio stream settings.
    #[must_use]
    pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
        self.audio_sample_rate = Some(sample_rate);
        self.audio_channels = Some(channels);
        self
    }

    /// Set audio codec.
    #[must_use]
    pub fn audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = codec;
        self.audio_codec_explicit = true;
        self
    }

    /// Set audio bitrate in bits per second.
    #[must_use]
    pub fn audio_bitrate(mut self, bitrate: u64) -> Self {
        self.audio_bitrate = Some(bitrate);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builder_audio_settings_should_be_stored() {
        let builder = VideoEncoderBuilder::new(PathBuf::from("output.mp4"))
            .audio(48000, 2)
            .audio_codec(AudioCodec::Aac)
            .audio_bitrate(192_000);
        assert_eq!(builder.audio_sample_rate, Some(48000));
        assert_eq!(builder.audio_channels, Some(2));
        assert_eq!(builder.audio_codec, AudioCodec::Aac);
        assert_eq!(builder.audio_bitrate, Some(192_000));
    }
}
