//! Video, audio, and subtitle stream information.
//!
//! This module provides structs for representing metadata about the
//! streams within media files.
//!
//! # Examples
//!
//! ```
//! use ff_format::stream::{VideoStreamInfo, AudioStreamInfo};
//! use ff_format::{PixelFormat, SampleFormat, Rational};
//! use ff_format::codec::{VideoCodec, AudioCodec};
//! use ff_format::color::{ColorSpace, ColorRange, ColorPrimaries};
//! use ff_format::channel::ChannelLayout;
//! use std::time::Duration;
//!
//! // Create video stream info
//! let video = VideoStreamInfo::builder()
//!     .index(0)
//!     .codec(VideoCodec::H264)
//!     .width(1920)
//!     .height(1080)
//!     .frame_rate(Rational::new(30, 1))
//!     .pixel_format(PixelFormat::Yuv420p)
//!     .build();
//!
//! assert_eq!(video.width(), 1920);
//! assert_eq!(video.height(), 1080);
//!
//! // Create audio stream info
//! let audio = AudioStreamInfo::builder()
//!     .index(1)
//!     .codec(AudioCodec::Aac)
//!     .sample_rate(48000)
//!     .channels(2)
//!     .sample_format(SampleFormat::F32)
//!     .build();
//!
//! assert_eq!(audio.sample_rate(), 48000);
//! assert_eq!(audio.channels(), 2);
//! ```

mod audio;
mod subtitle;
mod video;

pub use audio::{AudioStreamInfo, AudioStreamInfoBuilder};
pub use subtitle::{SubtitleStreamInfo, SubtitleStreamInfoBuilder};
pub use video::{VideoStreamInfo, VideoStreamInfoBuilder};
