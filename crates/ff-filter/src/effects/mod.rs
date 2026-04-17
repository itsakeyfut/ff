//! Whole-file video effects (not frame-by-frame filter graphs).
//!
//! Currently provides:
//! - [`Stabilizer`] — two-pass video stabilization via `vidstabdetect` /
//!   `vidstabtransform`.

pub(crate) mod effects_inner;
mod stabilizer;

pub use stabilizer::{AnalyzeOptions, Stabilizer};
