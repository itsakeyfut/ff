//! Loudness, audio analysis, and video quality metric tools for media files.
//!
//! This module provides:
//! - [`LoudnessMeter`] for EBU R128 integrated loudness, loudness range, and
//!   true peak measurement.
//! - [`QualityMetrics`] for computing video quality metrics (SSIM, PSNR) between
//!   a reference and a distorted video.
//!
//! All `unsafe` `FFmpeg` calls live in `analysis_inner`; users never need to write
//! `unsafe` code.

pub(crate) mod analysis_inner;
mod loudness_meter;
mod quality_metrics;

pub use loudness_meter::{LoudnessMeter, LoudnessResult};
pub use quality_metrics::QualityMetrics;
