/// Easing function applied to a keyframe interval.
///
/// Controls the shape of interpolation from one [`super::Keyframe`] to the
/// next.  Each keyframe carries the easing used for the transition *from that
/// keyframe to the subsequent one*; the last keyframe's easing is unused.
///
/// Individual easing functions are implemented across issues #352–#357.
#[derive(Debug, Clone)]
pub enum Easing {
    /// Hold: the value snaps to the next keyframe without interpolation.
    Hold,
    /// Linear: constant-rate interpolation (`y = t`).
    Linear,
    /// Cubic ease-in: slow start, fast end (`y = t³`).
    EaseIn,
    /// Cubic ease-out: fast start, slow end (`y = 1 − (1−t)³`).
    EaseOut,
    /// Cubic ease-in-out: slow at both ends, fast middle (`y = 3t² − 2t³`).
    EaseInOut,
    /// CSS-compatible cubic Bézier with two user-defined control points.
    ///
    /// P0 = (0, 0) and P3 = (1, 1) are fixed; `p1` and `p2` define the curve
    /// shape.  Equivalent to the CSS `cubic-bezier()` function.
    Bezier {
        /// First control point `(x, y)`, x clamped to `[0, 1]`.
        p1: (f64, f64),
        /// Second control point `(x, y)`, x clamped to `[0, 1]`.
        p2: (f64, f64),
    },
}

impl Easing {
    /// Applies the easing function to a normalised progress value `t ∈ [0, 1]`.
    ///
    /// Returns a remapped progress value `u ∈ [0, 1]` that is then used to
    /// drive `T::lerp`.  Full per-variant implementations are added in issues
    /// #352–#357; variants not yet implemented fall back to linear.
    pub(crate) fn apply(&self, t: f64) -> f64 {
        match self {
            Easing::Hold => 0.0,
            Easing::Linear => t,
            // Full cubic implementations added in #352–#357.
            Easing::EaseIn => t,
            Easing::EaseOut => t,
            Easing::EaseInOut => t,
            Easing::Bezier { .. } => t,
        }
    }
}
