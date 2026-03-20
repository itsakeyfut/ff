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

use std::fmt;
use std::time::Duration;

use crate::error::FrameError;
use crate::{SampleFormat, Timestamp};

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

impl AudioFrame {
    /// Creates a new audio frame with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `planes` - Audio sample data (1 plane for packed, channels for planar)
    /// * `samples` - Number of samples per channel
    /// * `channels` - Number of audio channels
    /// * `sample_rate` - Sample rate in Hz
    /// * `format` - Sample format
    /// * `timestamp` - Presentation timestamp
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::InvalidPlaneCount`] if the number of planes doesn't
    /// match the format (1 for packed, channels for planar).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat, Timestamp};
    ///
    /// // Create a mono F32 frame with 1024 samples
    /// let samples = 1024;
    /// let bytes_per_sample = 4; // F32
    /// let data = vec![0u8; samples * bytes_per_sample];
    ///
    /// let frame = AudioFrame::new(
    ///     vec![data],
    ///     samples,
    ///     1,
    ///     48000,
    ///     SampleFormat::F32,
    ///     Timestamp::default(),
    /// ).unwrap();
    ///
    /// assert_eq!(frame.samples(), 1024);
    /// assert_eq!(frame.channels(), 1);
    /// ```
    pub fn new(
        planes: Vec<Vec<u8>>,
        samples: usize,
        channels: u32,
        sample_rate: u32,
        format: SampleFormat,
        timestamp: Timestamp,
    ) -> Result<Self, FrameError> {
        let expected_planes = if format.is_planar() {
            channels as usize
        } else {
            1
        };

        if planes.len() != expected_planes {
            return Err(FrameError::InvalidPlaneCount {
                expected: expected_planes,
                actual: planes.len(),
            });
        }

        Ok(Self {
            planes,
            samples,
            channels,
            sample_rate,
            format,
            timestamp,
        })
    }

    /// Creates an empty audio frame with the specified parameters.
    ///
    /// The frame will have properly sized planes filled with zeros.
    ///
    /// # Arguments
    ///
    /// * `samples` - Number of samples per channel
    /// * `channels` - Number of audio channels
    /// * `sample_rate` - Sample rate in Hz
    /// * `format` - Sample format
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::UnsupportedSampleFormat`] if the format is
    /// [`SampleFormat::Other`], as the memory layout cannot be determined.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// // Create a stereo I16 frame
    /// let frame = AudioFrame::empty(1024, 2, 44100, SampleFormat::I16).unwrap();
    /// assert_eq!(frame.samples(), 1024);
    /// assert_eq!(frame.channels(), 2);
    /// assert_eq!(frame.num_planes(), 1); // Packed format
    /// ```
    pub fn empty(
        samples: usize,
        channels: u32,
        sample_rate: u32,
        format: SampleFormat,
    ) -> Result<Self, FrameError> {
        if matches!(format, SampleFormat::Other(_)) {
            return Err(FrameError::UnsupportedSampleFormat(format));
        }

        let planes = Self::allocate_planes(samples, channels, format);

        Ok(Self {
            planes,
            samples,
            channels,
            sample_rate,
            format,
            timestamp: Timestamp::default(),
        })
    }

    /// Allocates planes for the given parameters.
    fn allocate_planes(samples: usize, channels: u32, format: SampleFormat) -> Vec<Vec<u8>> {
        let bytes_per_sample = format.bytes_per_sample();

        if format.is_planar() {
            // Planar: one plane per channel
            let plane_size = samples * bytes_per_sample;
            (0..channels).map(|_| vec![0u8; plane_size]).collect()
        } else {
            // Packed: single plane with interleaved samples
            let total_size = samples * channels as usize * bytes_per_sample;
            vec![vec![0u8; total_size]]
        }
    }

    // ==========================================================================
    // Metadata Accessors
    // ==========================================================================

    /// Returns the number of samples per channel.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.samples(), 1024);
    /// ```
    #[must_use]
    #[inline]
    pub const fn samples(&self) -> usize {
        self.samples
    }

    /// Returns the number of audio channels.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.channels(), 2);
    /// ```
    #[must_use]
    #[inline]
    pub const fn channels(&self) -> u32 {
        self.channels
    }

    /// Returns the sample rate in Hz.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.sample_rate(), 48000);
    /// ```
    #[must_use]
    #[inline]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the sample format.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.format(), SampleFormat::F32);
    /// assert!(frame.format().is_float());
    /// ```
    #[must_use]
    #[inline]
    pub const fn format(&self) -> SampleFormat {
        self.format
    }

    /// Returns the presentation timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat, Timestamp, Rational};
    ///
    /// let ts = Timestamp::new(48000, Rational::new(1, 48000));
    /// let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// frame.set_timestamp(ts);
    /// assert_eq!(frame.timestamp(), ts);
    /// ```
    #[must_use]
    #[inline]
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Sets the presentation timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat, Timestamp, Rational};
    ///
    /// let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// let ts = Timestamp::new(48000, Rational::new(1, 48000));
    /// frame.set_timestamp(ts);
    /// assert_eq!(frame.timestamp(), ts);
    /// ```
    #[inline]
    pub fn set_timestamp(&mut self, timestamp: Timestamp) {
        self.timestamp = timestamp;
    }

    /// Returns the duration of this audio frame.
    ///
    /// The duration is calculated as `samples / sample_rate`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// // 1024 samples at 48kHz = ~21.33ms
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// let duration = frame.duration();
    /// assert!((duration.as_secs_f64() - 0.02133).abs() < 0.001);
    ///
    /// // 48000 samples at 48kHz = 1 second
    /// let frame = AudioFrame::empty(48000, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.duration().as_secs(), 1);
    /// ```
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Audio frame sample counts are well within f64's precision
    pub fn duration(&self) -> Duration {
        if self.sample_rate == 0 {
            log::warn!(
                "duration unavailable, sample_rate is 0, returning zero \
                 samples={} fallback=Duration::ZERO",
                self.samples
            );
            return Duration::ZERO;
        }
        let secs = self.samples as f64 / f64::from(self.sample_rate);
        Duration::from_secs_f64(secs)
    }

    // ==========================================================================
    // Plane Data Access
    // ==========================================================================

    /// Returns the number of planes in this frame.
    ///
    /// - Packed formats: 1 plane (interleaved channels)
    /// - Planar formats: 1 plane per channel
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// // Packed format - 1 plane
    /// let packed = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(packed.num_planes(), 1);
    ///
    /// // Planar format - 1 plane per channel
    /// let planar = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    /// assert_eq!(planar.num_planes(), 2);
    /// ```
    #[must_use]
    #[inline]
    pub fn num_planes(&self) -> usize {
        self.planes.len()
    }

    /// Returns a slice of all plane data.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    /// let planes = frame.planes();
    /// assert_eq!(planes.len(), 2);
    /// ```
    #[must_use]
    #[inline]
    pub fn planes(&self) -> &[Vec<u8>] {
        &self.planes
    }

    /// Returns the data for a specific plane, or `None` if the index is out of bounds.
    ///
    /// For packed formats, use `plane(0)`. For planar formats, use `plane(channel_index)`.
    ///
    /// # Arguments
    ///
    /// * `index` - The plane index (0 for packed, channel index for planar)
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    ///
    /// // Access left channel (plane 0)
    /// assert!(frame.plane(0).is_some());
    ///
    /// // Access right channel (plane 1)
    /// assert!(frame.plane(1).is_some());
    ///
    /// // No third channel
    /// assert!(frame.plane(2).is_none());
    /// ```
    #[must_use]
    #[inline]
    pub fn plane(&self, index: usize) -> Option<&[u8]> {
        self.planes.get(index).map(Vec::as_slice)
    }

    /// Returns mutable access to a specific plane's data.
    ///
    /// # Arguments
    ///
    /// * `index` - The plane index
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    /// if let Some(data) = frame.plane_mut(0) {
    ///     // Modify left channel
    ///     data[0] = 128;
    /// }
    /// ```
    #[must_use]
    #[inline]
    pub fn plane_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        self.planes.get_mut(index).map(Vec::as_mut_slice)
    }

    /// Returns the channel data for planar formats.
    ///
    /// This is an alias for [`plane()`](Self::plane) that's more semantically
    /// meaningful for audio data.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel index (0 = left, 1 = right, etc.)
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    ///
    /// // Get left channel data
    /// let left = frame.channel(0).unwrap();
    /// assert_eq!(left.len(), 1024 * 4); // 1024 samples * 4 bytes
    /// ```
    #[must_use]
    #[inline]
    pub fn channel(&self, channel: usize) -> Option<&[u8]> {
        self.plane(channel)
    }

    /// Returns mutable access to channel data for planar formats.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel index
    #[must_use]
    #[inline]
    pub fn channel_mut(&mut self, channel: usize) -> Option<&mut [u8]> {
        self.plane_mut(channel)
    }

    // ==========================================================================
    // Contiguous Data Access
    // ==========================================================================

    /// Returns the raw sample data as a contiguous byte slice.
    ///
    /// For packed formats, this returns a reference to the single plane.
    /// For planar formats, this returns `None` (use [`channel()`](Self::channel) instead).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// // Packed format - returns data
    /// let packed = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert!(packed.data().is_some());
    /// assert_eq!(packed.data().unwrap().len(), 1024 * 2 * 4);
    ///
    /// // Planar format - returns None
    /// let planar = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    /// assert!(planar.data().is_none());
    /// ```
    #[must_use]
    #[inline]
    pub fn data(&self) -> Option<&[u8]> {
        if self.format.is_packed() && self.planes.len() == 1 {
            Some(&self.planes[0])
        } else {
            None
        }
    }

    /// Returns mutable access to the raw sample data.
    ///
    /// Only available for packed formats.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// if let Some(data) = frame.data_mut() {
    ///     data[0] = 128;
    /// }
    /// ```
    #[must_use]
    #[inline]
    pub fn data_mut(&mut self) -> Option<&mut [u8]> {
        if self.format.is_packed() && self.planes.len() == 1 {
            Some(&mut self.planes[0])
        } else {
            None
        }
    }

    // ==========================================================================
    // Typed Sample Access
    // ==========================================================================
    //
    // These methods provide zero-copy typed access to audio sample data.
    // They use unsafe code to reinterpret byte buffers as typed slices.
    //
    // SAFETY: The data buffers are allocated with proper size and the
    // underlying Vec<u8> is guaranteed to be properly aligned for the
    // platform's requirements. We verify format matches before casting.

    /// Returns the sample data as an f32 slice.
    ///
    /// This only works if the format is [`SampleFormat::F32`] (packed).
    /// For planar F32p format, use [`channel_as_f32()`](Self::channel_as_f32).
    ///
    /// # Safety Note
    ///
    /// This method reinterprets the raw bytes as f32 values. It requires
    /// proper alignment and format matching.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// if let Some(samples) = frame.as_f32() {
    ///     assert_eq!(samples.len(), 1024 * 2); // samples * channels
    /// }
    /// ```
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn as_f32(&self) -> Option<&[f32]> {
        if self.format != SampleFormat::F32 {
            return None;
        }

        self.data().map(|bytes| {
            // SAFETY: We verified the format is F32, and the data was allocated
            // for F32 samples. Vec<u8> is aligned to at least 1 byte, but in practice
            // most allocators align to at least 8/16 bytes which is sufficient for f32.
            let ptr = bytes.as_ptr().cast::<f32>();
            let len = bytes.len() / std::mem::size_of::<f32>();
            unsafe { std::slice::from_raw_parts(ptr, len) }
        })
    }

    /// Returns mutable access to sample data as an f32 slice.
    ///
    /// Only works for [`SampleFormat::F32`] (packed).
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn as_f32_mut(&mut self) -> Option<&mut [f32]> {
        if self.format != SampleFormat::F32 {
            return None;
        }

        self.data_mut().map(|bytes| {
            let ptr = bytes.as_mut_ptr().cast::<f32>();
            let len = bytes.len() / std::mem::size_of::<f32>();
            unsafe { std::slice::from_raw_parts_mut(ptr, len) }
        })
    }

    /// Returns the sample data as an i16 slice.
    ///
    /// This only works if the format is [`SampleFormat::I16`] (packed).
    /// For planar I16p format, use [`channel_as_i16()`](Self::channel_as_i16).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16).unwrap();
    /// if let Some(samples) = frame.as_i16() {
    ///     assert_eq!(samples.len(), 1024 * 2);
    /// }
    /// ```
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn as_i16(&self) -> Option<&[i16]> {
        if self.format != SampleFormat::I16 {
            return None;
        }

        self.data().map(|bytes| {
            let ptr = bytes.as_ptr().cast::<i16>();
            let len = bytes.len() / std::mem::size_of::<i16>();
            unsafe { std::slice::from_raw_parts(ptr, len) }
        })
    }

    /// Returns mutable access to sample data as an i16 slice.
    ///
    /// Only works for [`SampleFormat::I16`] (packed).
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn as_i16_mut(&mut self) -> Option<&mut [i16]> {
        if self.format != SampleFormat::I16 {
            return None;
        }

        self.data_mut().map(|bytes| {
            let ptr = bytes.as_mut_ptr().cast::<i16>();
            let len = bytes.len() / std::mem::size_of::<i16>();
            unsafe { std::slice::from_raw_parts_mut(ptr, len) }
        })
    }

    /// Returns a specific channel's data as an f32 slice.
    ///
    /// Works for planar F32p format.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel index
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    /// if let Some(left) = frame.channel_as_f32(0) {
    ///     assert_eq!(left.len(), 1024);
    /// }
    /// ```
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn channel_as_f32(&self, channel: usize) -> Option<&[f32]> {
        if self.format != SampleFormat::F32p {
            return None;
        }

        self.channel(channel).map(|bytes| {
            let ptr = bytes.as_ptr().cast::<f32>();
            let len = bytes.len() / std::mem::size_of::<f32>();
            unsafe { std::slice::from_raw_parts(ptr, len) }
        })
    }

    /// Returns mutable access to a channel's data as an f32 slice.
    ///
    /// Works for planar F32p format.
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn channel_as_f32_mut(&mut self, channel: usize) -> Option<&mut [f32]> {
        if self.format != SampleFormat::F32p {
            return None;
        }

        self.channel_mut(channel).map(|bytes| {
            let ptr = bytes.as_mut_ptr().cast::<f32>();
            let len = bytes.len() / std::mem::size_of::<f32>();
            unsafe { std::slice::from_raw_parts_mut(ptr, len) }
        })
    }

    /// Returns a specific channel's data as an i16 slice.
    ///
    /// Works for planar I16p format.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel index
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16p).unwrap();
    /// if let Some(left) = frame.channel_as_i16(0) {
    ///     assert_eq!(left.len(), 1024);
    /// }
    /// ```
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn channel_as_i16(&self, channel: usize) -> Option<&[i16]> {
        if self.format != SampleFormat::I16p {
            return None;
        }

        self.channel(channel).map(|bytes| {
            let ptr = bytes.as_ptr().cast::<i16>();
            let len = bytes.len() / std::mem::size_of::<i16>();
            unsafe { std::slice::from_raw_parts(ptr, len) }
        })
    }

    /// Returns mutable access to a channel's data as an i16 slice.
    ///
    /// Works for planar I16p format.
    #[must_use]
    #[allow(unsafe_code, clippy::cast_ptr_alignment)]
    pub fn channel_as_i16_mut(&mut self, channel: usize) -> Option<&mut [i16]> {
        if self.format != SampleFormat::I16p {
            return None;
        }

        self.channel_mut(channel).map(|bytes| {
            let ptr = bytes.as_mut_ptr().cast::<i16>();
            let len = bytes.len() / std::mem::size_of::<i16>();
            unsafe { std::slice::from_raw_parts_mut(ptr, len) }
        })
    }

    // ==========================================================================
    // Utility Methods
    // ==========================================================================

    /// Returns the total size in bytes of all sample data.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.total_size(), 1024 * 2 * 4);
    /// ```
    #[must_use]
    pub fn total_size(&self) -> usize {
        self.planes.iter().map(Vec::len).sum()
    }

    /// Returns the size in bytes of a single sample (one channel).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.bytes_per_sample(), 4);
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16).unwrap();
    /// assert_eq!(frame.bytes_per_sample(), 2);
    /// ```
    #[must_use]
    #[inline]
    pub fn bytes_per_sample(&self) -> usize {
        self.format.bytes_per_sample()
    }

    /// Returns the total number of samples across all channels (`samples * channels`).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(frame.sample_count(), 2048);
    /// ```
    #[must_use]
    #[inline]
    pub fn sample_count(&self) -> usize {
        self.samples * self.channels as usize
    }

    // ==========================================================================
    // PCM Conversion
    // ==========================================================================

    /// Converts the audio frame to interleaved 32-bit float PCM.
    ///
    /// All [`SampleFormat`] variants are supported. Planar formats are transposed
    /// to interleaved layout (L0 R0 L1 R1 ...). Returns an empty `Vec` for
    /// [`SampleFormat::Other`].
    ///
    /// # Scaling
    ///
    /// | Source format | Normalization |
    /// |---|---|
    /// | U8  | `(sample − 128) / 128.0` → `[−1.0, 1.0]` |
    /// | I16 | `sample / 32767.0` → `[−1.0, 1.0]` |
    /// | I32 | `sample / 2147483647.0` → `[−1.0, 1.0]` |
    /// | F32 | identity |
    /// | F64 | narrowed to f32 (`as f32`) |
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(4, 2, 48000, SampleFormat::F32p).unwrap();
    /// let pcm = frame.to_f32_interleaved();
    /// assert_eq!(pcm.len(), 8); // 4 samples × 2 channels
    /// ```
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::too_many_lines
    )]
    pub fn to_f32_interleaved(&self) -> Vec<f32> {
        let total = self.sample_count();
        if total == 0 {
            return Vec::new();
        }

        match self.format {
            SampleFormat::F32 => self.as_f32().map(<[f32]>::to_vec).unwrap_or_default(),
            SampleFormat::F32p => {
                let mut out = vec![0f32; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(plane) = self.channel_as_f32(ch) {
                        for (i, &s) in plane.iter().enumerate() {
                            out[i * ch_count + ch] = s;
                        }
                    }
                }
                out
            }
            SampleFormat::F64 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .chunks_exact(8)
                    .map(|b| {
                        f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
                    })
                    .collect()
            }
            SampleFormat::F64p => {
                let mut out = vec![0f32; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, b) in bytes.chunks_exact(8).enumerate() {
                            out[i * ch_count + ch] = f64::from_le_bytes([
                                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                            ]) as f32;
                        }
                    }
                }
                out
            }
            SampleFormat::I16 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .chunks_exact(2)
                    .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / f32::from(i16::MAX))
                    .collect()
            }
            SampleFormat::I16p => {
                let mut out = vec![0f32; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, b) in bytes.chunks_exact(2).enumerate() {
                            out[i * ch_count + ch] =
                                f32::from(i16::from_le_bytes([b[0], b[1]])) / f32::from(i16::MAX);
                        }
                    }
                }
                out
            }
            SampleFormat::I32 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .chunks_exact(4)
                    .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / i32::MAX as f32)
                    .collect()
            }
            SampleFormat::I32p => {
                let mut out = vec![0f32; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, b) in bytes.chunks_exact(4).enumerate() {
                            out[i * ch_count + ch] = i32::from_le_bytes([b[0], b[1], b[2], b[3]])
                                as f32
                                / i32::MAX as f32;
                        }
                    }
                }
                out
            }
            SampleFormat::U8 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .iter()
                    .map(|&b| (f32::from(b) - 128.0) / 128.0)
                    .collect()
            }
            SampleFormat::U8p => {
                let mut out = vec![0f32; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, &b) in bytes.iter().enumerate() {
                            out[i * ch_count + ch] = (f32::from(b) - 128.0) / 128.0;
                        }
                    }
                }
                out
            }
            SampleFormat::Other(_) => Vec::new(),
        }
    }

    /// Converts the audio frame to interleaved 16-bit signed integer PCM.
    ///
    /// Suitable for use with `rodio::buffer::SamplesBuffer<i16>`. All
    /// [`SampleFormat`] variants are supported. Returns an empty `Vec` for
    /// [`SampleFormat::Other`].
    ///
    /// # Scaling
    ///
    /// | Source format | Conversion |
    /// |---|---|
    /// | I16 | identity |
    /// | I32 | `sample >> 16` (high 16 bits) |
    /// | U8  | `(sample − 128) << 8` |
    /// | F32/F64 | `clamp(−1, 1) × 32767`, truncated |
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let frame = AudioFrame::empty(4, 2, 48000, SampleFormat::I16p).unwrap();
    /// let pcm = frame.to_i16_interleaved();
    /// assert_eq!(pcm.len(), 8);
    /// ```
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)] // float→i16 and i32→i16 are intentional truncations
    pub fn to_i16_interleaved(&self) -> Vec<i16> {
        let total = self.sample_count();
        if total == 0 {
            return Vec::new();
        }

        match self.format {
            SampleFormat::I16 => self.as_i16().map(<[i16]>::to_vec).unwrap_or_default(),
            SampleFormat::I16p => {
                let mut out = vec![0i16; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(plane) = self.channel_as_i16(ch) {
                        for (i, &s) in plane.iter().enumerate() {
                            out[i * ch_count + ch] = s;
                        }
                    }
                }
                out
            }
            SampleFormat::F32 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .chunks_exact(4)
                    .map(|b| {
                        let s = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                        (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
                    })
                    .collect()
            }
            SampleFormat::F32p => {
                let mut out = vec![0i16; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, b) in bytes.chunks_exact(4).enumerate() {
                            let s = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                            out[i * ch_count + ch] =
                                (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                        }
                    }
                }
                out
            }
            SampleFormat::F64 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .chunks_exact(8)
                    .map(|b| {
                        let s =
                            f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
                        (s.clamp(-1.0, 1.0) * f64::from(i16::MAX)) as i16
                    })
                    .collect()
            }
            SampleFormat::F64p => {
                let mut out = vec![0i16; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, b) in bytes.chunks_exact(8).enumerate() {
                            let s = f64::from_le_bytes([
                                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
                            ]);
                            out[i * ch_count + ch] =
                                (s.clamp(-1.0, 1.0) * f64::from(i16::MAX)) as i16;
                        }
                    }
                }
                out
            }
            SampleFormat::I32 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes
                    .chunks_exact(4)
                    .map(|b| (i32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 16) as i16)
                    .collect()
            }
            SampleFormat::I32p => {
                let mut out = vec![0i16; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, b) in bytes.chunks_exact(4).enumerate() {
                            out[i * ch_count + ch] =
                                (i32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 16) as i16;
                        }
                    }
                }
                out
            }
            SampleFormat::U8 => {
                let Some(bytes) = self.data() else {
                    return Vec::new();
                };
                bytes.iter().map(|&b| (i16::from(b) - 128) << 8).collect()
            }
            SampleFormat::U8p => {
                let mut out = vec![0i16; total];
                let ch_count = self.channels as usize;
                for ch in 0..ch_count {
                    if let Some(bytes) = self.channel(ch) {
                        for (i, &b) in bytes.iter().enumerate() {
                            out[i * ch_count + ch] = (i16::from(b) - 128) << 8;
                        }
                    }
                }
                out
            }
            SampleFormat::Other(_) => Vec::new(),
        }
    }
}

impl fmt::Debug for AudioFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioFrame")
            .field("samples", &self.samples)
            .field("channels", &self.channels)
            .field("sample_rate", &self.sample_rate)
            .field("format", &self.format)
            .field("timestamp", &self.timestamp)
            .field("num_planes", &self.planes.len())
            .field(
                "plane_sizes",
                &self.planes.iter().map(Vec::len).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl fmt::Display for AudioFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let duration_ms = self.duration().as_secs_f64() * 1000.0;
        write!(
            f,
            "AudioFrame({} samples, {}ch, {}Hz, {} @ {}, {:.2}ms)",
            self.samples, self.channels, self.sample_rate, self.format, self.timestamp, duration_ms
        )
    }
}

impl Default for AudioFrame {
    /// Returns a default empty mono F32 frame with 0 samples.
    fn default() -> Self {
        Self {
            planes: vec![vec![]],
            samples: 0,
            channels: 1,
            sample_rate: 48000,
            format: SampleFormat::F32,
            timestamp: Timestamp::default(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::redundant_closure_for_method_calls)]
mod tests {
    use super::*;
    use crate::Rational;

    // ==========================================================================
    // Construction Tests
    // ==========================================================================

    #[test]
    fn test_new_packed_f32() {
        let samples = 1024;
        let channels = 2u32;
        let bytes_per_sample = 4;
        let data = vec![0u8; samples * channels as usize * bytes_per_sample];

        let frame = AudioFrame::new(
            vec![data],
            samples,
            channels,
            48000,
            SampleFormat::F32,
            Timestamp::default(),
        )
        .unwrap();

        assert_eq!(frame.samples(), 1024);
        assert_eq!(frame.channels(), 2);
        assert_eq!(frame.sample_rate(), 48000);
        assert_eq!(frame.format(), SampleFormat::F32);
        assert_eq!(frame.num_planes(), 1);
    }

    #[test]
    fn test_new_planar_f32p() {
        let samples = 1024;
        let channels = 2u32;
        let bytes_per_sample = 4;
        let plane_size = samples * bytes_per_sample;

        let planes = vec![vec![0u8; plane_size], vec![0u8; plane_size]];

        let frame = AudioFrame::new(
            planes,
            samples,
            channels,
            48000,
            SampleFormat::F32p,
            Timestamp::default(),
        )
        .unwrap();

        assert_eq!(frame.samples(), 1024);
        assert_eq!(frame.channels(), 2);
        assert_eq!(frame.format(), SampleFormat::F32p);
        assert_eq!(frame.num_planes(), 2);
    }

    #[test]
    fn test_new_invalid_plane_count_packed() {
        // Packed format should have 1 plane, but we provide 2
        let result = AudioFrame::new(
            vec![vec![0u8; 100], vec![0u8; 100]],
            100,
            2,
            48000,
            SampleFormat::F32,
            Timestamp::default(),
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            FrameError::InvalidPlaneCount {
                expected: 1,
                actual: 2
            }
        );
    }

    #[test]
    fn test_new_invalid_plane_count_planar() {
        // Planar format with 2 channels should have 2 planes, but we provide 1
        let result = AudioFrame::new(
            vec![vec![0u8; 100]],
            100,
            2,
            48000,
            SampleFormat::F32p,
            Timestamp::default(),
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            FrameError::InvalidPlaneCount {
                expected: 2,
                actual: 1
            }
        );
    }

    #[test]
    fn test_empty_packed() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();

        assert_eq!(frame.samples(), 1024);
        assert_eq!(frame.channels(), 2);
        assert_eq!(frame.sample_rate(), 48000);
        assert_eq!(frame.format(), SampleFormat::F32);
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.total_size(), 1024 * 2 * 4);
    }

    #[test]
    fn test_empty_planar() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();

        assert_eq!(frame.num_planes(), 2);
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(1024 * 4));
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(1024 * 4));
        assert_eq!(frame.total_size(), 1024 * 2 * 4);
    }

    #[test]
    fn test_empty_i16() {
        let frame = AudioFrame::empty(1024, 2, 44100, SampleFormat::I16).unwrap();

        assert_eq!(frame.bytes_per_sample(), 2);
        assert_eq!(frame.total_size(), 1024 * 2 * 2);
    }

    #[test]
    fn test_empty_other_format_error() {
        let result = AudioFrame::empty(1024, 2, 48000, SampleFormat::Other(999));

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            FrameError::UnsupportedSampleFormat(SampleFormat::Other(999))
        );
    }

    #[test]
    fn test_default() {
        let frame = AudioFrame::default();

        assert_eq!(frame.samples(), 0);
        assert_eq!(frame.channels(), 1);
        assert_eq!(frame.sample_rate(), 48000);
        assert_eq!(frame.format(), SampleFormat::F32);
    }

    // ==========================================================================
    // Metadata Tests
    // ==========================================================================

    #[test]
    fn test_duration() {
        // 1024 samples at 48kHz = 0.02133... seconds
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        let duration = frame.duration();
        assert!((duration.as_secs_f64() - 0.021_333_333).abs() < 0.000_001);

        // 48000 samples at 48kHz = 1 second
        let frame = AudioFrame::empty(48000, 2, 48000, SampleFormat::F32).unwrap();
        assert_eq!(frame.duration().as_secs(), 1);
    }

    #[test]
    fn test_duration_zero_sample_rate() {
        let frame = AudioFrame::new(
            vec![vec![]],
            0,
            1,
            0,
            SampleFormat::F32,
            Timestamp::default(),
        )
        .unwrap();

        assert_eq!(frame.duration(), Duration::ZERO);
    }

    #[test]
    fn test_set_timestamp() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        let ts = Timestamp::new(48000, Rational::new(1, 48000));

        frame.set_timestamp(ts);
        assert_eq!(frame.timestamp(), ts);
    }

    // ==========================================================================
    // Plane Access Tests
    // ==========================================================================

    #[test]
    fn test_plane_access_packed() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();

        assert!(frame.plane(0).is_some());
        assert!(frame.plane(1).is_none());
    }

    #[test]
    fn test_plane_access_planar() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();

        assert!(frame.plane(0).is_some());
        assert!(frame.plane(1).is_some());
        assert!(frame.plane(2).is_none());
    }

    #[test]
    fn test_plane_mut_access() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();

        if let Some(data) = frame.plane_mut(0) {
            data[0] = 255;
        }

        assert_eq!(frame.plane(0).unwrap()[0], 255);
    }

    #[test]
    fn test_channel_access() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();

        let left = frame.channel(0).unwrap();
        let right = frame.channel(1).unwrap();

        assert_eq!(left.len(), 1024 * 4);
        assert_eq!(right.len(), 1024 * 4);
    }

    // ==========================================================================
    // Data Access Tests
    // ==========================================================================

    #[test]
    fn test_data_packed() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(frame.data().is_some());
        assert_eq!(frame.data().unwrap().len(), 1024 * 2 * 4);
    }

    #[test]
    fn test_data_planar_returns_none() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        assert!(frame.data().is_none());
    }

    #[test]
    fn test_data_mut() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();

        if let Some(data) = frame.data_mut() {
            data[0] = 123;
        }

        assert_eq!(frame.data().unwrap()[0], 123);
    }

    // ==========================================================================
    // Typed Access Tests
    // ==========================================================================

    #[test]
    fn test_as_f32() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        let samples = frame.as_f32().unwrap();
        assert_eq!(samples.len(), 1024 * 2);
    }

    #[test]
    fn test_as_f32_wrong_format() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16).unwrap();
        assert!(frame.as_f32().is_none());
    }

    #[test]
    fn test_as_f32_mut() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();

        if let Some(samples) = frame.as_f32_mut() {
            samples[0] = 1.0;
            samples[1] = -1.0;
        }

        let samples = frame.as_f32().unwrap();
        assert!((samples[0] - 1.0).abs() < f32::EPSILON);
        assert!((samples[1] - (-1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_as_i16() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16).unwrap();
        let samples = frame.as_i16().unwrap();
        assert_eq!(samples.len(), 1024 * 2);
    }

    #[test]
    fn test_as_i16_wrong_format() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(frame.as_i16().is_none());
    }

    #[test]
    fn test_channel_as_f32() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();

        let left = frame.channel_as_f32(0).unwrap();
        let right = frame.channel_as_f32(1).unwrap();

        assert_eq!(left.len(), 1024);
        assert_eq!(right.len(), 1024);
    }

    #[test]
    fn test_channel_as_f32_wrong_format() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(frame.channel_as_f32(0).is_none()); // F32 is packed, not F32p
    }

    #[test]
    fn test_channel_as_f32_mut() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();

        if let Some(left) = frame.channel_as_f32_mut(0) {
            left[0] = 0.5;
        }

        assert!((frame.channel_as_f32(0).unwrap()[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_channel_as_i16() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16p).unwrap();

        let left = frame.channel_as_i16(0).unwrap();
        assert_eq!(left.len(), 1024);
    }

    #[test]
    fn test_channel_as_i16_mut() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16p).unwrap();

        if let Some(left) = frame.channel_as_i16_mut(0) {
            left[0] = 32767;
        }

        assert_eq!(frame.channel_as_i16(0).unwrap()[0], 32767);
    }

    // ==========================================================================
    // Utility Tests
    // ==========================================================================

    #[test]
    fn test_total_size() {
        // Packed stereo F32: 1024 samples * 2 channels * 4 bytes
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert_eq!(frame.total_size(), 1024 * 2 * 4);

        // Planar stereo F32p: 2 planes * 1024 samples * 4 bytes
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        assert_eq!(frame.total_size(), 1024 * 4 * 2);
    }

    #[test]
    fn test_bytes_per_sample() {
        assert_eq!(
            AudioFrame::empty(1024, 2, 48000, SampleFormat::U8)
                .unwrap()
                .bytes_per_sample(),
            1
        );
        assert_eq!(
            AudioFrame::empty(1024, 2, 48000, SampleFormat::I16)
                .unwrap()
                .bytes_per_sample(),
            2
        );
        assert_eq!(
            AudioFrame::empty(1024, 2, 48000, SampleFormat::F32)
                .unwrap()
                .bytes_per_sample(),
            4
        );
        assert_eq!(
            AudioFrame::empty(1024, 2, 48000, SampleFormat::F64)
                .unwrap()
                .bytes_per_sample(),
            8
        );
    }

    // ==========================================================================
    // Clone Tests
    // ==========================================================================

    #[test]
    fn test_clone() {
        let mut original = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        original.set_timestamp(Timestamp::new(1000, Rational::new(1, 1000)));

        // Modify some data
        if let Some(data) = original.plane_mut(0) {
            data[0] = 42;
        }

        let cloned = original.clone();

        // Verify metadata matches
        assert_eq!(cloned.samples(), original.samples());
        assert_eq!(cloned.channels(), original.channels());
        assert_eq!(cloned.sample_rate(), original.sample_rate());
        assert_eq!(cloned.format(), original.format());
        assert_eq!(cloned.timestamp(), original.timestamp());

        // Verify data was cloned
        assert_eq!(cloned.plane(0).unwrap()[0], 42);

        // Verify it's a deep clone
        let mut cloned = cloned;
        if let Some(data) = cloned.plane_mut(0) {
            data[0] = 99;
        }
        assert_eq!(original.plane(0).unwrap()[0], 42);
        assert_eq!(cloned.plane(0).unwrap()[0], 99);
    }

    // ==========================================================================
    // Display/Debug Tests
    // ==========================================================================

    #[test]
    fn test_debug() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        let debug = format!("{frame:?}");
        assert!(debug.contains("AudioFrame"));
        assert!(debug.contains("1024"));
        assert!(debug.contains("48000"));
        assert!(debug.contains("F32"));
    }

    #[test]
    fn test_display() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        let display = format!("{frame}");
        assert!(display.contains("1024 samples"));
        assert!(display.contains("2ch"));
        assert!(display.contains("48000Hz"));
        assert!(display.contains("flt")); // F32 displays as "flt"
    }

    // ==========================================================================
    // All Sample Formats Tests
    // ==========================================================================

    #[test]
    fn test_all_packed_formats() {
        let formats = [
            SampleFormat::U8,
            SampleFormat::I16,
            SampleFormat::I32,
            SampleFormat::F32,
            SampleFormat::F64,
        ];

        for format in formats {
            let frame = AudioFrame::empty(1024, 2, 48000, format).unwrap();
            assert_eq!(frame.num_planes(), 1);
            assert!(frame.data().is_some());
        }
    }

    #[test]
    fn test_all_planar_formats() {
        let formats = [
            SampleFormat::U8p,
            SampleFormat::I16p,
            SampleFormat::I32p,
            SampleFormat::F32p,
            SampleFormat::F64p,
        ];

        for format in formats {
            let frame = AudioFrame::empty(1024, 2, 48000, format).unwrap();
            assert_eq!(frame.num_planes(), 2);
            assert!(frame.data().is_none());
            assert!(frame.channel(0).is_some());
            assert!(frame.channel(1).is_some());
        }
    }

    #[test]
    fn test_mono_planar() {
        let frame = AudioFrame::empty(1024, 1, 48000, SampleFormat::F32p).unwrap();
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(1024 * 4));
    }

    #[test]
    fn test_surround_planar() {
        // 5.1 surround sound
        let frame = AudioFrame::empty(1024, 6, 48000, SampleFormat::F32p).unwrap();
        assert_eq!(frame.num_planes(), 6);
        for i in 0..6 {
            assert!(frame.plane(i).is_some());
        }
        assert!(frame.plane(6).is_none());
    }

    // ==========================================================================
    // PCM Conversion Tests
    // ==========================================================================

    #[test]
    fn sample_count_should_return_samples_times_channels() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert_eq!(frame.sample_count(), 2048);

        let mono = AudioFrame::empty(512, 1, 44100, SampleFormat::I16).unwrap();
        assert_eq!(mono.sample_count(), 512);
    }

    #[test]
    fn to_f32_interleaved_f32p_should_transpose_to_interleaved() {
        // Stereo F32p: L=[1.0, 2.0], R=[3.0, 4.0]
        // Expected interleaved: [1.0, 3.0, 2.0, 4.0]
        let left: Vec<u8> = [1.0f32, 2.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let right: Vec<u8> = [3.0f32, 4.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let frame = AudioFrame::new(
            vec![left, right],
            2,
            2,
            48000,
            SampleFormat::F32p,
            Timestamp::default(),
        )
        .unwrap();

        let pcm = frame.to_f32_interleaved();
        assert_eq!(pcm.len(), 4);
        assert!((pcm[0] - 1.0).abs() < f32::EPSILON); // L0
        assert!((pcm[1] - 3.0).abs() < f32::EPSILON); // R0
        assert!((pcm[2] - 2.0).abs() < f32::EPSILON); // L1
        assert!((pcm[3] - 4.0).abs() < f32::EPSILON); // R1
    }

    #[test]
    fn to_f32_interleaved_i16p_should_scale_to_minus_one_to_one() {
        // i16::MAX → ~1.0,  i16::MIN → ~-1.0,  0 → 0.0
        let make_i16_bytes = |v: i16| v.to_le_bytes().to_vec();

        let left: Vec<u8> = [i16::MAX, 0i16]
            .iter()
            .flat_map(|&v| make_i16_bytes(v))
            .collect();
        let right: Vec<u8> = [i16::MIN, 0i16]
            .iter()
            .flat_map(|&v| make_i16_bytes(v))
            .collect();

        let frame = AudioFrame::new(
            vec![left, right],
            2,
            2,
            48000,
            SampleFormat::I16p,
            Timestamp::default(),
        )
        .unwrap();

        let pcm = frame.to_f32_interleaved();
        // i16 is asymmetric: MIN=-32768, MAX=32767, so MIN/MAX ≈ -1.00003
        // Values should be very close to [-1.0, 1.0]
        for &s in &pcm {
            assert!(s >= -1.001 && s <= 1.001, "out of range: {s}");
        }
        assert!((pcm[0] - 1.0).abs() < 0.0001); // i16::MAX → ~1.0
        assert!((pcm[1] - (-1.0)).abs() < 0.001); // i16::MIN → ~-1.00003
    }

    #[test]
    fn to_f32_interleaved_unknown_should_return_empty() {
        // AudioFrame::new() (unlike empty()) accepts Other(_): Other is treated
        // as packed so expected_planes = 1.
        let frame = AudioFrame::new(
            vec![vec![0u8; 16]],
            4,
            1,
            48000,
            SampleFormat::Other(999),
            Timestamp::default(),
        )
        .unwrap();
        assert_eq!(frame.to_f32_interleaved(), Vec::<f32>::new());
    }

    #[test]
    fn to_i16_interleaved_i16p_should_transpose_to_interleaved() {
        let left: Vec<u8> = [100i16, 200i16]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let right: Vec<u8> = [300i16, 400i16]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();

        let frame = AudioFrame::new(
            vec![left, right],
            2,
            2,
            48000,
            SampleFormat::I16p,
            Timestamp::default(),
        )
        .unwrap();

        let pcm = frame.to_i16_interleaved();
        assert_eq!(pcm, vec![100, 300, 200, 400]);
    }

    #[test]
    fn to_i16_interleaved_f32_should_scale_and_clamp() {
        // 1.0 → i16::MAX, -1.0 → -i16::MAX, 2.0 → clamped to i16::MAX
        let samples: &[f32] = &[1.0, -1.0, 2.0, -2.0];
        let bytes: Vec<u8> = samples.iter().flat_map(|f| f.to_le_bytes()).collect();

        let frame = AudioFrame::new(
            vec![bytes],
            4,
            1,
            48000,
            SampleFormat::F32,
            Timestamp::default(),
        )
        .unwrap();

        let pcm = frame.to_i16_interleaved();
        assert_eq!(pcm.len(), 4);
        assert_eq!(pcm[0], i16::MAX);
        assert_eq!(pcm[1], -i16::MAX);
        // Clamped values
        assert_eq!(pcm[2], i16::MAX);
        assert_eq!(pcm[3], -i16::MAX);
    }
}
