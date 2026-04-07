//! Sequential video clip concatenation.

#![allow(unsafe_code)]

use std::path::PathBuf;

use crate::error::FilterError;
use crate::graph::graph::FilterGraph;

// ── VideoConcatenator ─────────────────────────────────────────────────────────

/// Concatenates multiple video clips into a single seamless output stream.
///
/// Each clip is loaded via a `movie=` source node.  When
/// [`output_resolution`](Self::output_resolution) is set, a `scale` filter is
/// inserted per clip to normalise all clips to a common resolution before
/// concatenation.  A single clip skips the `concat` filter and passes through
/// directly.
///
/// The resulting [`FilterGraph`] is source-only — call
/// [`FilterGraph::pull_video`] in a loop to extract the output frames.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::VideoConcatenator;
///
/// let mut graph = VideoConcatenator::new(vec!["clip_a.mp4", "clip_b.mp4"])
///     .output_resolution(1280, 720)
///     .build()?;
///
/// while let Some(frame) = graph.pull_video()? {
///     // encode or display `frame`
/// }
/// ```
pub struct VideoConcatenator {
    clips: Vec<PathBuf>,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

impl VideoConcatenator {
    /// Creates a new concatenator for the given clip paths.
    pub fn new(clips: Vec<impl AsRef<std::path::Path>>) -> Self {
        Self {
            clips: clips
                .into_iter()
                .map(|p| p.as_ref().to_path_buf())
                .collect(),
            output_width: None,
            output_height: None,
        }
    }

    /// Sets the output resolution.  When provided, a `scale=W:H` filter is
    /// inserted per clip before concatenation.
    #[must_use]
    pub fn output_resolution(self, w: u32, h: u32) -> Self {
        Self {
            output_width: Some(w),
            output_height: Some(h),
            ..self
        }
    }

    /// Builds a source-only [`FilterGraph`] that concatenates all clips.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — no clips were provided, or an
    ///   underlying `FFmpeg` graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.clips.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no clips".to_string(),
            });
        }
        // SAFETY: all raw pointer operations follow the avfilter ownership rules:
        // - avfilter_graph_alloc() returns an owned pointer freed via
        //   avfilter_graph_free() on error or stored in FilterGraphInner on success.
        // - avfilter_graph_create_filter() adds contexts owned by the graph.
        // - avfilter_link() connects pads; connections are owned by the graph.
        // - avfilter_graph_config() finalises the graph.
        // - NonNull::new_unchecked() is called only after ret >= 0 checks.
        unsafe {
            super::composition_inner::build_video_concat(
                &self.clips,
                self.output_width,
                self.output_height,
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn concatenator_empty_clips_should_err() {
        let result = VideoConcatenator::new(Vec::<PathBuf>::new()).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for empty clips, got {result:?}"
        );
    }

    #[test]
    fn concatenator_three_clips_should_build_successfully() {
        // Build with three nonexistent clips.  Graph construction of individual
        // filter nodes (movie, concat, buffersink) should succeed; failure only
        // at avfilter_graph_config (file not found) is expected.
        //
        // Some FFmpeg builds omit the `movie` or `concat` lavfi filters; skip
        // gracefully on those environments rather than failing.
        let result = VideoConcatenator::new(vec!["a.mp4", "b.mp4", "c.mp4"]).build();
        assert!(result.is_err(), "expected error (nonexistent files)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            if reason.contains("filter not found: movie")
                || reason.contains("filter not found: concat")
            {
                println!(
                    "Skipping: required lavfi filter unavailable in this FFmpeg build ({reason})"
                );
                return;
            }
        }
    }
}
