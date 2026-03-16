//! # ff-pipeline
//!
//! Unified decode → filter → encode pipeline for the ff-* crate family.
//!
//! This crate wires together `ff-decode`, `ff-filter`, and `ff-encode` into a
//! single high-level API. All `unsafe` `FFmpeg` internals remain encapsulated in
//! the underlying crates; users of `ff-pipeline` never need to write `unsafe` code.
//!
//! ## Features
//!
//! - **Single-call transcode**: open → filter → encode in one `Pipeline::run()`
//! - **Progress callbacks**: real-time frame-count and elapsed-time updates
//! - **Cancellation**: returning `false` from the progress callback aborts the pipeline
//! - **Multi-input concatenation**: pass multiple input paths to concatenate clips
//!
//! ## Usage
//!
//! ```ignore
//! use ff_pipeline::{Pipeline, EncoderConfig};
//! use ff_encode::{VideoCodec, AudioCodec, BitrateMode};
//!
//! let config = EncoderConfig {
//!     video_codec:  VideoCodec::H264,
//!     audio_codec:  AudioCodec::Aac,
//!     bitrate_mode: BitrateMode::Cbr(4_000_000),
//!     resolution:   Some((1280, 720)),
//!     framerate:    Some(30.0),
//!     hardware:     None,
//! };
//!
//! Pipeline::builder()
//!     .input("input.mp4")
//!     .output("output.mp4", config)
//!     .on_progress(|p| {
//!         println!("frame={} elapsed={:?}", p.frames_processed, p.elapsed);
//!         true // return false to cancel
//!     })
//!     .build()?
//!     .run()?;
//! ```
//!
//! ## Module Structure
//!
//! - [`error`] — [`PipelineError`]
//! - [`pipeline`] — [`Pipeline`], [`PipelineBuilder`], [`EncoderConfig`]
//! - [`progress`] — [`Progress`], [`ProgressCallback`]
//! - [`thumbnail`] — [`ThumbnailPipeline`]

#![warn(missing_docs)]

pub mod error;
pub mod pipeline;
pub mod progress;
pub mod thumbnail;

pub use error::PipelineError;
pub use pipeline::{EncoderConfig, Pipeline, PipelineBuilder};
pub use progress::{Progress, ProgressCallback};
pub use thumbnail::ThumbnailPipeline;
