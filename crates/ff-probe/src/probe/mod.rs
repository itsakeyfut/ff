//! Probe module — media file metadata extraction.

mod builder;
pub(crate) mod probe_inner;

pub use builder::open;
