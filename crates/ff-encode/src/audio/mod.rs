//! Audio encoding implementation.
//!
//! This module provides audio encoding functionality with an FFmpeg backend.
//! The implementation is split into public API ([`builder`]) and internal
//! implementation details ([`encoder_inner`]).

pub mod builder;
mod encoder_inner;

pub use builder::{AudioEncoder, AudioEncoderBuilder};
