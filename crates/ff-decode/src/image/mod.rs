//! Image decoding module.
//!
//! This module provides the image decoder implementation for decoding still
//! images (JPEG, PNG, BMP, TIFF, WebP) into [`VideoFrame`](ff_format::VideoFrame)s.

#[cfg(feature = "tokio")]
pub mod async_decoder;
pub mod builder;
pub mod decoder_inner;

#[cfg(feature = "tokio")]
pub use async_decoder::AsyncImageDecoder;
pub use builder::{ImageDecoder, ImageDecoderBuilder};
