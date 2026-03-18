//! Video decoding module.
//!
//! This module provides the video decoder implementation for extracting video
//! frames from media files with hardware acceleration support.

pub mod builder;
pub mod decoder_inner;

pub use builder::{VideoDecoder, VideoDecoderBuilder};
