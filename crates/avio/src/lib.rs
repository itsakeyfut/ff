//! Safe, high-level audio/video/image processing for Rust.
//!
//! `avio` is the facade crate for the ff-* crate family — a backend-agnostic
//! multimedia toolkit. It re-exports the public APIs of all member crates behind
//! feature flags, so users can depend on a single crate and opt into only the
//! functionality they need.
//!
//! # Status
//!
//! **This crate is not yet fully implemented.** It is a placeholder for future development.
//! The core crates (`ff-probe`, `ff-decode`, `ff-encode`) are under active development.
//!
//! # Feature Flags
//!
//! | Feature    | Crate         | Default |
//! |------------|---------------|---------|
//! | `probe`    | `ff-probe`    | yes     |
//! | `decode`   | `ff-decode`   | yes     |
//! | `encode`   | `ff-encode`   | yes     |
//! | `filter`   | `ff-filter`   | no      |
//! | `pipeline` | `ff-pipeline` | no      |
//! | `stream`   | `ff-stream`   | no      |
//!
//! # Planned Usage
//!
//! ```toml
//! [dependencies]
//! avio = { version = "0.5", features = ["filter", "pipeline", "stream"] }
//! ```

// This crate is a placeholder. No public API is available yet.
