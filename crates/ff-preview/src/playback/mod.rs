//! Real-time playback types for ff-preview.
//!
//! This module exposes the primary public API for single-file video/audio
//! playback. All `unsafe` `FFmpeg` calls are isolated in `playback_inner`.
//!
//! | Sub-module         | Contents |
//! |--------------------|---------|
//! | `clock`            | [`PlaybackClock`], internal `MasterClock` |
//! | `sink`             | [`FrameSink`] trait, [`RgbaFrame`], [`RgbaSink`] |
//! | `decode_buffer`    | [`DecodeBuffer`], [`FrameResult`], [`SeekEvent`] |
//! | `player`           | [`PreviewPlayer`] |
//! | `async_player`     | [`AsyncPreviewPlayer`] (tokio feature) |
//! | `playback_inner`   | Unsafe `FFmpeg` calls (`SwsRgbaConverter`, PCM extraction) |

mod playback_inner;

pub(crate) mod clock;
pub(crate) mod decode_buffer;
pub(crate) mod player;
pub(crate) mod sink;

#[cfg(feature = "tokio")]
pub(crate) mod async_player;

pub use clock::PlaybackClock;
pub use decode_buffer::{DecodeBuffer, DecodeBufferBuilder, FrameResult, SeekEvent};
pub use player::PreviewPlayer;
pub use sink::{FrameSink, RgbaFrame, RgbaSink};

#[cfg(feature = "tokio")]
pub use async_player::AsyncPreviewPlayer;
