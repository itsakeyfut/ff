//! HLS and DASH adaptive streaming output for the ff-* crate family.
//!
//! # Status
//!
//! **This crate is not yet implemented.** It is a placeholder for future development.
//!
//! # Design Principles
//!
//! All public APIs in this crate are **safe**. Users never need to write `unsafe` code.
//! Unsafe `FFmpeg` internals are encapsulated within the underlying ff-encode crate.
//!
//! # Planned Features
//!
//! - HLS output via `HlsOutput` with configurable segment duration
//! - DASH output via `DashOutput`
//! - ABR ladder generation via `AbrLadder` (multiple renditions in one pass)
//! - Keyframe interval control for optimal segment boundaries

// This crate is a placeholder. No public API is available yet.
