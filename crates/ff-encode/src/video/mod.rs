//! Video encoding implementation.
//!
//! This module provides video encoding functionality with FFmpeg backend.
//! The implementation is split into public API ([`encoder`]) and internal
//! implementation details ([`encoder_inner`]).

mod encoder;
mod encoder_inner;

pub use encoder::VideoEncoder;
