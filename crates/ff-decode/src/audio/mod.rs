//! Audio decoding module.
//!
//! This module provides the audio decoder implementation for extracting audio
//! frames from media files.

pub mod builder;
pub mod decoder_inner;

pub use builder::{AudioDecoder, AudioDecoderBuilder, AudioFrameIterator};
