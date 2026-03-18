//! Common types and traits for the ff-* crate family.
//!
//! This crate provides shared abstractions used across all ff-* crates,
//! particularly for memory management and buffer pooling.

#![warn(missing_docs)]

mod pool;

pub use pool::{FramePool, PooledBuffer, VecPool};
