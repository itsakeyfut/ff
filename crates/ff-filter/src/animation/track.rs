use std::time::Duration;

use super::{Easing, Keyframe, Lerp};

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(bound(
        serialize = "T: serde::Serialize",
        deserialize = "T: serde::Deserialize<'de>",
    ))
)]
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

    /// Returns all keyframes in sorted (ascending-timestamp) order.
    pub fn keyframes(&self) -> &[Keyframe<T>] {
        &self.keyframes
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

impl AnimationTrack<f64> {
    /// Creates a two-keyframe track that ramps linearly (or with `easing`) from
    /// `from` to `to` between `start` and `end`.
    ///
    /// - Before `start`: value is held at `from`.
    /// - Between `start` and `end`: value is interpolated using `easing`.
    /// - After `end`: value is held at `to`.
    ///
    /// This is the common-case shorthand for a volume fade, opacity ramp, or
    /// position sweep.  Equivalent to:
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use ff_filter::animation::{AnimationTrack, Easing, Keyframe};
    /// AnimationTrack::new()
    ///     .push(Keyframe::new(Duration::ZERO, 0.0_f64, Easing::Linear))
    ///     .push(Keyframe::new(Duration::from_secs(2), 1.0_f64, Easing::Linear));
    /// ```
    pub fn fade(from: f64, to: f64, start: Duration, end: Duration, easing: Easing) -> Self {
        Self::new()
            .push(Keyframe::new(start, from, easing))
            .push(Keyframe::new(end, to, Easing::Linear))
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

    #[test]
    fn fade_shorthand_should_produce_linear_ramp() {
        // fade(0.0, 1.0, 0 ms, 2000 ms, Linear) must interpolate linearly.
        let track = AnimationTrack::fade(
            0.0,
            1.0,
            Duration::ZERO,
            Duration::from_secs(2),
            Easing::Linear,
        );

        assert_eq!(track.len(), 2, "fade must produce exactly 2 keyframes");

        let mid = track.value_at(Duration::from_secs(1));
        assert!(
            (mid - 0.5).abs() < 1e-9,
            "expected 0.5 at midpoint (1 s), got {mid}"
        );

        let quarter = track.value_at(Duration::from_millis(500));
        assert!(
            (quarter - 0.25).abs() < 1e-9,
            "expected 0.25 at quarter-point (500 ms), got {quarter}"
        );
    }

    #[test]
    fn fade_shorthand_should_hold_before_start_and_after_end() {
        let track = AnimationTrack::fade(
            10.0,
            20.0,
            Duration::from_secs(1),
            Duration::from_secs(3),
            Easing::Linear,
        );

        // Before start — held at `from`.
        let before = track.value_at(Duration::ZERO);
        assert!(
            (before - 10.0).abs() < f64::EPSILON,
            "expected 10.0 before start, got {before}"
        );
        let at_start = track.value_at(Duration::from_millis(999));
        assert!(
            (at_start - 10.0).abs() < f64::EPSILON,
            "expected 10.0 just before start, got {at_start}"
        );

        // After end — held at `to`.
        let after = track.value_at(Duration::from_secs(3));
        assert!(
            (after - 20.0).abs() < f64::EPSILON,
            "expected 20.0 at end, got {after}"
        );
        let long_after = track.value_at(Duration::from_secs(9999));
        assert!(
            (long_after - 20.0).abs() < f64::EPSILON,
            "expected 20.0 long after end, got {long_after}"
        );
    }
}
