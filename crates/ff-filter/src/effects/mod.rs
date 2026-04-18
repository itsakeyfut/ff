//! Video effects — both whole-file and frame-level.
//!
//! - [`Stabilizer`] — two-pass video stabilization via `vidstabdetect` /
//!   `vidstabtransform` (whole-file).
//! - [`FilterGraph::motion_blur`](crate::FilterGraph::motion_blur) — shutter-angle
//!   motion blur via `tblend` (frame-level, extends [`crate::FilterGraph`]).
//! - [`LensProfile`] — predefined lens distortion correction profiles for common cameras.

mod audio_effects;
pub(crate) mod effects_inner;
pub mod lens_profile;
mod stabilizer;
mod video_effects;

pub use lens_profile::LensProfile;
pub use stabilizer::{AnalyzeOptions, Interpolation, StabilizeOptions, Stabilizer};
