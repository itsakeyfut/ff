//! Audio encoding implementation.
//!
//! This module provides audio encoding functionality with FFmpeg backend.
//! The implementation is split into public API ([`encoder`]) and internal
//! implementation details ([`encoder_inner`]).

mod encoder;
mod encoder_inner;

pub use encoder::AudioEncoder;
