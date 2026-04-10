use std::time::Duration;

use super::{Keyframe, Lerp};

/// A sorted collection of keyframes with interpolated `value_at(t)` lookup.
///
/// Keyframes are kept in ascending timestamp order at all times.  The easing
/// used for each interval is taken from the **preceding** keyframe's `easing`
/// field; the last keyframe's easing is never read.
///
/// # Panics
///
/// `value_at` panics if the track is empty.  Always push at least one keyframe
/// before querying.
#[derive(Debug, Clone)]
pub struct AnimationTrack<T: Lerp> {
    keyframes: Vec<Keyframe<T>>,
}

impl<T: Lerp> AnimationTrack<T> {
    /// Creates an empty track.
    pub fn new() -> Self {
        Self {
            keyframes: Vec::new(),
        }
    }

    /// Inserts a keyframe, maintaining timestamp-sorted order.
    ///
    /// If a keyframe at the same timestamp already exists it is replaced.
    #[must_use]
    pub fn push(mut self, kf: Keyframe<T>) -> Self {
        let pos = self
            .keyframes
            .partition_point(|k| k.timestamp < kf.timestamp);
        if self
            .keyframes
            .get(pos)
            .is_some_and(|k| k.timestamp == kf.timestamp)
        {
            self.keyframes[pos] = kf;
        } else {
            self.keyframes.insert(pos, kf);
        }
        self
    }

    /// Returns the interpolated value at time `t`.
    ///
    /// - Before the first keyframe: returns the first value (hold).
    /// - After the last keyframe: returns the last value (hold).
    /// - Between two keyframes: uses the preceding keyframe's `easing`.
    ///
    /// # Panics
    ///
    /// Panics if the track is empty.
    pub fn value_at(&self, t: Duration) -> T {
        let len = self.keyframes.len();
        // pos = number of keyframes with timestamp < t
        let pos = self.keyframes.partition_point(|k| k.timestamp <= t);

        if pos == 0 {
            // Before or exactly at the first keyframe.
            return self.keyframes[0].value.clone();
        }
        if pos >= len {
            // After or exactly at the last keyframe.
            return self.keyframes[len - 1].value.clone();
        }

        let a = &self.keyframes[pos - 1];
        let b = &self.keyframes[pos];

        let span = b
            .timestamp
            .checked_sub(a.timestamp)
            .map_or(0.0, |d| d.as_secs_f64());
        let elapsed = t.checked_sub(a.timestamp).map_or(0.0, |d| d.as_secs_f64());
        let norm_t = if span > 0.0 { elapsed / span } else { 1.0 };

        let u = a.easing.apply(norm_t);
        T::lerp(&a.value, &b.value, u)
    }

    /// Returns the number of keyframes in the track.
    pub fn len(&self) -> usize {
        self.keyframes.len()
    }

    /// Returns `true` if the track has no keyframes.
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }
}

impl<T: Lerp> Default for AnimationTrack<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Easing;

    fn kf(ms: u64, v: f64) -> Keyframe<f64> {
        Keyframe::new(Duration::from_millis(ms), v, Easing::Linear)
    }

    #[test]
    fn animation_track_should_return_first_value_before_first_keyframe() {
        let track = AnimationTrack::new()
            .push(kf(500, 10.0))
            .push(kf(1000, 20.0));

        let v = track.value_at(Duration::from_millis(0));
        assert!((v - 10.0).abs() < f64::EPSILON, "expected 10.0, got {v}");

        let v2 = track.value_at(Duration::from_millis(499));
        assert!((v2 - 10.0).abs() < f64::EPSILON, "expected 10.0, got {v2}");
    }

    #[test]
    fn animation_track_should_return_last_value_after_last_keyframe() {
        let track = AnimationTrack::new().push(kf(0, 0.0)).push(kf(1000, 50.0));

        let v = track.value_at(Duration::from_millis(1000));
        assert!((v - 50.0).abs() < f64::EPSILON, "expected 50.0, got {v}");

        let v2 = track.value_at(Duration::from_millis(9999));
        assert!((v2 - 50.0).abs() < f64::EPSILON, "expected 50.0, got {v2}");
    }

    #[test]
    fn animation_track_should_interpolate_between_keyframes() {
        // 0 ms → 0.0, 1000 ms → 1.0, linear easing.
        let track = AnimationTrack::new().push(kf(0, 0.0)).push(kf(1000, 1.0));

        let v = track.value_at(Duration::from_millis(500));
        assert!((v - 0.5).abs() < 1e-9, "expected 0.5 at midpoint, got {v}");

        let v2 = track.value_at(Duration::from_millis(250));
        assert!(
            (v2 - 0.25).abs() < 1e-9,
            "expected 0.25 at quarter-point, got {v2}"
        );
    }
}
