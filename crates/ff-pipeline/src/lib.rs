//! Unified decode-filter-encode pipeline for the ff-* crate family.
//!
//! # Status
//!
//! **This crate is not yet implemented.** It is a placeholder for future development.
//!
//! # Design Principles
//!
//! All public APIs in this crate are **safe**. Users never need to write `unsafe` code.
//! Unsafe `FFmpeg` internals are encapsulated within the underlying ff-decode, ff-filter,
//! and ff-encode crates.
//!
//! # Planned Features
//!
//! - Unified `Pipeline` type connecting decode → filter → encode in a single call
//! - Progress tracking via callback (`on_progress`)
//! - Cancellation support
//! - Multi-input concatenation
//! - Parallel thumbnail generation via optional `rayon` feature

// This crate is a placeholder. No public API is available yet.
