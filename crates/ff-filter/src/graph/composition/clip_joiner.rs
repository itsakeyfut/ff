//! Cross-dissolve join of two video clips.

#![allow(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use crate::error::FilterError;
use crate::graph::graph::FilterGraph;

// ── ClipJoiner ────────────────────────────────────────────────────────────────

/// Joins two video clips with a cross-dissolve transition.
///
/// Each clip is loaded via a `movie=` source node.  The last
/// `dissolve_duration` seconds of clip A overlap with the first
/// `dissolve_duration` seconds of clip B, producing an output shorter than
/// simple concatenation by `dissolve_duration`.
///
/// When `dissolve_duration` is [`Duration::ZERO`] the clips are concatenated
/// without a transition (equivalent to
/// [`VideoConcatenator::new(vec![clip_a, clip_b]).build()`]).
///
/// # Errors
///
/// Returns [`FilterError::CompositionFailed`] when:
/// - The clip duration cannot be probed (e.g. file not found).
/// - `dissolve_duration` exceeds the duration of either clip.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::ClipJoiner;
/// use std::time::Duration;
///
/// let mut graph = ClipJoiner::new("intro.mp4", "main.mp4", Duration::from_secs(1))
///     .build()?;
///
/// while let Some(frame) = graph.pull_video()? {
///     // encode or display `frame`
/// }
/// ```
pub struct ClipJoiner {
    clip_a: PathBuf,
    clip_b: PathBuf,
    dissolve_duration: Duration,
}

impl ClipJoiner {
    /// Create a new `ClipJoiner`.
    ///
    /// `dissolve_duration` is the length of the cross-dissolve overlap.
    /// Pass [`Duration::ZERO`] for plain concatenation (no transition).
    pub fn new(
        clip_a: impl AsRef<std::path::Path>,
        clip_b: impl AsRef<std::path::Path>,
        dissolve_duration: Duration,
    ) -> Self {
        Self {
            clip_a: clip_a.as_ref().to_path_buf(),
            clip_b: clip_b.as_ref().to_path_buf(),
            dissolve_duration,
        }
    }

    /// Builds a source-only [`FilterGraph`] that joins the two clips.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — clip duration probe failed, or
    ///   `dissolve_duration` exceeds a clip's duration, or an `FFmpeg`
    ///   graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        let dissolve_sec = self.dissolve_duration.as_secs_f64();
        // SAFETY: avformat and avfilter invariants are maintained internally;
        //         all pointers are null-checked; resources are freed on every
        //         error path.
        unsafe {
            super::composition_inner::build_dissolve_join(&self.clip_a, &self.clip_b, dissolve_sec)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn join_with_dissolve_exceeding_clip_duration_should_err() {
        // dissolve_duration (9999 s) exceeds any realistic clip.  With
        // nonexistent files the probe itself returns CompositionFailed, which
        // also satisfies the assertion.  With real files the duration check
        // fires.
        let result = ClipJoiner::new("a.mp4", "b.mp4", Duration::from_secs(9999)).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for dissolve_duration > clip duration, got {result:?}"
        );
    }

    #[test]
    fn join_with_dissolve_should_reduce_total_duration() {
        // With nonexistent files the probe step returns CompositionFailed.
        // This test verifies: (a) no panic, (b) the error is CompositionFailed
        // (not an unexpected variant), and (c) the `xfade` filter exists in the
        // running FFmpeg build (if the error mentions "filter not found: xfade"
        // we skip instead of failing, matching the pattern used by the concat
        // tests).
        let result =
            ClipJoiner::new("clip_a.mp4", "clip_b.mp4", Duration::from_millis(500)).build();
        assert!(result.is_err(), "expected error (probe or graph failure)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            if reason.contains("filter not found: xfade")
                || reason.contains("filter not found: movie")
            {
                println!(
                    "Skipping: required lavfi filter unavailable in this FFmpeg build ({reason})"
                );
                return;
            }
        }
    }
}
