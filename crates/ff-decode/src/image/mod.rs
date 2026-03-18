//! Image decoding module.
//!
//! This module provides the image decoder implementation for decoding still
//! images (JPEG, PNG, BMP, TIFF, WebP) into [`VideoFrame`](ff_format::VideoFrame)s.

pub mod builder;
pub mod decoder_inner;

pub use builder::{ImageDecoder, ImageDecoderBuilder};
