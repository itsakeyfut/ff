/// A value that supports linear component-wise interpolation.
///
/// `t = 0.0` returns a clone of `a`; `t = 1.0` returns a clone of `b`.
///
/// Implementations for `f64`, `(f64, f64)`, and `(f64, f64, f64)` are added
/// in issue #351.
pub trait Lerp: Clone {
    /// Linearly interpolates between `a` and `b` by the factor `t`.
    fn lerp(a: &Self, b: &Self, t: f64) -> Self;
}
