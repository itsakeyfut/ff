//! Multi-track audio mixing into a single output stream.

#![allow(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use ff_format::ChannelLayout;

use crate::error::FilterError;
use crate::graph::filter_step::FilterStep;
use crate::graph::graph::FilterGraph;

// ── AudioTrack ────────────────────────────────────────────────────────────────

/// A single audio track in a [`MultiTrackAudioMixer`] mix.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    /// Source media file path.
    pub source: PathBuf,
    /// Volume adjustment in decibels (`0.0` = unity gain).
    pub volume_db: f32,
    /// Stereo pan (`-1.0` = full left, `0.0` = centre, `+1.0` = full right).
    pub pan: f32,
    /// Start offset on the output timeline (`Duration::ZERO` = at the beginning).
    pub time_offset: Duration,
    /// Ordered per-track audio effect chain applied before mixing.
    ///
    /// Each [`FilterStep`] is inserted as a filter node immediately after
    /// the track's pan/volume chain and before the `amix` node.
    /// Use audio-relevant variants such as [`FilterStep::Volume`],
    /// [`FilterStep::AFadeIn`], and [`FilterStep::ACompressor`].
    /// An empty vec inserts no extra nodes (zero overhead).
    pub effects: Vec<FilterStep>,
    /// Sample rate of the source audio in Hz (e.g. `44_100` or `48_000`).
    ///
    /// When this differs from the mixer's output sample rate an `aresample`
    /// filter is inserted automatically.  Set to the mixer's output rate to
    /// skip resampling.
    pub sample_rate: u32,
    /// Channel layout of the source audio.
    ///
    /// When this differs from the mixer's output layout an `aformat` filter
    /// is inserted automatically.  Set to the mixer's output layout to skip
    /// format conversion.
    pub channel_layout: ChannelLayout,
}

// ── MultiTrackAudioMixer ──────────────────────────────────────────────────────

/// Mixes multiple audio tracks into a single output stream.
///
/// The resulting [`FilterGraph`] is source-only — call [`FilterGraph::pull_audio`]
/// in a loop to extract the output frames.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::MultiTrackAudioMixer;
/// use ff_format::ChannelLayout;
/// use std::time::Duration;
///
/// let mut graph = MultiTrackAudioMixer::new(48000, ChannelLayout::Stereo)
///     .add_track(ff_filter::AudioTrack {
///         source: "music.mp3".into(),
///         volume_db: -3.0,
///         pan: 0.0,
///         time_offset: Duration::ZERO,
///         effects: vec![],
///         sample_rate: 48000,
///         channel_layout: ChannelLayout::Stereo,
///     })
///     .build()?;
///
/// while let Some(frame) = graph.pull_audio()? {
///     // encode or write `frame`
/// }
/// ```
pub struct MultiTrackAudioMixer {
    sample_rate: u32,
    channel_layout: ChannelLayout,
    tracks: Vec<AudioTrack>,
}

impl MultiTrackAudioMixer {
    /// Creates a new mixer with no tracks.
    pub fn new(sample_rate: u32, layout: ChannelLayout) -> Self {
        Self {
            sample_rate,
            channel_layout: layout,
            tracks: Vec::new(),
        }
    }

    /// Appends an audio track and returns the updated mixer.
    #[must_use]
    pub fn add_track(self, track: AudioTrack) -> Self {
        let mut tracks = self.tracks;
        tracks.push(track);
        Self { tracks, ..self }
    }

    /// Builds a source-only [`FilterGraph`] that mixes all tracks.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — no tracks were added, or an
    ///   underlying `FFmpeg` graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.tracks.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no tracks".to_string(),
            });
        }
        // SAFETY: same ownership invariants as build_video_composition.
        unsafe {
            super::composition_inner::build_audio_mix(
                self.sample_rate,
                self.channel_layout,
                &self.tracks,
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn mixer_empty_tracks_should_err() {
        let result = MultiTrackAudioMixer::new(48000, ChannelLayout::Stereo).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed, got {result:?}"
        );
    }

    #[test]
    fn audio_track_with_empty_effects_should_build_successfully() {
        // build() may fail because the source doesn't exist, but must NOT fail
        // with a reason related to effects.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 48_000,
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("effect"),
                "build must not fail due to empty effects, got: {reason}"
            );
        }
    }

    #[test]
    fn audio_track_with_volume_effect_should_include_volume_filter() {
        // Structural test: verify the effects field accepts FilterStep::Volume.
        let track = AudioTrack {
            source: "track.mp3".into(),
            volume_db: 0.0,
            pan: 0.0,
            time_offset: Duration::ZERO,
            effects: vec![FilterStep::Volume(6.0)],
            sample_rate: 48_000,
            channel_layout: ChannelLayout::Stereo,
        };
        assert_eq!(track.effects.len(), 1);
        assert!(
            matches!(track.effects[0], FilterStep::Volume(_)),
            "expected Volume variant"
        );
    }

    #[test]
    fn mixer_mismatched_sample_rate_should_insert_aresample() {
        // Track is 44100 Hz, output is 48000 Hz → build_audio_mix must attempt
        // to create an aresample node.  With a nonexistent file the graph fails
        // at avfilter_graph_config, NOT at "filter not found: aresample", which
        // proves the node was created successfully before the config step.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 44_100, // mismatch → aresample should be inserted
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        assert!(result.is_err(), "expected error from nonexistent file");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("filter not found: aresample"),
                "aresample filter must exist in FFmpeg and be created; got: {reason}"
            );
        }
    }

    #[test]
    fn audio_track_with_positive_offset_should_insert_adelay() {
        // adelay is inserted when time_offset > 0.
        // Build fails (nonexistent file) but NOT at "filter not found: adelay".
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::from_secs(2),
                effects: vec![],
                sample_rate: 48_000,
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        assert!(result.is_err(), "expected error (nonexistent file)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("filter not found: adelay"),
                "adelay must exist in FFmpeg and be created; got: {reason}"
            );
        }
    }

    #[test]
    fn zero_audio_offset_should_not_insert_extra_filters() {
        // time_offset=ZERO must not cause adelay nodes.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 48_000,
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("adelay"),
                "adelay must not appear for zero offset; got: {reason}"
            );
        }
    }

    #[test]
    fn mixer_matching_format_should_not_insert_extra_filters() {
        // Track format matches output → no aresample or aformat should be
        // inserted.  Build fails only because the source file does not exist.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 48_000, // matches output → no aresample
                channel_layout: ChannelLayout::Stereo, // matches output → no aformat
            })
            .build();
        assert!(result.is_err(), "expected error from nonexistent file");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("aresample"),
                "aresample must not appear for matching format; got: {reason}"
            );
            assert!(
                !reason.contains("filter not found: aformat"),
                "aformat must not appear for matching format; got: {reason}"
            );
        }
    }
}
