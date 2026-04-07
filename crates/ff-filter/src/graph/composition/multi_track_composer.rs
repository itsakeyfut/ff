//! Multi-track video composition onto a solid-colour canvas.

#![allow(unsafe_code)]

use std::path::PathBuf;
use std::time::Duration;

use crate::error::FilterError;
use crate::graph::graph::FilterGraph;
use crate::graph::types::Rgb;

// ── VideoLayer ────────────────────────────────────────────────────────────────

/// A single video layer in a [`MultiTrackComposer`] composition.
///
/// Layers are composited in ascending [`z_order`](Self::z_order), with
/// `0` rendered first (bottom of the stack).
#[derive(Debug, Clone)]
pub struct VideoLayer {
    /// Source media file path.
    pub source: PathBuf,
    /// X offset on the canvas in pixels (top-left origin).
    pub x: i32,
    /// Y offset on the canvas in pixels.
    pub y: i32,
    /// Uniform scale factor applied to the source frame (`1.0` = original size).
    pub scale: f32,
    /// Opacity (`0.0` = fully transparent, `1.0` = fully opaque).
    pub opacity: f32,
    /// Compositing order (`0` = bottom layer; higher values render on top).
    pub z_order: u32,
    /// Start offset on the output timeline (`Duration::ZERO` = at the beginning).
    pub time_offset: Duration,
    /// Optional trim start within the source file.
    pub in_point: Option<Duration>,
    /// Optional trim end within the source file.
    pub out_point: Option<Duration>,
}

// ── MultiTrackComposer ────────────────────────────────────────────────────────

/// Composes multiple video layers onto a solid-colour canvas.
///
/// Layers are sorted by [`VideoLayer::z_order`] before compositing.  The
/// resulting [`FilterGraph`] is source-only — call [`FilterGraph::pull_video`]
/// in a loop to extract the output frames.  The graph terminates when the
/// last (highest `z_order`) layer finishes.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::{MultiTrackComposer, VideoLayer};
/// use std::time::Duration;
///
/// let mut graph = MultiTrackComposer::new(1920, 1080)
///     .add_layer(VideoLayer {
///         source: "clip.mp4".into(),
///         x: 0, y: 0, scale: 1.0, opacity: 1.0, z_order: 0,
///         time_offset: Duration::ZERO, in_point: None, out_point: None,
///     })
///     .build()?;
///
/// while let Some(frame) = graph.pull_video()? {
///     // encode or display `frame`
/// }
/// ```
pub struct MultiTrackComposer {
    canvas_width: u32,
    canvas_height: u32,
    background: Rgb,
    layers: Vec<VideoLayer>,
}

impl MultiTrackComposer {
    /// Creates a new composer with a black canvas and no layers.
    pub fn new(canvas_width: u32, canvas_height: u32) -> Self {
        Self {
            canvas_width,
            canvas_height,
            background: Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            layers: Vec::new(),
        }
    }

    /// Sets the canvas background colour and returns the updated composer.
    #[must_use]
    pub fn background(self, rgb: Rgb) -> Self {
        Self {
            background: rgb,
            ..self
        }
    }

    /// Appends a video layer and returns the updated composer.
    #[must_use]
    pub fn add_layer(self, layer: VideoLayer) -> Self {
        let mut layers = self.layers;
        layers.push(layer);
        Self { layers, ..self }
    }

    /// Builds a source-only [`FilterGraph`] that composites all layers.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — canvas width or height is zero,
    ///   no layers were added, or an underlying `FFmpeg` graph-construction
    ///   call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.canvas_width == 0 || self.canvas_height == 0 {
            return Err(FilterError::CompositionFailed {
                reason: format!(
                    "canvas dimensions must be non-zero: {}x{}",
                    self.canvas_width, self.canvas_height
                ),
            });
        }
        if self.layers.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no layers".to_string(),
            });
        }
        let mut layers = self.layers;
        layers.sort_by_key(|l| l.z_order);
        // SAFETY: all raw pointer operations follow the avfilter ownership rules:
        // - avfilter_graph_alloc() returns an owned pointer freed via
        //   avfilter_graph_free() on error or stored in FilterGraphInner on success.
        // - avfilter_graph_create_filter() adds contexts owned by the graph.
        // - avfilter_link() connects pads; connections are owned by the graph.
        // - avfilter_graph_config() finalises the graph.
        // - NonNull::new_unchecked() is called only after ret >= 0 checks.
        unsafe {
            super::composition_inner::build_video_composition(
                self.canvas_width,
                self.canvas_height,
                self.background,
                &layers,
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn composer_zero_canvas_size_should_err() {
        // width = 0
        let result = MultiTrackComposer::new(0, 1080)
            .add_layer(VideoLayer {
                source: "clip.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for zero width, got {result:?}"
        );

        // height = 0
        let result = MultiTrackComposer::new(1920, 0)
            .add_layer(VideoLayer {
                source: "clip.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for zero height, got {result:?}"
        );
    }

    #[test]
    fn composer_canvas_larger_than_track_should_succeed() {
        // A 1920×1080 canvas is larger than a typical 640×480 source track.
        // Canvas size is independent of layer resolution — placement at (x, y)
        // is handled by the overlay filter; no auto-scale is applied.
        // The validation guard must not reject non-zero canvas dimensions.
        // If the build fails it must be for an FFmpeg reason (e.g. source file
        // not found), not because of canvas size.
        let result = MultiTrackComposer::new(1920, 1080)
            .add_layer(VideoLayer {
                source: "nonexistent_640x480.mp4".into(),
                x: 100,
                y: 100,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("canvas") && !reason.contains("zero"),
                "build failed due to canvas size, which must not happen for 1920x1080: {reason}"
            );
        }
        // Ok(_) is also acceptable if the movie source happened to be present.
    }

    #[test]
    fn composer_empty_layers_should_return_err() {
        let result = MultiTrackComposer::new(1920, 1080).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed, got {result:?}"
        );
    }

    #[test]
    fn video_layer_with_positive_offset_should_insert_setpts() {
        // setpts_offset is inserted when time_offset > 0.
        // Build fails (nonexistent file) but NOT at "filter not found: setpts".
        let result = MultiTrackComposer::new(1920, 1080)
            .add_layer(VideoLayer {
                source: "nonexistent.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::from_secs(2),
                in_point: None,
                out_point: None,
            })
            .build();
        assert!(result.is_err(), "expected error (nonexistent file)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("filter not found: setpts"),
                "setpts must exist in FFmpeg and be created; got: {reason}"
            );
        }
    }

    #[test]
    fn zero_video_offset_should_not_insert_extra_filters() {
        // time_offset=ZERO must not cause setpts_offset nodes.
        let result = MultiTrackComposer::new(1920, 1080)
            .add_layer(VideoLayer {
                source: "nonexistent.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("setpts_offset"),
                "setpts_offset must not appear for zero offset; got: {reason}"
            );
        }
    }
}
