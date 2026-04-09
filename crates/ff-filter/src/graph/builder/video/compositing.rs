//! Compositing and blend filter methods for [`FilterGraphBuilder`].

#[allow(clippy::wildcard_imports)]
use super::*;

impl FilterGraphBuilder {
    /// Blend a `top` layer over `self` (the bottom) using the given [`BlendMode`]
    /// and `opacity`.
    ///
    /// `opacity` is clamped to `[0.0, 1.0]` before being stored.
    ///
    /// # Normal mode
    ///
    /// The bottom stream is `self`; the top stream is pushed on input slot 1.
    /// When `opacity == 1.0` the filter chain is:
    /// ```text
    /// [bottom][top]overlay=format=auto:shortest=1[out]
    /// ```
    /// When `opacity < 1.0` a `colorchannelmixer=aa=<opacity>` step is applied
    /// to the top stream first:
    /// ```text
    /// [top]colorchannelmixer=aa=<opacity>[top_faded];
    /// [bottom][top_faded]overlay=format=auto:shortest=1[out]
    /// ```
    ///
    /// # Unimplemented modes
    ///
    /// All modes other than [`BlendMode::Normal`] are defined but not yet
    /// implemented.  Calling [`build`](FilterGraphBuilder::build) with an
    /// unimplemented mode returns
    /// [`FilterError::InvalidConfig`].
    #[must_use]
    pub fn blend(mut self, top: FilterGraphBuilder, mode: BlendMode, opacity: f32) -> Self {
        let opacity = opacity.clamp(0.0, 1.0);
        self.steps.push(FilterStep::Blend {
            top: Box::new(top),
            mode,
            opacity,
        });
        self
    }

    /// Key out pixels matching `color` using `FFmpeg`'s `chromakey` filter.
    ///
    /// - `color`: `FFmpeg` color string, e.g. `"green"`, `"0x00FF00"`, `"#00FF00"`.
    /// - `similarity`: match radius in `[0.0, 1.0]`; higher = more pixels removed.
    /// - `blend`: edge softness in `[0.0, 1.0]`; `0.0` = hard edge.
    ///
    /// `similarity` and `blend` are validated in [`build`](FilterGraphBuilder::build);
    /// out-of-range values return [`FilterError::InvalidConfig`].
    ///
    /// The output pixel format is `yuva420p` (adds an alpha channel).
    /// Use this for YCbCr-encoded sources (most video).
    #[must_use]
    pub fn chromakey(mut self, color: &str, similarity: f32, blend: f32) -> Self {
        self.steps.push(FilterStep::ChromaKey {
            color: color.to_string(),
            similarity,
            blend,
        });
        self
    }
}
