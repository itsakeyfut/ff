//! Audio decoding module.
//!
//! This module provides the audio decoder implementation for extracting audio
//! frames from media files.

#[cfg(feature = "tokio")]
pub mod async_decoder;
pub mod builder;
pub mod decoder_inner;
pub(crate) mod resample_inner;

#[cfg(feature = "tokio")]
pub use async_decoder::AsyncAudioDecoder;
pub use builder::{AudioDecoder, AudioDecoderBuilder};
