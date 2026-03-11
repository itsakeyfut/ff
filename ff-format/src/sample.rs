//! Audio sample format definitions for audio processing.
//!
//! This module provides the [`SampleFormat`] enum which represents various
//! audio sample formats used in audio processing. It supports both packed
//! (interleaved) and planar formats commonly used in audio editing.
//!
//! # Examples
//!
//! ```
//! use ff_format::SampleFormat;
//!
//! let format = SampleFormat::F32;
//! assert!(!format.is_planar());
//! assert!(format.is_float());
//! assert_eq!(format.bytes_per_sample(), 4);
//!
//! let planar = SampleFormat::I16p;
//! assert!(planar.is_planar());
//! assert_eq!(planar.packed_equivalent(), SampleFormat::I16);
//! ```

use std::fmt;

/// Audio sample format for audio frames.
///
/// This enum represents various sample formats used in audio processing.
/// It is designed to cover the most common formats used in audio editing
/// while remaining extensible via the `Other` variant.
///
/// # Format Categories
///
/// - **Packed (Interleaved)**: Samples from all channels are interleaved
///   (U8, I16, I32, F32, F64)
/// - **Planar**: Each channel stored in a separate buffer
///   (U8p, I16p, I32p, F32p, F64p)
///
/// # Common Usage
///
/// - **I16**: CD quality audio (16-bit signed)
/// - **F32**: Most common for audio editing (32-bit float)
/// - **F32p**: Common in `FFmpeg` decoders for processing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SampleFormat {
    // Packed (interleaved) formats
    /// 8-bit unsigned integer (0-255)
    U8,
    /// 16-bit signed integer - CD quality audio
    I16,
    /// 32-bit signed integer
    I32,
    /// 32-bit floating point - most common for editing
    F32,
    /// 64-bit floating point - highest precision
    F64,

    // Planar formats
    /// 8-bit unsigned integer, planar
    U8p,
    /// 16-bit signed integer, planar
    I16p,
    /// 32-bit signed integer, planar
    I32p,
    /// 32-bit floating point, planar - common in decoders
    F32p,
    /// 64-bit floating point, planar
    F64p,

    // Extensibility
    /// Unknown or unsupported format with `FFmpeg`'s `AVSampleFormat` value
    Other(u32),
}

impl SampleFormat {
    /// Returns the format name as a human-readable string.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert_eq!(SampleFormat::I16.name(), "s16");
    /// assert_eq!(SampleFormat::F32.name(), "flt");
    /// assert_eq!(SampleFormat::F32p.name(), "fltp");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::I16 => "s16",
            Self::I32 => "s32",
            Self::F32 => "flt",
            Self::F64 => "dbl",
            Self::U8p => "u8p",
            Self::I16p => "s16p",
            Self::I32p => "s32p",
            Self::F32p => "fltp",
            Self::F64p => "dblp",
            Self::Other(_) => "unknown",
        }
    }

    /// Returns the number of bytes per sample.
    ///
    /// This is the size of a single sample value, regardless of whether
    /// the format is planar or packed.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert_eq!(SampleFormat::U8.bytes_per_sample(), 1);
    /// assert_eq!(SampleFormat::I16.bytes_per_sample(), 2);
    /// assert_eq!(SampleFormat::I32.bytes_per_sample(), 4);
    /// assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
    /// assert_eq!(SampleFormat::F64.bytes_per_sample(), 8);
    /// // Planar formats have the same bytes per sample
    /// assert_eq!(SampleFormat::F32p.bytes_per_sample(), 4);
    /// ```
    #[must_use]
    pub const fn bytes_per_sample(&self) -> usize {
        match self {
            Self::U8 | Self::U8p => 1,
            Self::I16 | Self::I16p => 2,
            Self::I32 | Self::I32p | Self::F32 | Self::F32p | Self::Other(_) => 4,
            Self::F64 | Self::F64p => 8,
        }
    }

    /// Returns `true` if this is a planar format.
    ///
    /// In planar formats, samples for each channel are stored in separate
    /// contiguous buffers. This is more efficient for processing but
    /// requires conversion for output.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert!(!SampleFormat::F32.is_planar());
    /// assert!(SampleFormat::F32p.is_planar());
    /// assert!(!SampleFormat::I16.is_planar());
    /// assert!(SampleFormat::I16p.is_planar());
    /// ```
    #[must_use]
    pub const fn is_planar(&self) -> bool {
        matches!(
            self,
            Self::U8p | Self::I16p | Self::I32p | Self::F32p | Self::F64p
        )
    }

    /// Returns `true` if this is a packed (interleaved) format.
    ///
    /// In packed formats, samples from all channels are interleaved
    /// (e.g., L R L R L R for stereo). This is the format typically
    /// used for audio output.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert!(SampleFormat::F32.is_packed());
    /// assert!(!SampleFormat::F32p.is_packed());
    /// assert!(SampleFormat::I16.is_packed());
    /// assert!(!SampleFormat::I16p.is_packed());
    /// ```
    #[must_use]
    pub const fn is_packed(&self) -> bool {
        !self.is_planar()
    }

    /// Returns `true` if this is a floating-point format.
    ///
    /// Floating-point formats (F32, F64, F32p, F64p) offer higher
    /// dynamic range and are preferred for audio processing to
    /// avoid clipping.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert!(SampleFormat::F32.is_float());
    /// assert!(SampleFormat::F64.is_float());
    /// assert!(SampleFormat::F32p.is_float());
    /// assert!(!SampleFormat::I16.is_float());
    /// assert!(!SampleFormat::I32.is_float());
    /// ```
    #[must_use]
    pub const fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64 | Self::F32p | Self::F64p)
    }

    /// Returns `true` if this is an integer format.
    ///
    /// Integer formats include both signed (I16, I32) and unsigned (U8) types.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert!(SampleFormat::I16.is_integer());
    /// assert!(SampleFormat::U8.is_integer());
    /// assert!(!SampleFormat::F32.is_integer());
    /// ```
    #[must_use]
    pub const fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::U8 | Self::I16 | Self::I32 | Self::U8p | Self::I16p | Self::I32p
        )
    }

    /// Returns `true` if this is a signed format.
    ///
    /// All formats except U8 and U8p are signed.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert!(SampleFormat::I16.is_signed());
    /// assert!(SampleFormat::F32.is_signed());
    /// assert!(!SampleFormat::U8.is_signed());
    /// ```
    #[must_use]
    pub const fn is_signed(&self) -> bool {
        !matches!(self, Self::U8 | Self::U8p | Self::Other(_))
    }

    /// Returns the packed (interleaved) equivalent of this format.
    ///
    /// If the format is already packed, returns itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert_eq!(SampleFormat::F32p.packed_equivalent(), SampleFormat::F32);
    /// assert_eq!(SampleFormat::I16p.packed_equivalent(), SampleFormat::I16);
    /// assert_eq!(SampleFormat::F32.packed_equivalent(), SampleFormat::F32);
    /// ```
    #[must_use]
    pub const fn packed_equivalent(&self) -> Self {
        match self {
            Self::U8p => Self::U8,
            Self::I16p => Self::I16,
            Self::I32p => Self::I32,
            Self::F32p => Self::F32,
            Self::F64p => Self::F64,
            // Already packed or unknown
            other => *other,
        }
    }

    /// Returns the planar equivalent of this format.
    ///
    /// If the format is already planar, returns itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert_eq!(SampleFormat::F32.planar_equivalent(), SampleFormat::F32p);
    /// assert_eq!(SampleFormat::I16.planar_equivalent(), SampleFormat::I16p);
    /// assert_eq!(SampleFormat::F32p.planar_equivalent(), SampleFormat::F32p);
    /// ```
    #[must_use]
    pub const fn planar_equivalent(&self) -> Self {
        match self {
            Self::U8 => Self::U8p,
            Self::I16 => Self::I16p,
            Self::I32 => Self::I32p,
            Self::F32 => Self::F32p,
            Self::F64 => Self::F64p,
            // Already planar or unknown
            other => *other,
        }
    }

    /// Returns the bit depth of this format.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::SampleFormat;
    ///
    /// assert_eq!(SampleFormat::U8.bit_depth(), 8);
    /// assert_eq!(SampleFormat::I16.bit_depth(), 16);
    /// assert_eq!(SampleFormat::F32.bit_depth(), 32);
    /// assert_eq!(SampleFormat::F64.bit_depth(), 64);
    /// ```
    #[must_use]
    pub const fn bit_depth(&self) -> usize {
        self.bytes_per_sample() * 8
    }
}

impl fmt::Display for SampleFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Default for SampleFormat {
    /// Returns the default sample format.
    ///
    /// The default is [`SampleFormat::F32`] as it's the most common
    /// format used in audio editing and processing.
    fn default() -> Self {
        Self::F32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_names() {
        assert_eq!(SampleFormat::U8.name(), "u8");
        assert_eq!(SampleFormat::I16.name(), "s16");
        assert_eq!(SampleFormat::I32.name(), "s32");
        assert_eq!(SampleFormat::F32.name(), "flt");
        assert_eq!(SampleFormat::F64.name(), "dbl");
        assert_eq!(SampleFormat::U8p.name(), "u8p");
        assert_eq!(SampleFormat::I16p.name(), "s16p");
        assert_eq!(SampleFormat::I32p.name(), "s32p");
        assert_eq!(SampleFormat::F32p.name(), "fltp");
        assert_eq!(SampleFormat::F64p.name(), "dblp");
        assert_eq!(SampleFormat::Other(999).name(), "unknown");
    }

    #[test]
    fn test_bytes_per_sample() {
        // 1 byte formats
        assert_eq!(SampleFormat::U8.bytes_per_sample(), 1);
        assert_eq!(SampleFormat::U8p.bytes_per_sample(), 1);

        // 2 byte formats
        assert_eq!(SampleFormat::I16.bytes_per_sample(), 2);
        assert_eq!(SampleFormat::I16p.bytes_per_sample(), 2);

        // 4 byte formats
        assert_eq!(SampleFormat::I32.bytes_per_sample(), 4);
        assert_eq!(SampleFormat::I32p.bytes_per_sample(), 4);
        assert_eq!(SampleFormat::F32.bytes_per_sample(), 4);
        assert_eq!(SampleFormat::F32p.bytes_per_sample(), 4);

        // 8 byte formats
        assert_eq!(SampleFormat::F64.bytes_per_sample(), 8);
        assert_eq!(SampleFormat::F64p.bytes_per_sample(), 8);

        // Unknown defaults to 4
        assert_eq!(SampleFormat::Other(123).bytes_per_sample(), 4);
    }

    #[test]
    fn test_is_planar() {
        // Packed formats
        assert!(!SampleFormat::U8.is_planar());
        assert!(!SampleFormat::I16.is_planar());
        assert!(!SampleFormat::I32.is_planar());
        assert!(!SampleFormat::F32.is_planar());
        assert!(!SampleFormat::F64.is_planar());
        assert!(!SampleFormat::Other(0).is_planar());

        // Planar formats
        assert!(SampleFormat::U8p.is_planar());
        assert!(SampleFormat::I16p.is_planar());
        assert!(SampleFormat::I32p.is_planar());
        assert!(SampleFormat::F32p.is_planar());
        assert!(SampleFormat::F64p.is_planar());
    }

    #[test]
    fn test_is_packed() {
        // Packed formats
        assert!(SampleFormat::U8.is_packed());
        assert!(SampleFormat::I16.is_packed());
        assert!(SampleFormat::I32.is_packed());
        assert!(SampleFormat::F32.is_packed());
        assert!(SampleFormat::F64.is_packed());

        // Planar formats
        assert!(!SampleFormat::U8p.is_packed());
        assert!(!SampleFormat::I16p.is_packed());
        assert!(!SampleFormat::I32p.is_packed());
        assert!(!SampleFormat::F32p.is_packed());
        assert!(!SampleFormat::F64p.is_packed());
    }

    #[test]
    fn test_is_float() {
        // Float formats
        assert!(SampleFormat::F32.is_float());
        assert!(SampleFormat::F64.is_float());
        assert!(SampleFormat::F32p.is_float());
        assert!(SampleFormat::F64p.is_float());

        // Integer formats
        assert!(!SampleFormat::U8.is_float());
        assert!(!SampleFormat::I16.is_float());
        assert!(!SampleFormat::I32.is_float());
        assert!(!SampleFormat::U8p.is_float());
        assert!(!SampleFormat::I16p.is_float());
        assert!(!SampleFormat::I32p.is_float());
        assert!(!SampleFormat::Other(0).is_float());
    }

    #[test]
    fn test_is_integer() {
        // Integer formats
        assert!(SampleFormat::U8.is_integer());
        assert!(SampleFormat::I16.is_integer());
        assert!(SampleFormat::I32.is_integer());
        assert!(SampleFormat::U8p.is_integer());
        assert!(SampleFormat::I16p.is_integer());
        assert!(SampleFormat::I32p.is_integer());

        // Float formats
        assert!(!SampleFormat::F32.is_integer());
        assert!(!SampleFormat::F64.is_integer());
        assert!(!SampleFormat::F32p.is_integer());
        assert!(!SampleFormat::F64p.is_integer());
        assert!(!SampleFormat::Other(0).is_integer());
    }

    #[test]
    fn test_is_signed() {
        // Signed formats
        assert!(SampleFormat::I16.is_signed());
        assert!(SampleFormat::I32.is_signed());
        assert!(SampleFormat::F32.is_signed());
        assert!(SampleFormat::F64.is_signed());
        assert!(SampleFormat::I16p.is_signed());
        assert!(SampleFormat::I32p.is_signed());
        assert!(SampleFormat::F32p.is_signed());
        assert!(SampleFormat::F64p.is_signed());

        // Unsigned formats
        assert!(!SampleFormat::U8.is_signed());
        assert!(!SampleFormat::U8p.is_signed());
        assert!(!SampleFormat::Other(0).is_signed());
    }

    #[test]
    fn test_packed_equivalent() {
        // Planar to packed
        assert_eq!(SampleFormat::U8p.packed_equivalent(), SampleFormat::U8);
        assert_eq!(SampleFormat::I16p.packed_equivalent(), SampleFormat::I16);
        assert_eq!(SampleFormat::I32p.packed_equivalent(), SampleFormat::I32);
        assert_eq!(SampleFormat::F32p.packed_equivalent(), SampleFormat::F32);
        assert_eq!(SampleFormat::F64p.packed_equivalent(), SampleFormat::F64);

        // Already packed - returns itself
        assert_eq!(SampleFormat::U8.packed_equivalent(), SampleFormat::U8);
        assert_eq!(SampleFormat::I16.packed_equivalent(), SampleFormat::I16);
        assert_eq!(SampleFormat::F32.packed_equivalent(), SampleFormat::F32);

        // Unknown returns itself
        assert_eq!(
            SampleFormat::Other(42).packed_equivalent(),
            SampleFormat::Other(42)
        );
    }

    #[test]
    fn test_planar_equivalent() {
        // Packed to planar
        assert_eq!(SampleFormat::U8.planar_equivalent(), SampleFormat::U8p);
        assert_eq!(SampleFormat::I16.planar_equivalent(), SampleFormat::I16p);
        assert_eq!(SampleFormat::I32.planar_equivalent(), SampleFormat::I32p);
        assert_eq!(SampleFormat::F32.planar_equivalent(), SampleFormat::F32p);
        assert_eq!(SampleFormat::F64.planar_equivalent(), SampleFormat::F64p);

        // Already planar - returns itself
        assert_eq!(SampleFormat::U8p.planar_equivalent(), SampleFormat::U8p);
        assert_eq!(SampleFormat::I16p.planar_equivalent(), SampleFormat::I16p);
        assert_eq!(SampleFormat::F32p.planar_equivalent(), SampleFormat::F32p);

        // Unknown returns itself
        assert_eq!(
            SampleFormat::Other(42).planar_equivalent(),
            SampleFormat::Other(42)
        );
    }

    #[test]
    fn test_bit_depth() {
        assert_eq!(SampleFormat::U8.bit_depth(), 8);
        assert_eq!(SampleFormat::U8p.bit_depth(), 8);
        assert_eq!(SampleFormat::I16.bit_depth(), 16);
        assert_eq!(SampleFormat::I16p.bit_depth(), 16);
        assert_eq!(SampleFormat::I32.bit_depth(), 32);
        assert_eq!(SampleFormat::F32.bit_depth(), 32);
        assert_eq!(SampleFormat::F64.bit_depth(), 64);
        assert_eq!(SampleFormat::F64p.bit_depth(), 64);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", SampleFormat::F32), "flt");
        assert_eq!(format!("{}", SampleFormat::I16), "s16");
        assert_eq!(format!("{}", SampleFormat::F32p), "fltp");
        assert_eq!(format!("{}", SampleFormat::Other(123)), "unknown");
    }

    #[test]
    fn test_default() {
        assert_eq!(SampleFormat::default(), SampleFormat::F32);
    }

    #[test]
    fn test_debug() {
        assert_eq!(format!("{:?}", SampleFormat::F32), "F32");
        assert_eq!(format!("{:?}", SampleFormat::I16p), "I16p");
        assert_eq!(format!("{:?}", SampleFormat::Other(42)), "Other(42)");
    }

    #[test]
    fn test_equality_and_hash() {
        use std::collections::HashSet;

        assert_eq!(SampleFormat::F32, SampleFormat::F32);
        assert_ne!(SampleFormat::F32, SampleFormat::F32p);
        assert_eq!(SampleFormat::Other(1), SampleFormat::Other(1));
        assert_ne!(SampleFormat::Other(1), SampleFormat::Other(2));

        // Test Hash implementation
        let mut set = HashSet::new();
        set.insert(SampleFormat::F32);
        set.insert(SampleFormat::I16);
        assert!(set.contains(&SampleFormat::F32));
        assert!(!set.contains(&SampleFormat::F64));
    }

    #[test]
    fn test_copy() {
        let format = SampleFormat::F32;
        let copied = format;
        // Both original and copy are still usable (Copy semantics)
        assert_eq!(format, copied);
        assert_eq!(format.name(), copied.name());
    }

    #[test]
    fn test_round_trip_equivalents() {
        // Packed -> planar -> packed should return original
        let packed_formats = [
            SampleFormat::U8,
            SampleFormat::I16,
            SampleFormat::I32,
            SampleFormat::F32,
            SampleFormat::F64,
        ];
        for format in packed_formats {
            assert_eq!(format.planar_equivalent().packed_equivalent(), format);
        }

        // Planar -> packed -> planar should return original
        let planar_formats = [
            SampleFormat::U8p,
            SampleFormat::I16p,
            SampleFormat::I32p,
            SampleFormat::F32p,
            SampleFormat::F64p,
        ];
        for format in planar_formats {
            assert_eq!(format.packed_equivalent().planar_equivalent(), format);
        }
    }
}
