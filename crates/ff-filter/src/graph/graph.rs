//! [`FilterGraph`] struct definition and push/pull implementations.

use ff_format::{AudioFrame, VideoFrame};

use crate::animation::AnimationEntry;
use crate::error::FilterError;
use crate::filter_inner::FilterGraphInner;

use super::builder::FilterGraphBuilder;

// ── FilterGraph ───────────────────────────────────────────────────────────────

/// An `FFmpeg` libavfilter filter graph.
///
/// Constructed via [`FilterGraph::builder()`].  The underlying `AVFilterGraph` is
/// initialised lazily on the first push call, deriving format information from
/// the first frame.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::FilterGraph;
///
/// let mut graph = FilterGraph::builder()
///     .scale(1280, 720)
///     .build()?;
///
/// // Push decoded frames in …
/// graph.push_video(0, &video_frame)?;
///
/// // … and pull filtered frames out.
/// while let Some(frame) = graph.pull_video()? {
///     // use frame
/// }
/// ```
pub struct FilterGraph {
    pub(crate) inner: FilterGraphInner,
    pub(crate) output_resolution: Option<(u32, u32)>,
    /// Animation entries registered via `crop_animated` / `gblur_animated`.
    ///
    /// Consumed by per-frame `avfilter_graph_send_command` in #363.
    // TODO(#363): remove allow once consumed by the send_command loop.
    #[allow(dead_code)]
    pub(crate) pending_animations: Vec<AnimationEntry>,
}

impl std::fmt::Debug for FilterGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilterGraph").finish_non_exhaustive()
    }
}

impl FilterGraph {
    /// Create a new builder.
    #[must_use]
    pub fn builder() -> FilterGraphBuilder {
        FilterGraphBuilder::new()
    }

    /// Creates a `FilterGraph` from a pre-built [`FilterGraphInner`].
    ///
    /// Used by [`MultiTrackComposer`](crate::MultiTrackComposer) and
    /// [`MultiTrackAudioMixer`](crate::MultiTrackAudioMixer) to wrap
    /// source-only filter graphs that need no external `buffersrc`.
    pub(crate) fn from_prebuilt(inner: FilterGraphInner) -> Self {
        Self {
            inner,
            output_resolution: None,
            pending_animations: Vec::new(),
        }
    }

    /// Returns the registered animation entries accumulated by
    /// [`crop_animated`](FilterGraphBuilder::crop_animated) and
    /// [`gblur_animated`](FilterGraphBuilder::gblur_animated).
    ///
    /// These are consumed by per-frame `avfilter_graph_send_command` in #363.
    // TODO(#363): remove allow once consumed by the send_command loop.
    #[allow(dead_code)]
    pub(crate) fn pending_animations(&self) -> &[AnimationEntry] {
        &self.pending_animations
    }

    /// Returns the output resolution produced by this graph's `scale` filter step,
    /// if one was configured.
    ///
    /// When multiple `scale` steps are chained, the **last** one's dimensions are
    /// returned. Returns `None` when no `scale` step was added.
    #[must_use]
    pub fn output_resolution(&self) -> Option<(u32, u32)> {
        self.output_resolution
    }

    /// Push a video frame into input slot `slot`.
    ///
    /// On the first call the filter graph is initialised using this frame's
    /// format, resolution, and time base.
    ///
    /// # Errors
    ///
    /// - [`FilterError::InvalidInput`] if `slot` is out of range.
    /// - [`FilterError::BuildFailed`] if the graph cannot be initialised.
    /// - [`FilterError::ProcessFailed`] if the `FFmpeg` push fails.
    pub fn push_video(&mut self, slot: usize, frame: &VideoFrame) -> Result<(), FilterError> {
        self.inner.push_video(slot, frame)
    }

    /// Pull the next filtered video frame, if one is available.
    ///
    /// Returns `None` when the internal `FFmpeg` buffer is empty (EAGAIN) or
    /// at end-of-stream.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::ProcessFailed`] on an unexpected `FFmpeg` error.
    pub fn pull_video(&mut self) -> Result<Option<VideoFrame>, FilterError> {
        self.inner.pull_video()
    }

    /// Push an audio frame into input slot `slot`.
    ///
    /// On the first call the audio filter graph is initialised using this
    /// frame's format, sample rate, and channel count.
    ///
    /// # Errors
    ///
    /// - [`FilterError::InvalidInput`] if `slot` is out of range.
    /// - [`FilterError::BuildFailed`] if the graph cannot be initialised.
    /// - [`FilterError::ProcessFailed`] if the `FFmpeg` push fails.
    pub fn push_audio(&mut self, slot: usize, frame: &AudioFrame) -> Result<(), FilterError> {
        self.inner.push_audio(slot, frame)
    }

    /// Pull the next filtered audio frame, if one is available.
    ///
    /// Returns `None` when the internal `FFmpeg` buffer is empty (EAGAIN) or
    /// at end-of-stream.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::ProcessFailed`] on an unexpected `FFmpeg` error.
    pub fn pull_audio(&mut self) -> Result<Option<AudioFrame>, FilterError> {
        self.inner.pull_audio()
    }
}
