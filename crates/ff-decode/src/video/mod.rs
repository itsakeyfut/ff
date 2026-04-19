//! Video decoding module.
//!
//! This module provides the video decoder implementation for extracting video
//! frames from media files with hardware acceleration support.

#[cfg(feature = "tokio")]
pub mod async_decoder;
pub mod builder;
pub mod decoder_inner;

#[cfg(feature = "tokio")]
pub use async_decoder::{AsyncVideoDecoder, AsyncVideoDecoderBuilder};
pub use builder::{VideoDecoder, VideoDecoderBuilder};
