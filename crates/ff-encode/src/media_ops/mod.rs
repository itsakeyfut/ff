//! Media stream operations — audio replacement via stream-copy remux.

mod media_inner;

use std::path::PathBuf;

use crate::error::EncodeError;

/// Replace a video file's audio track with audio from a separate source file.
///
/// The video bitstream is copied bit-for-bit (no decode/encode cycle).  The
/// audio track from `audio_input` replaces any existing audio in
/// `video_input`.
///
/// Returns [`EncodeError::MediaOperationFailed`] when no audio stream is found
/// in `audio_input`, or no video stream is found in `video_input`.
///
/// # Example
///
/// ```ignore
/// use ff_encode::AudioReplacement;
///
/// AudioReplacement::new("source.mp4", "new_audio.aac", "output.mp4").run()?;
/// ```
pub struct AudioReplacement {
    video_input: PathBuf,
    audio_input: PathBuf,
    output: PathBuf,
}

impl AudioReplacement {
    /// Create a new `AudioReplacement`.
    ///
    /// - `video_input` — source file whose video stream is kept.
    /// - `audio_input` — source file whose first audio stream is used.
    /// - `output`      — path for the combined output file.
    pub fn new(
        video_input: impl Into<PathBuf>,
        audio_input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
    ) -> Self {
        Self {
            video_input: video_input.into(),
            audio_input: audio_input.into(),
            output: output.into(),
        }
    }

    /// Execute the audio replacement operation.
    ///
    /// # Errors
    ///
    /// - [`EncodeError::MediaOperationFailed`] if `video_input` has no video
    ///   stream or `audio_input` has no audio stream.
    /// - [`EncodeError::Ffmpeg`] if any FFmpeg API call fails.
    pub fn run(self) -> Result<(), EncodeError> {
        log::debug!(
            "audio replacement start video_input={} audio_input={} output={}",
            self.video_input.display(),
            self.audio_input.display(),
            self.output.display(),
        );
        media_inner::run_audio_replacement(&self.video_input, &self.audio_input, &self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_replacement_run_with_nonexistent_video_input_should_fail() {
        let result =
            AudioReplacement::new("nonexistent_video.mp4", "nonexistent_audio.mp3", "out.mp4")
                .run();
        assert!(
            result.is_err(),
            "expected error for nonexistent video input, got Ok(())"
        );
    }
}
