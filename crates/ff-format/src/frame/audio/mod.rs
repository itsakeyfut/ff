//! Audio frame type.
//!
//! This module provides [`AudioFrame`] for working with decoded audio frames.
//!
//! # Examples
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

use crate::{SampleFormat, Timestamp};

mod buffer;
mod core;
mod samples;

/// A decoded audio frame.
///
/// This structure holds audio sample data and metadata for a segment of audio.
/// It supports both packed (interleaved) formats where all channels are
/// interleaved in a single buffer, and planar formats where each channel
/// is stored in a separate buffer.
///
/// # Memory Layout
///
/// For packed (interleaved) formats (I16, F32, etc.):
/// - Single plane containing interleaved samples: L R L R L R ...
/// - Total size: `samples * channels * bytes_per_sample`
///
/// For planar formats (I16p, F32p, etc.):
/// - One plane per channel
/// - Each plane size: `samples * bytes_per_sample`
///
/// # Examples
///
/// ```
/// use ff_format::{AudioFrame, SampleFormat, Timestamp, Rational};
///
/// // Create a stereo F32 frame with 1024 samples at 48kHz
/// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
///
/// assert_eq!(frame.samples(), 1024);
/// assert_eq!(frame.channels(), 2);
/// assert_eq!(frame.sample_rate(), 48000);
/// assert_eq!(frame.format(), SampleFormat::F32);
///
/// // Duration of this frame: 1024 / 48000 ≈ 21.33ms
/// let duration = frame.duration();
/// assert!((duration.as_secs_f64() - 0.02133).abs() < 0.001);
/// ```
#[derive(Clone)]
pub struct AudioFrame {
    /// Sample data for each plane (1 for packed, channels for planar)
    planes: Vec<Vec<u8>>,
    /// Number of samples per channel
    samples: usize,
    /// Number of audio channels
    channels: u32,
    /// Sample rate in Hz
    sample_rate: u32,
    /// Sample format
    format: SampleFormat,
    /// Presentation timestamp
    timestamp: Timestamp,
}
