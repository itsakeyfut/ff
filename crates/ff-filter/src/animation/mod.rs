//! Keyframe animation system for time-varying filter parameters.
//!
//! The animation system is built in layers:
//!
//! 1. [`Lerp`] — trait for component-wise linear interpolation (#351)
//! 2. [`Easing`] — six easing functions: `Hold`, `Linear`, `EaseIn`, `EaseOut`,
//!    `EaseInOut`, `Bezier` (#352–#357)
//! 3. [`Keyframe<T>`] — timestamp + value + per-segment easing (#349)
//! 4. [`AnimationTrack<T>`] — sorted collection with `value_at(t)` (#350)
//! 5. [`AnimatedValue<T>`] — `Static(T)` or `Track(AnimationTrack<T>)` (#358)
//! 6. [`AnimationEntry`] — registered animation track for a specific filter parameter (#359)

mod easing;
mod keyframe;
mod lerp;
mod track;
mod value;

pub use easing::Easing;
pub use keyframe::Keyframe;
pub use lerp::Lerp;
pub use track::AnimationTrack;
pub use value::AnimatedValue;

/// A registered animation track for a specific filter parameter.
///
/// Accumulated in [`crate::FilterGraphBuilder`] and transferred to
/// [`crate::FilterGraph`] on [`build()`](crate::FilterGraphBuilder::build).
/// Per-frame `avfilter_graph_send_command` updates are applied during playback
/// in issue #363.
#[derive(Debug, Clone)]
pub struct AnimationEntry {
    /// `FFmpeg` filter node name, e.g. `"crop_0"` or `"gblur_0"`.
    pub node_name: String,
    /// `FFmpeg` `send_command` parameter name, e.g. `"w"`, `"h"`, `"x"`, `"y"`,
    /// or `"sigma"`.
    pub param: &'static str,
    /// The animation track providing the value over time.
    pub track: AnimationTrack<f64>,
    /// Optional suffix appended to the formatted value before sending, e.g.
    /// `"dB"` for the `volume` filter.  Use `""` for dimensionless parameters.
    pub suffix: &'static str,
}
