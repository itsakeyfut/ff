//! Keyframe animation system for time-varying filter parameters.
//!
//! The animation system is built in layers:
//!
//! 1. [`Lerp`] — trait for component-wise linear interpolation (#351)
//! 2. [`Easing`] — six easing functions: `Hold`, `Linear`, `EaseIn`, `EaseOut`,
//!    `EaseInOut`, `Bezier` (#352–#357)
//! 3. [`Keyframe<T>`] — timestamp + value + per-segment easing (#349)
//! 4. [`AnimationTrack<T>`] — sorted collection with `value_at(t)` (#350)

mod easing;
mod keyframe;
mod lerp;
mod track;

pub use easing::Easing;
pub use keyframe::Keyframe;
pub use lerp::Lerp;
pub use track::AnimationTrack;
