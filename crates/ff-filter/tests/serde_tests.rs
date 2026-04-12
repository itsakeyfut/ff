//! Serialization round-trip tests for animation types.
//!
//! Only compiled / run when the `serde` feature is enabled:
//!   `cargo test -p ff-filter --features serde`

#[cfg(feature = "serde")]
mod serde_tests {
    use std::time::Duration;

    use ff_filter::AnimatedValue;
    use ff_filter::animation::{AnimationTrack, Easing, Keyframe};

    #[test]
    fn animation_track_should_round_trip_through_json() {
        let original: AnimationTrack<f64> = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0_f64, Easing::Linear))
            .push(Keyframe::new(
                Duration::from_secs(2),
                1.0_f64,
                Easing::EaseInOut,
            ));

        let json = serde_json::to_string(&original).expect("serialize failed");
        let restored: AnimationTrack<f64> =
            serde_json::from_str(&json).expect("deserialize failed");

        assert_eq!(original.len(), restored.len());

        let orig_kfs = original.keyframes();
        let rest_kfs = restored.keyframes();

        assert_eq!(orig_kfs[0].timestamp, rest_kfs[0].timestamp);
        assert!(
            (orig_kfs[0].value - rest_kfs[0].value).abs() < f64::EPSILON,
            "first keyframe value mismatch"
        );

        assert_eq!(orig_kfs[1].timestamp, rest_kfs[1].timestamp);
        assert!(
            (orig_kfs[1].value - rest_kfs[1].value).abs() < f64::EPSILON,
            "second keyframe value mismatch"
        );
    }

    #[test]
    fn animated_value_static_should_round_trip_through_json() {
        let original: AnimatedValue<f64> = AnimatedValue::Static(3.14);

        let json = serde_json::to_string(&original).expect("serialize failed");
        let restored: AnimatedValue<f64> = serde_json::from_str(&json).expect("deserialize failed");

        let AnimatedValue::Static(v) = restored else {
            panic!("expected Static variant after round-trip");
        };
        assert!((v - 3.14).abs() < f64::EPSILON, "value mismatch: {v}");
    }

    #[test]
    fn animated_value_track_should_round_trip_through_json() {
        let track = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0_f64, Easing::Hold))
            .push(Keyframe::new(
                Duration::from_millis(500),
                0.5_f64,
                Easing::Linear,
            ));
        let original: AnimatedValue<f64> = AnimatedValue::Track(track);

        let json = serde_json::to_string(&original).expect("serialize failed");
        let restored: AnimatedValue<f64> = serde_json::from_str(&json).expect("deserialize failed");

        let AnimatedValue::Track(t) = restored else {
            panic!("expected Track variant after round-trip");
        };
        assert_eq!(t.len(), 2, "keyframe count mismatch after round-trip");
    }

    #[test]
    fn easing_variants_should_round_trip_through_json() {
        let variants = [
            Easing::Hold,
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
            Easing::Bezier {
                p1: (0.25, 0.1),
                p2: (0.25, 1.0),
            },
        ];

        for easing in &variants {
            let json = serde_json::to_string(easing)
                .unwrap_or_else(|e| panic!("serialize failed for {easing:?}: {e}"));
            let _: Easing = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("deserialize failed for {easing:?}: {e}"));
        }
    }
}
