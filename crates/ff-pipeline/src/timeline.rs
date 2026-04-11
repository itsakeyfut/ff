//! Timeline data type for multi-track composition.
//!
//! This module provides [`Timeline`] and [`TimelineBuilder`], which represent
//! an ordered layout of [`Clip`] instances across video and audio tracks.
//! `Timeline` holds no `FFmpeg` context; all rendering is done in
//! [`Timeline::render()`].

use std::path::Path;

use ff_decode::VideoDecoder;
use ff_encode::VideoEncoder;
use ff_filter::{AnimatedValue, AudioTrack, MultiTrackAudioMixer, MultiTrackComposer, VideoLayer};
use ff_format::ChannelLayout;

use crate::clip::Clip;
use crate::encoder_config::EncoderConfig;
use crate::error::PipelineError;
use crate::pipeline::hwaccel_to_hardware_encoder;

/// An ordered layout of [`Clip`] instances across video and audio tracks.
///
/// `Timeline` is a plain Rust value type — it holds no `FFmpeg` context.
/// All rendering happens in [`Timeline::render()`].
///
/// # Construction
///
/// Use [`Timeline::builder()`] to obtain a [`TimelineBuilder`].
///
/// # Examples
///
/// ```
/// use ff_pipeline::{Clip, Timeline};
/// use std::time::Duration;
///
/// let clip = Clip::new("intro.mp4")
///     .trim(Duration::from_secs(0), Duration::from_secs(5));
///
/// let result = Timeline::builder()
///     .canvas(1920, 1080)
///     .frame_rate(30.0)
///     .video_track(vec![clip])
///     .build();
///
/// assert!(result.is_ok());
/// ```
#[derive(Debug, Clone)]
pub struct Timeline {
    pub(crate) canvas_width: u32,
    pub(crate) canvas_height: u32,
    pub(crate) frame_rate: f64,
    /// `video_tracks[track_idx][clip_idx]`; track 0 = bottom layer.
    pub(crate) video_tracks: Vec<Vec<Clip>>,
    pub(crate) audio_tracks: Vec<Vec<Clip>>,
}

impl Timeline {
    /// Returns a new [`TimelineBuilder`].
    pub fn builder() -> TimelineBuilder {
        TimelineBuilder::new()
    }

    /// Returns the canvas width in pixels.
    pub fn canvas_width(&self) -> u32 {
        self.canvas_width
    }

    /// Returns the canvas height in pixels.
    pub fn canvas_height(&self) -> u32 {
        self.canvas_height
    }

    /// Returns the frame rate in frames per second.
    pub fn frame_rate(&self) -> f64 {
        self.frame_rate
    }

    /// Returns a slice of all video tracks.
    pub fn video_tracks(&self) -> &[Vec<Clip>] {
        &self.video_tracks
    }

    /// Returns a slice of all audio tracks.
    pub fn audio_tracks(&self) -> &[Vec<Clip>] {
        &self.audio_tracks
    }

    /// Renders the timeline to an output file.
    ///
    /// # Errors
    ///
    /// - [`PipelineError::ClipNotFound`] — a clip's source file is missing
    /// - [`PipelineError::Encode`] — encoder failure
    /// - [`PipelineError::Filter`] — filter graph construction failure
    /// - [`PipelineError::TimelineRenderFailed`] — other structural failure
    pub fn render(
        self,
        output: impl AsRef<Path>,
        config: EncoderConfig,
    ) -> Result<(), PipelineError> {
        let output = output.as_ref();
        let nv = self.video_tracks.len();
        let na = self.audio_tracks.len();

        // 1. Pre-check: all clip sources must exist on disk.
        for track in self.video_tracks.iter().chain(self.audio_tracks.iter()) {
            for clip in track {
                if !clip.source.exists() {
                    return Err(PipelineError::ClipNotFound {
                        path: clip.source.to_string_lossy().into_owned(),
                    });
                }
            }
        }

        // 2. Build video composition graph.
        let mut video_graph = None;
        if !self.video_tracks.is_empty() {
            let mut composer = MultiTrackComposer::new(self.canvas_width, self.canvas_height);
            for (track_idx, track) in self.video_tracks.iter().enumerate() {
                for clip in track {
                    composer = composer.add_layer(VideoLayer {
                        source: clip.source.clone(),
                        x: AnimatedValue::Static(0.0),
                        y: AnimatedValue::Static(0.0),
                        scale_x: AnimatedValue::Static(1.0),
                        scale_y: AnimatedValue::Static(1.0),
                        rotation: AnimatedValue::Static(0.0),
                        opacity: AnimatedValue::Static(1.0),
                        z_order: u32::try_from(track_idx).unwrap_or(u32::MAX),
                        time_offset: clip.timeline_offset,
                        in_point: clip.in_point,
                        out_point: clip.out_point,
                    });
                }
            }
            video_graph = Some(composer.build().map_err(PipelineError::Filter)?);
        }

        // 3. Build audio mix graph.
        let mut audio_graph = None;
        if !self.audio_tracks.is_empty() {
            let mut mixer = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo);
            for track in &self.audio_tracks {
                for clip in track {
                    mixer = mixer.add_track(AudioTrack {
                        source: clip.source.clone(),
                        volume: AnimatedValue::Static(0.0),
                        pan: AnimatedValue::Static(0.0),
                        time_offset: clip.timeline_offset,
                        effects: vec![],
                        sample_rate: 48_000,
                        channel_layout: ff_format::ChannelLayout::Stereo,
                    });
                }
            }
            audio_graph = Some(mixer.build().map_err(PipelineError::Filter)?);
        }

        // 4. Build encoder.
        let hw = hwaccel_to_hardware_encoder(config.hardware);
        let mut enc_builder = VideoEncoder::create(output)
            .video(self.canvas_width, self.canvas_height, self.frame_rate)
            .video_codec(config.video_codec)
            .bitrate_mode(config.bitrate_mode)
            .hardware_encoder(hw);
        if audio_graph.is_some() {
            enc_builder = enc_builder.audio(48_000, 2).audio_codec(config.audio_codec);
        }
        let mut encoder = enc_builder.build().map_err(PipelineError::Encode)?;

        // 5. Drain video graph → encoder.
        if let Some(mut vgraph) = video_graph {
            while let Some(frame) = vgraph.pull_video().map_err(PipelineError::Filter)? {
                encoder.push_video(&frame).map_err(PipelineError::Encode)?;
            }
        }

        // 6. Drain audio graph → encoder.
        if let Some(mut agraph) = audio_graph {
            while let Some(frame) = agraph.pull_audio().map_err(PipelineError::Filter)? {
                encoder.push_audio(&frame).map_err(PipelineError::Encode)?;
            }
        }

        // 7. Flush encoder.
        encoder.finish().map_err(PipelineError::Encode)?;

        log::info!(
            "timeline render complete output={} video_tracks={nv} audio_tracks={na}",
            output.display()
        );
        Ok(())
    }
}

/// Builder for [`Timeline`].
///
/// Obtain one via [`Timeline::builder()`].
pub struct TimelineBuilder {
    canvas_width: Option<u32>,
    canvas_height: Option<u32>,
    frame_rate: Option<f64>,
    video_tracks: Vec<Vec<Clip>>,
    audio_tracks: Vec<Vec<Clip>>,
}

impl Default for TimelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TimelineBuilder {
    /// Creates a new builder with no tracks and no canvas/frame-rate set.
    pub fn new() -> Self {
        Self {
            canvas_width: None,
            canvas_height: None,
            frame_rate: None,
            video_tracks: Vec::new(),
            audio_tracks: Vec::new(),
        }
    }

    /// Sets the output canvas dimensions in pixels.
    #[must_use]
    pub fn canvas(self, width: u32, height: u32) -> Self {
        Self {
            canvas_width: Some(width),
            canvas_height: Some(height),
            ..self
        }
    }

    /// Sets the output frame rate in frames per second.
    #[must_use]
    pub fn frame_rate(self, fps: f64) -> Self {
        Self {
            frame_rate: Some(fps),
            ..self
        }
    }

    /// Appends a video track. Track 0 (first call) is the bottom layer.
    #[must_use]
    pub fn video_track(self, clips: Vec<Clip>) -> Self {
        let mut video_tracks = self.video_tracks;
        video_tracks.push(clips);
        Self {
            video_tracks,
            ..self
        }
    }

    /// Appends an audio track.
    #[must_use]
    pub fn audio_track(self, clips: Vec<Clip>) -> Self {
        let mut audio_tracks = self.audio_tracks;
        audio_tracks.push(clips);
        Self {
            audio_tracks,
            ..self
        }
    }

    /// Builds the [`Timeline`].
    ///
    /// # Errors
    ///
    /// - [`PipelineError::NoInput`] — both track lists are empty
    /// - [`PipelineError::ClipNotFound`] — canvas/fps auto-probe needed but
    ///   the first video clip's source file does not exist
    /// - [`PipelineError::Decode`] — the first video clip could not be opened
    pub fn build(self) -> Result<Timeline, PipelineError> {
        if self.video_tracks.is_empty() && self.audio_tracks.is_empty() {
            return Err(PipelineError::NoInput);
        }

        let (canvas_width, canvas_height, frame_rate) = self.resolve_canvas_and_fps()?;

        Ok(Timeline {
            canvas_width,
            canvas_height,
            frame_rate,
            video_tracks: self.video_tracks,
            audio_tracks: self.audio_tracks,
        })
    }

    /// Resolves canvas dimensions and frame rate.
    ///
    /// When all three values are explicitly set, returns them directly.
    /// Otherwise probes the first video clip with `VideoDecoder`. For
    /// audio-only timelines (no video tracks) falls back to 1920×1080 @ 30 fps.
    fn resolve_canvas_and_fps(&self) -> Result<(u32, u32, f64), PipelineError> {
        let need_probe = self.canvas_width.is_none()
            || self.canvas_height.is_none()
            || self.frame_rate.is_none();

        if need_probe && let Some(first_clip) = self.video_tracks.first().and_then(|t| t.first()) {
            if !first_clip.source.exists() {
                return Err(PipelineError::ClipNotFound {
                    path: first_clip.source.to_string_lossy().into_owned(),
                });
            }
            let vdec = VideoDecoder::open(&first_clip.source).build()?;
            let w = self.canvas_width.unwrap_or_else(|| vdec.width());
            let h = self.canvas_height.unwrap_or_else(|| vdec.height());
            let fps = self.frame_rate.unwrap_or_else(|| vdec.frame_rate());
            return Ok((w, h, fps));
        }

        // All values explicit, or no video tracks (audio-only) — fall back for absent values.
        Ok((
            self.canvas_width.unwrap_or(1920),
            self.canvas_height.unwrap_or(1080),
            self.frame_rate.unwrap_or(30.0),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn timeline_builder_should_err_when_no_tracks() {
        let result = Timeline::builder().build();
        assert!(matches!(result, Err(PipelineError::NoInput)));
    }

    #[test]
    fn timeline_builder_should_succeed_with_video_track() {
        let clip = Clip::new("video.mp4");
        let timeline = Timeline::builder()
            .canvas(1920, 1080)
            .frame_rate(30.0)
            .video_track(vec![clip])
            .build()
            .unwrap();

        assert_eq!(timeline.canvas_width, 1920);
        assert_eq!(timeline.canvas_height, 1080);
        assert!((timeline.frame_rate - 30.0).abs() < f64::EPSILON);
        assert_eq!(timeline.video_tracks.len(), 1);
        assert!(timeline.audio_tracks.is_empty());
    }
}
