//! Video and audio frame types.
//!
//! This module provides [`VideoFrame`] and [`AudioFrame`] structures
//! for working with decoded media frames. These types abstract away
//! `FFmpeg`'s internal frame structures and provide a safe, Rust-idiomatic API.
//!
//! # Examples
//!
//! ## Video Frame
//!
//! ```
//! use ff_format::{PixelFormat, PooledBuffer, Rational, Timestamp, VideoFrame};
//!
//! // Create a simple 1920x1080 RGBA frame
//! let width = 1920u32;
//! let height = 1080u32;
//! let bytes_per_pixel = 4; // RGBA
//! let stride = width as usize * bytes_per_pixel;
//! let data = vec![0u8; stride * height as usize];
//!
//! let frame = VideoFrame::new(
//!     vec![PooledBuffer::standalone(data)],
//!     vec![stride],
//!     width,
//!     height,
//!     PixelFormat::Rgba,
//!     Timestamp::default(),
//!     true,
//! ).unwrap();
//!
//! assert_eq!(frame.width(), 1920);
//! assert_eq!(frame.height(), 1080);
//! assert!(frame.is_key_frame());
//! assert_eq!(frame.num_planes(), 1);
//! ```
//!
//! ## Audio Frame
//!
//! ```
//! use ff_format::{AudioFrame, SampleFormat, Rational, Timestamp};
//!
//! // Create a stereo F32 audio frame with 1024 samples
//! let channels = 2u32;
//! let samples = 1024usize;
//! let sample_rate = 48000u32;
//!
//! let frame = AudioFrame::empty(
//!     samples,
//!     channels,
//!     sample_rate,
//!     SampleFormat::F32,
//! ).unwrap();
//!
//! assert_eq!(frame.samples(), 1024);
//! assert_eq!(frame.channels(), 2);
//! assert_eq!(frame.sample_rate(), 48000);
//! assert!(!frame.format().is_planar());
//! ```

mod audio;
mod video;

pub use audio::AudioFrame;
pub use video::VideoFrame;
