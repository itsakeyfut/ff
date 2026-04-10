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
            // Hold: snap — stay at the start value until t reaches 1.0,
            // then jump to the end value.
            Easing::Hold => {
                if t >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
            Easing::Linear => t,
            // Cubic ease-in: slow start, fast end (y = t³).
            Easing::EaseIn => t * t * t,
            // Cubic ease-out: fast start, slow end (y = 1 − (1−t)³).
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            // Cubic ease-in-out: slow at both ends, fast middle (y = 3t² − 2t³).
            // Equivalent to Ken Perlin's smoothstep; symmetric about t = 0.5.
            Easing::EaseInOut => 3.0 * t * t - 2.0 * t * t * t,
            // CSS cubic-bezier: find t via Newton–Raphson, return By(t).
            // P0=(0,0) and P3=(1,1) are fixed; P1=p1, P2=p2.
            // P1.x and P2.x are clamped to [0, 1] to preserve monotonicity.
            Easing::Bezier { p1, p2 } => {
                let p1x = p1.0.clamp(0.0, 1.0);
                let p2x = p2.0.clamp(0.0, 1.0);

                // Solve Bx(nt) = t via Newton–Raphson (4 iterations).
                let mut nt = t;
                for _ in 0..4 {
                    let bx_prime = bez_x_prime(nt, p1x, p2x);
                    if bx_prime.abs() < 1e-10 {
                        break;
                    }
                    nt -= (bez_x(nt, p1x, p2x) - t) / bx_prime;
                    nt = nt.clamp(0.0, 1.0);
                }

                bez_y(nt, p1.1, p2.1)
            }
        }
    }
}

// ── Cubic Bézier helpers (P0=0, P3=1) ────────────────────────────────────────

/// X position on the Bézier curve at parameter `t`.
fn bez_x(t: f64, p1x: f64, p2x: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * p1x * t * u * u + 3.0 * p2x * t * t * u + t * t * t
}

/// Derivative of `bez_x` with respect to `t`.
fn bez_x_prime(t: f64, p1x: f64, p2x: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * p1x * u * u + 6.0 * (p2x - p1x) * t * u + 3.0 * (1.0 - p2x) * t * t
}

/// Y position on the Bézier curve at parameter `t`.
fn bez_y(t: f64, p1y: f64, p2y: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * p1y * t * u * u + 3.0 * p2y * t * t * u + t * t * t
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::animation::{AnimationTrack, Keyframe};

    #[test]
    fn bezier_ease_css_preset_should_match_reference_values() {
        // CSS `ease` = cubic-bezier(0.25, 0.1, 0.25, 1.0).
        // At x=0.5 the CSS reference value is ~0.8029 (t ≈ 0.7 solves Bx(t)=0.5).
        let ease = Easing::Bezier {
            p1: (0.25, 0.1),
            p2: (0.25, 1.0),
        };
        let v = ease.apply(0.5);
        assert!(
            (v - 0.8029_f64).abs() < 0.01,
            "expected ~0.8029 for CSS ease at x=0.5, got {v}"
        );

        // Boundary conditions: apply(0.0) = 0.0 and apply(1.0) = 1.0.
        assert!(
            ease.apply(0.0).abs() < f64::EPSILON,
            "apply(0.0) must be 0.0"
        );
        assert!(
            (ease.apply(1.0) - 1.0).abs() < f64::EPSILON,
            "apply(1.0) must be 1.0"
        );
    }

    #[test]
    fn linear_easing_should_return_half_at_midpoint() {
        // Build a [0 s → 0.0, 1 s → 1.0] track with Linear easing.
        let track = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0_f64, Easing::Linear))
            .push(Keyframe::new(
                Duration::from_secs(1),
                1.0_f64,
                Easing::Linear,
            ));

        let v = track.value_at(Duration::from_millis(500));
        assert!((v - 0.5).abs() < 0.001, "expected 0.5 at midpoint, got {v}");
    }

    #[test]
    fn ease_in_out_should_return_half_at_midpoint() {
        // 3(0.5)² − 2(0.5)³ = 0.75 − 0.25 = 0.5 exactly.
        let u = Easing::EaseInOut.apply(0.5);
        assert!((u - 0.5).abs() < 0.001, "expected 0.5 at midpoint, got {u}");
    }

    #[test]
    fn ease_in_out_should_be_below_linear_at_quarter() {
        // Slow start: eased value at t=0.1 should be below 0.1.
        let u = Easing::EaseInOut.apply(0.1);
        assert!(u < 0.1, "ease-in-out at t=0.1 should be below 0.1, got {u}");
    }

    #[test]
    fn ease_in_out_should_be_above_linear_at_three_quarters() {
        // Slow end: eased value at t=0.9 should be above 0.9.
        let u = Easing::EaseInOut.apply(0.9);
        assert!(u > 0.9, "ease-in-out at t=0.9 should be above 0.9, got {u}");
    }

    #[test]
    fn ease_out_should_be_above_linear_at_midpoint() {
        // 1 − (1−0.5)³ = 1 − 0.125 = 0.875, well above the linear 0.5.
        let u = Easing::EaseOut.apply(0.5);
        assert!(u > 0.5, "ease-out at t=0.5 should be above 0.5, got {u}");
        assert!((u - 0.875).abs() < f64::EPSILON, "expected 0.875, got {u}");
    }

    #[test]
    fn ease_in_should_be_below_linear_at_midpoint() {
        // t³ at t=0.5 → 0.125, well below the linear 0.5.
        let u = Easing::EaseIn.apply(0.5);
        assert!(u < 0.5, "ease-in at t=0.5 should be below 0.5, got {u}");
        assert!((u - 0.125).abs() < f64::EPSILON, "expected 0.125, got {u}");
    }

    #[test]
    fn hold_easing_should_return_start_value_at_midpoint() {
        // t = 0.5: still holding at the start — must return 0.0.
        let u = Easing::Hold.apply(0.5);
        assert!(
            (u - 0.0).abs() < f64::EPSILON,
            "expected 0.0 at t=0.5, got {u}"
        );
    }

    #[test]
    fn hold_easing_should_snap_at_keyframe_boundary() {
        // t = 1.0: exactly at the next keyframe — must snap to 1.0.
        let u = Easing::Hold.apply(1.0);
        assert!(
            (u - 1.0).abs() < f64::EPSILON,
            "expected 1.0 at t=1.0, got {u}"
        );

        // t slightly above 1.0 also returns 1.0.
        let u2 = Easing::Hold.apply(1.5);
        assert!(
            (u2 - 1.0).abs() < f64::EPSILON,
            "expected 1.0 at t=1.5, got {u2}"
        );
    }
}
