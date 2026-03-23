//! Multi-rendition ABR ladder for live frame-push output.
//!
//! [`LiveAbrLadder`] receives pre-decoded [`VideoFrame`] / [`AudioFrame`] values
//! from the caller and fans them out to multiple encoders — one per rendition —
//! each with its own resolution and bitrate. After [`StreamOutput::finish`], it
//! writes a master playlist (`master.m3u8` for HLS, `manifest.mpd` for DASH).
//!
//! Each rendition is encoded and muxed independently by a [`LiveHlsOutput`] or
//! [`LiveDashOutput`] instance stored inside the ladder. The input frame is
//! passed to every rendition's `push_video` without pre-scaling; each inner
//! encoder uses its own `SwsContext` to scale from the source resolution to the
//! rendition's target dimensions.
//!
//! # Example
//!
//! ```ignore
//! use ff_stream::{LiveAbrFormat, LiveAbrLadder, AbrRendition, StreamOutput};
//! use std::time::Duration;
//!
//! let mut ladder = LiveAbrLadder::new("/var/www/live")
//!     .add_rendition(AbrRendition {
//!         width: 1920, height: 1080,
//!         video_bitrate: 4_000_000, audio_bitrate: 192_000, name: None,
//!     })
//!     .add_rendition(AbrRendition {
//!         width: 1280, height: 720,
//!         video_bitrate: 2_000_000, audio_bitrate: 128_000, name: None,
//!     })
//!     .fps(30.0)
//!     .audio(48000, 2)
//!     .segment_duration(Duration::from_secs(6))
//!     .build()?;
//!
//! // for each decoded frame:
//! ladder.push_video(&video_frame)?;
//! ladder.push_audio(&audio_frame)?;
//!
//! // when done:
//! Box::new(ladder).finish()?;  // also writes master.m3u8
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use ff_format::{AudioFrame, VideoCodec, VideoFrame};

use crate::error::StreamError;
use crate::live_dash::LiveDashOutput;
use crate::live_hls::LiveHlsOutput;
use crate::output::StreamOutput;

// ============================================================================
// AbrRendition
// ============================================================================

/// One resolution/bitrate tier in an ABR ladder.
///
/// Each rendition becomes an independent encoder stream. The output files are
/// placed in a subdirectory named after the rendition (see [`dir_name`](Self::dir_name)).
pub struct AbrRendition {
    /// Target video width in pixels.
    pub width: u32,
    /// Target video height in pixels.
    pub height: u32,
    /// Target video encoder bitrate in bits per second.
    pub video_bitrate: u64,
    /// Target audio encoder bitrate in bits per second.
    pub audio_bitrate: u64,
    /// Optional subdirectory name. Defaults to `"{width}x{height}"`.
    pub name: Option<String>,
}

impl AbrRendition {
    /// Returns the subdirectory name for this rendition.
    ///
    /// Uses [`name`](Self::name) if set; otherwise `"{width}x{height}"`.
    ///
    /// # Example
    ///
    /// ```
    /// use ff_stream::AbrRendition;
    ///
    /// let r = AbrRendition { width: 1920, height: 1080,
    ///     video_bitrate: 4_000_000, audio_bitrate: 192_000, name: None };
    /// assert_eq!(r.dir_name(), "1920x1080");
    ///
    /// let r2 = AbrRendition { width: 1280, height: 720,
    ///     video_bitrate: 2_000_000, audio_bitrate: 128_000,
    ///     name: Some("720p".into()) };
    /// assert_eq!(r2.dir_name(), "720p");
    /// ```
    #[must_use]
    pub fn dir_name(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{}x{}", self.width, self.height))
    }
}

// ============================================================================
// LiveAbrFormat
// ============================================================================

/// Output container format for the ABR ladder.
pub enum LiveAbrFormat {
    /// Segmented HLS (`.ts` + `index.m3u8`). Writes `master.m3u8` on finish.
    Hls,
    /// Segmented DASH (`.m4s` + per-rendition `manifest.mpd`). Writes a
    /// top-level `manifest.mpd` on finish.
    Dash,
}

// ============================================================================
// LiveAbrLadder — safe builder + StreamOutput impl
// ============================================================================

/// Live ABR ladder: fans frames to multiple encoders at different resolutions.
///
/// Build with [`LiveAbrLadder::new`], add renditions, configure encoding
/// parameters, then call [`build`](Self::build). After that:
///
/// - [`push_video`](Self::push_video) and [`push_audio`](Self::push_audio)
///   forward frames to all renditions (each scales internally via its own
///   `SwsContext`).
/// - [`StreamOutput::finish`] flushes all encoders and writes the master
///   playlist.
///
/// All rendition subdirectories are created by `build()` if they do not exist.
pub struct LiveAbrLadder {
    output_dir: PathBuf,
    renditions: Vec<AbrRendition>,
    format: LiveAbrFormat,
    segment_duration: Duration,
    playlist_size: u32,
    video_codec: VideoCodec,
    fps: Option<f64>,
    sample_rate: Option<u32>,
    channels: Option<u32>,
    /// Populated by `build()`. Empty before that.
    outputs: Vec<Box<dyn StreamOutput>>,
    finished: bool,
}

impl LiveAbrLadder {
    /// Create a new builder that writes the ABR ladder to `output_dir`.
    ///
    /// Accepts any path-like value: `"/var/www/live"`, `Path::new(…)`, etc.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ff_stream::LiveAbrLadder;
    ///
    /// let ladder = LiveAbrLadder::new("/var/www/live");
    /// ```
    #[must_use]
    pub fn new(output_dir: impl AsRef<Path>) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            renditions: Vec::new(),
            format: LiveAbrFormat::Hls,
            segment_duration: Duration::from_secs(6),
            playlist_size: 5,
            video_codec: VideoCodec::H264,
            fps: None,
            sample_rate: None,
            channels: None,
            outputs: Vec::new(),
            finished: false,
        }
    }

    /// Add a rendition to the ladder.
    ///
    /// At least one rendition is required; [`build`](Self::build) returns
    /// [`StreamError::InvalidConfig`] when the list is empty.
    #[must_use]
    pub fn add_rendition(mut self, rendition: AbrRendition) -> Self {
        self.renditions.push(rendition);
        self
    }

    /// Set the output container format.
    ///
    /// Default: [`LiveAbrFormat::Hls`].
    #[must_use]
    pub fn format(mut self, format: LiveAbrFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the frame rate used for all renditions.
    ///
    /// This method **must** be called before [`build`](Self::build).
    #[must_use]
    pub fn fps(mut self, fps: f64) -> Self {
        self.fps = Some(fps);
        self
    }

    /// Enable audio output with the given sample rate and channel count.
    ///
    /// If not called, audio is disabled for all renditions.
    #[must_use]
    pub fn audio(mut self, sample_rate: u32, channels: u32) -> Self {
        self.sample_rate = Some(sample_rate);
        self.channels = Some(channels);
        self
    }

    /// Set the target segment duration for all renditions.
    ///
    /// Default: 6 seconds.
    #[must_use]
    pub fn segment_duration(mut self, duration: Duration) -> Self {
        self.segment_duration = duration;
        self
    }

    /// Set the sliding-window playlist size (HLS only).
    ///
    /// Default: 5.
    #[must_use]
    pub fn playlist_size(mut self, size: u32) -> Self {
        self.playlist_size = size;
        self
    }

    /// Set the video codec for all renditions.
    ///
    /// Default: [`VideoCodec::H264`].
    #[must_use]
    pub fn video_codec(mut self, codec: VideoCodec) -> Self {
        self.video_codec = codec;
        self
    }

    /// Open all per-rendition `FFmpeg` contexts.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::InvalidConfig`] when:
    /// - `output_dir` is empty.
    /// - No renditions have been added.
    /// - [`fps`](Self::fps) was not called.
    ///
    /// Returns [`StreamError::Io`] when a rendition subdirectory cannot be
    /// created. Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation
    /// fails.
    pub fn build(mut self) -> Result<Self, StreamError> {
        if self.output_dir.as_os_str().is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "output_dir must not be empty".into(),
            });
        }

        if self.renditions.is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "at least one rendition is required; call .add_rendition() before .build()"
                    .into(),
            });
        }

        let fps = self.fps.ok_or_else(|| StreamError::InvalidConfig {
            reason: "fps not set; call .fps(value) before .build()".into(),
        })?;

        std::fs::create_dir_all(&self.output_dir)?;

        let mut outputs: Vec<Box<dyn StreamOutput>> = Vec::with_capacity(self.renditions.len());

        for rendition in &self.renditions {
            let rendition_dir = self.output_dir.join(rendition.dir_name());

            let output: Box<dyn StreamOutput> = match self.format {
                LiveAbrFormat::Hls => {
                    let mut builder = LiveHlsOutput::new(&rendition_dir)
                        .video(rendition.width, rendition.height, fps)
                        .video_bitrate(rendition.video_bitrate)
                        .audio_bitrate(rendition.audio_bitrate)
                        .segment_duration(self.segment_duration)
                        .playlist_size(self.playlist_size)
                        .video_codec(self.video_codec);

                    if let (Some(sr), Some(ch)) = (self.sample_rate, self.channels) {
                        builder = builder.audio(sr, ch);
                    }

                    Box::new(builder.build()?)
                }
                LiveAbrFormat::Dash => {
                    let mut builder = LiveDashOutput::new(&rendition_dir)
                        .video(rendition.width, rendition.height, fps)
                        .video_bitrate(rendition.video_bitrate)
                        .audio_bitrate(rendition.audio_bitrate)
                        .segment_duration(self.segment_duration)
                        .video_codec(self.video_codec);

                    if let (Some(sr), Some(ch)) = (self.sample_rate, self.channels) {
                        builder = builder.audio(sr, ch);
                    }

                    Box::new(builder.build()?)
                }
            };

            outputs.push(output);
        }

        self.outputs = outputs;
        Ok(self)
    }
}

// ============================================================================
// StreamOutput impl
// ============================================================================

impl StreamOutput for LiveAbrLadder {
    fn push_video(&mut self, frame: &VideoFrame) -> Result<(), StreamError> {
        if self.finished {
            return Err(StreamError::InvalidConfig {
                reason: "push_video called after finish()".into(),
            });
        }
        if self.outputs.is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "push_video called before build()".into(),
            });
        }
        for output in &mut self.outputs {
            output.push_video(frame)?;
        }
        Ok(())
    }

    fn push_audio(&mut self, frame: &AudioFrame) -> Result<(), StreamError> {
        if self.finished {
            return Err(StreamError::InvalidConfig {
                reason: "push_audio called after finish()".into(),
            });
        }
        if self.outputs.is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "push_audio called before build()".into(),
            });
        }
        for output in &mut self.outputs {
            output.push_audio(frame)?;
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), StreamError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;

        let outputs = std::mem::take(&mut self.outputs);
        for output in outputs {
            output.finish()?;
        }

        match self.format {
            LiveAbrFormat::Hls => {
                write_hls_master(&self.output_dir, &self.renditions)?;
            }
            LiveAbrFormat::Dash => {
                write_dash_manifest(&self.output_dir, &self.renditions)?;
            }
        }

        log::info!(
            "live_abr finished output_dir={} renditions={}",
            self.output_dir.display(),
            self.renditions.len()
        );
        Ok(())
    }
}

// ============================================================================
// Playlist writers
// ============================================================================

/// Write `master.m3u8` listing all rendition variant streams.
fn write_hls_master(output_dir: &Path, renditions: &[AbrRendition]) -> Result<(), StreamError> {
    use std::fmt::Write as _;

    let mut content = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
    for r in renditions {
        let bandwidth = r.video_bitrate + r.audio_bitrate;
        let dir = r.dir_name();
        let _ = write!(
            content,
            "#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},RESOLUTION={}x{}\n{dir}/index.m3u8\n",
            r.width, r.height,
        );
    }

    let master_path = output_dir.join("master.m3u8");
    std::fs::write(&master_path, content)?;
    log::info!(
        "live_abr wrote master playlist path={}",
        master_path.display()
    );
    Ok(())
}

/// Write a top-level `manifest.mpd` referencing all rendition subdirectories.
fn write_dash_manifest(output_dir: &Path, renditions: &[AbrRendition]) -> Result<(), StreamError> {
    use std::fmt::Write as _;

    let mut representations = String::new();
    for r in renditions {
        let bandwidth = r.video_bitrate + r.audio_bitrate;
        let dir = r.dir_name();
        let _ = write!(
            representations,
            "      <Representation bandwidth=\"{bandwidth}\" width=\"{}\" height=\"{}\">\
\n        <BaseURL>{dir}/</BaseURL>\n      </Representation>\n",
            r.width, r.height,
        );
    }

    let content = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<MPD xmlns=\"urn:mpeg:dash:schema:mpd:2011\" type=\"dynamic\"\
 profiles=\"urn:mpeg:dash:profile:isoff-live:2011\">\n\
  <Period>\n\
    <AdaptationSet mimeType=\"video/mp4\" segmentAlignment=\"true\">\n\
{representations}\
    </AdaptationSet>\n\
  </Period>\n\
</MPD>\n"
    );

    let manifest_path = output_dir.join("manifest.mpd");
    std::fs::write(&manifest_path, content)?;
    log::info!(
        "live_abr wrote dash manifest path={}",
        manifest_path.display()
    );
    Ok(())
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_with_empty_output_dir_should_return_invalid_config() {
        let result = LiveAbrLadder::new("")
            .add_rendition(AbrRendition {
                width: 1280,
                height: 720,
                video_bitrate: 2_000_000,
                audio_bitrate: 128_000,
                name: None,
            })
            .fps(30.0)
            .build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_with_no_renditions_should_return_invalid_config() {
        let result = LiveAbrLadder::new("/tmp/live_abr_test_no_renditions")
            .fps(30.0)
            .build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn build_without_fps_should_return_invalid_config() {
        let result = LiveAbrLadder::new("/tmp/live_abr_test_no_fps")
            .add_rendition(AbrRendition {
                width: 1280,
                height: 720,
                video_bitrate: 2_000_000,
                audio_bitrate: 128_000,
                name: None,
            })
            .build();
        assert!(matches!(result, Err(StreamError::InvalidConfig { .. })));
    }

    #[test]
    fn segment_duration_default_should_be_six_seconds() {
        let ladder = LiveAbrLadder::new("/tmp/x");
        assert_eq!(ladder.segment_duration, Duration::from_secs(6));
    }

    #[test]
    fn playlist_size_default_should_be_five() {
        let ladder = LiveAbrLadder::new("/tmp/x");
        assert_eq!(ladder.playlist_size, 5);
    }

    #[test]
    fn abr_rendition_dir_name_default_should_use_resolution() {
        let r = AbrRendition {
            width: 1920,
            height: 1080,
            video_bitrate: 4_000_000,
            audio_bitrate: 192_000,
            name: None,
        };
        assert_eq!(r.dir_name(), "1920x1080");
    }

    #[test]
    fn abr_rendition_dir_name_custom_should_use_name() {
        let r = AbrRendition {
            width: 1280,
            height: 720,
            video_bitrate: 2_000_000,
            audio_bitrate: 128_000,
            name: Some("720p".into()),
        };
        assert_eq!(r.dir_name(), "720p");
    }
}
