//! Porter-Duff alpha-compositing operators.

// CompositeOp

/// Porter-Duff compositing operator for combining two video layers.
///
/// # Which path renders which operator
///
/// `Over` and `Under` are built with the `overlay` filter and are genuine alpha
/// compositing on both paths. **`In`, `Out`, `Atop` and `Xor` render on the GPU
/// compositor only**, which implements the W3C / Porter-Duff definitions
/// (#1670). On the filter path they are **refused at build time** with
/// [`FilterError::UnsupportedCompositeOp`](crate::FilterError::UnsupportedCompositeOp)
/// (#1753, ADR-0014): libavfilter has no Porter-Duff filter, `blend`'s
/// `all_expr` can only reference the same plane of both inputs, and the
/// composite chain normalises to `yuv420p`, so the only thing it could compute
/// is per-channel arithmetic wearing the operator's name. A filter-path
/// implementation that carries alpha through the chain is #1784, which lifts
/// the refusal.
///
/// Use [`is_filter_path_supported`](Self::is_filter_path_supported) to ask
/// before building.
///
/// Unlike [`BlendMode`](crate::BlendMode), which is a colour function of two
/// pixels, these are meant to be alpha algebra. There is no `blend all_mode`
/// token for Porter-Duff compositing, so this type has no `FfmpegToken` impl;
/// each operator maps to a specific `FFmpeg` construction (`overlay` or `blend`
/// with a per-channel expression) in the filter graph.
// Open catalog: the Porter-Duff operator set is added to incrementally.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompositeOp {
    /// Top layer rendered over the bottom (standard alpha compositing).
    ///
    /// Built via `overlay=format=auto:shortest=1`.
    #[default]
    Over,

    /// Bottom layer rendered over the top; `Over` with the inputs swapped.
    ///
    /// Built via `overlay` with swapped input order.
    Under,

    /// Top layer masked by the bottom layer's alpha (intersection).
    ///
    /// GPU compositor only; refused on the filter path (see the type docs).
    In,

    /// Top layer visible only where the bottom layer is transparent.
    ///
    /// GPU compositor only; refused on the filter path (see the type docs).
    Out,

    /// Top layer placed atop the bottom; visible only where the bottom is opaque.
    ///
    /// GPU compositor only; refused on the filter path (see the type docs).
    Atop,

    /// Pixels from exactly one layer (XOR of opaque regions).
    ///
    /// GPU compositor only; refused on the filter path (see the type docs).
    Xor,
}

impl CompositeOp {
    /// Whether the filter (CPU) path can build this operator correctly.
    ///
    /// `true` for `Over` and `Under`, which the `overlay` filter implements as
    /// real alpha compositing. `false` for `In`, `Out`, `Atop` and `Xor`, which
    /// the filter path refuses until #1784 carries alpha through the chain; the
    /// GPU compositor renders them. Every filter-path entry point checks this at
    /// build time, so a caller only needs it to decide *before* building, for
    /// example to require a GPU compositor up front.
    #[must_use]
    pub fn is_filter_path_supported(self) -> bool {
        matches!(self, Self::Over | Self::Under)
    }

    /// The per-plane `blend` `all_expr` the filter path *would* use for
    /// `In`/`Out`/`Atop`/`Xor`, or `None` for `Over`/`Under` (built with
    /// `overlay`).
    ///
    /// Kept as the starting point for #1784. It is not reachable from any public
    /// builder today: every entry point refuses these operators first (see
    /// [`is_filter_path_supported`](Self::is_filter_path_supported)), because
    /// applied per colour plane on `yuv420p` this is arithmetic, not alpha
    /// compositing. `A` is the bottom pixel and `B` the top.
    #[must_use]
    pub(crate) fn blend_all_expr(self) -> Option<&'static str> {
        match self {
            Self::Over | Self::Under => None,
            Self::In => Some("B*A/255"),
            Self::Out => Some("B*(255-A)/255"),
            Self::Atop => Some("B*A/255 + A*(255-B)/255"),
            Self::Xor => Some("B*(255-A)/255 + A*(255-B)/255"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CompositeOp;

    #[test]
    fn blend_all_expr_should_be_none_for_overlay_built_operators() {
        assert_eq!(CompositeOp::Over.blend_all_expr(), None);
        assert_eq!(CompositeOp::Under.blend_all_expr(), None);
    }

    #[test]
    fn blend_all_expr_should_return_porter_duff_formula_for_expression_operators() {
        assert_eq!(CompositeOp::In.blend_all_expr(), Some("B*A/255"));
        assert_eq!(CompositeOp::Out.blend_all_expr(), Some("B*(255-A)/255"));
        assert_eq!(
            CompositeOp::Atop.blend_all_expr(),
            Some("B*A/255 + A*(255-B)/255")
        );
        assert_eq!(
            CompositeOp::Xor.blend_all_expr(),
            Some("B*(255-A)/255 + A*(255-B)/255")
        );
    }

    #[test]
    fn composite_op_should_default_to_over() {
        assert_eq!(CompositeOp::default(), CompositeOp::Over);
    }
}
