//! Video effects — both whole-file and frame-level.
//!
//! - [`Stabilizer`] — two-pass video stabilization via `vidstabdetect` /
//!   `vidstabtransform` (whole-file).
//! - [`FilterGraph::motion_blur`](crate::FilterGraph::motion_blur) — shutter-angle
//!   motion blur via `tblend` (frame-level, extends [`crate::FilterGraph`]).

pub(crate) mod effects_inner;
mod stabilizer;
mod video_effects;

pub use stabilizer::{AnalyzeOptions, Interpolation, StabilizeOptions, Stabilizer};
