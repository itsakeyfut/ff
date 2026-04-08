//! Plane, channel, and typed sample access for [`AudioFrame`].

use crate::SampleFormat;

use super::AudioFrame;

impl AudioFrame {
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
    /// For packed formats (e.g. [`SampleFormat::F32`], [`SampleFormat::I16`]), this returns
    /// the interleaved sample bytes. For planar formats (e.g. [`SampleFormat::F32p`],
    /// [`SampleFormat::I16p`]), this returns an empty slice — use [`channel()`](Self::channel)
    /// or [`channel_as_f32()`](Self::channel_as_f32) to access individual channel planes instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// // Packed format - returns interleaved sample bytes
    /// let packed = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// assert_eq!(packed.data().len(), 1024 * 2 * 4);
    ///
    /// // Planar format - returns empty slice; use channel() instead
    /// let planar = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
    /// assert!(planar.data().is_empty());
    /// let left = planar.channel(0).unwrap();
    /// assert_eq!(left.len(), 1024 * 4);
    /// ```
    #[must_use]
    #[inline]
    pub fn data(&self) -> &[u8] {
        if self.format.is_packed() && self.planes.len() == 1 {
            &self.planes[0]
        } else {
            &[]
        }
    }

    /// Returns mutable access to the raw sample data.
    ///
    /// For packed formats, returns the interleaved sample bytes as a mutable slice.
    /// For planar formats, returns an empty mutable slice — use
    /// [`channel_mut()`](Self::channel_mut) to modify individual channel planes instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_format::{AudioFrame, SampleFormat};
    ///
    /// let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
    /// let data = frame.data_mut();
    /// data[0] = 128;
    /// ```
    #[must_use]
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        if self.format.is_packed() && self.planes.len() == 1 {
            &mut self.planes[0]
        } else {
            &mut []
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

        let bytes = self.data();
        if bytes.is_empty() {
            return None;
        }
        // SAFETY: We verified the format is F32, and the data was allocated
        // for F32 samples. Vec<u8> is aligned to at least 1 byte, but in practice
        // most allocators align to at least 8/16 bytes which is sufficient for f32.
        let ptr = bytes.as_ptr().cast::<f32>();
        let len = bytes.len() / std::mem::size_of::<f32>();
        Some(unsafe { std::slice::from_raw_parts(ptr, len) })
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

        let bytes = self.data_mut();
        if bytes.is_empty() {
            return None;
        }
        let ptr = bytes.as_mut_ptr().cast::<f32>();
        let len = bytes.len() / std::mem::size_of::<f32>();
        Some(unsafe { std::slice::from_raw_parts_mut(ptr, len) })
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

        let bytes = self.data();
        if bytes.is_empty() {
            return None;
        }
        let ptr = bytes.as_ptr().cast::<i16>();
        let len = bytes.len() / std::mem::size_of::<i16>();
        Some(unsafe { std::slice::from_raw_parts(ptr, len) })
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

        let bytes = self.data_mut();
        if bytes.is_empty() {
            return None;
        }
        let ptr = bytes.as_mut_ptr().cast::<i16>();
        let len = bytes.len() / std::mem::size_of::<i16>();
        Some(unsafe { std::slice::from_raw_parts_mut(ptr, len) })
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
            // SAFETY: We verified the format is F32p, so each channel plane was
            // allocated for F32 samples. Alignment is sufficient as above.
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
            // SAFETY: Same as channel_as_f32, plus we hold &mut self so no
            // aliasing can occur.
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
            // SAFETY: We verified the format is I16p, so each channel plane was
            // allocated for I16 samples. Alignment is sufficient as above.
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
            // SAFETY: Same as channel_as_i16, plus we hold &mut self so no
            // aliasing can occur.
            unsafe { std::slice::from_raw_parts_mut(ptr, len) }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ==========================================================================
    // Plane Access Tests
    // ==========================================================================

    #[test]
    fn plane_access_packed_should_have_one_plane() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(frame.plane(0).is_some());
        assert!(frame.plane(1).is_none());
    }

    #[test]
    fn plane_access_planar_should_have_one_plane_per_channel() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        assert!(frame.plane(0).is_some());
        assert!(frame.plane(1).is_some());
        assert!(frame.plane(2).is_none());
    }

    #[test]
    fn plane_mut_should_allow_byte_modification() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        if let Some(data) = frame.plane_mut(0) {
            data[0] = 255;
        }
        assert_eq!(frame.plane(0).unwrap()[0], 255);
    }

    #[test]
    fn channel_access_planar_should_return_per_channel_bytes() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        let left = frame.channel(0).unwrap();
        let right = frame.channel(1).unwrap();
        assert_eq!(left.len(), 1024 * 4);
        assert_eq!(right.len(), 1024 * 4);
    }

    // ==========================================================================
    // Contiguous Data Access Tests
    // ==========================================================================

    #[test]
    fn data_packed_should_return_sample_bytes() {
        let frame = AudioFrame::empty(4, 2, 48000, SampleFormat::F32).unwrap();
        // 4 samples * 2 channels * 4 bytes = 32
        assert_eq!(frame.data().len(), 32);
    }

    #[test]
    fn data_packed_should_return_all_interleaved_bytes() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(!frame.data().is_empty());
        assert_eq!(frame.data().len(), 1024 * 2 * 4);
    }

    #[test]
    fn data_planar_should_return_empty_slice() {
        let frame = AudioFrame::empty(4, 2, 48000, SampleFormat::F32p).unwrap();
        assert!(frame.data().is_empty());
    }

    #[test]
    fn data_planar_f32p_should_return_empty_slice() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        assert!(frame.data().is_empty());
    }

    #[test]
    fn data_mut_packed_should_allow_mutation() {
        let mut frame = AudioFrame::empty(4, 1, 48000, SampleFormat::I16).unwrap();
        frame.data_mut()[0] = 0x42;
        frame.data_mut()[1] = 0x00;
        assert_eq!(frame.data()[0], 0x42);
    }

    #[test]
    fn data_mut_planar_should_return_empty_slice() {
        let mut frame = AudioFrame::empty(4, 2, 48000, SampleFormat::I16p).unwrap();
        assert!(frame.data_mut().is_empty());
    }

    #[test]
    fn data_mut_packed_should_persist_change() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        frame.data_mut()[0] = 123;
        assert_eq!(frame.data()[0], 123);
    }

    // ==========================================================================
    // Typed Access Tests
    // ==========================================================================

    #[test]
    fn as_f32_packed_should_return_typed_slice() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        let samples = frame.as_f32().unwrap();
        assert_eq!(samples.len(), 1024 * 2);
    }

    #[test]
    fn as_f32_wrong_format_should_return_none() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16).unwrap();
        assert!(frame.as_f32().is_none());
    }

    #[test]
    fn as_f32_mut_should_allow_typed_modification() {
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
    fn as_i16_packed_should_return_typed_slice() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16).unwrap();
        let samples = frame.as_i16().unwrap();
        assert_eq!(samples.len(), 1024 * 2);
    }

    #[test]
    fn as_i16_wrong_format_should_return_none() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(frame.as_i16().is_none());
    }

    #[test]
    fn channel_as_f32_planar_should_return_typed_slice_per_channel() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        let left = frame.channel_as_f32(0).unwrap();
        let right = frame.channel_as_f32(1).unwrap();
        assert_eq!(left.len(), 1024);
        assert_eq!(right.len(), 1024);
    }

    #[test]
    fn channel_as_f32_packed_format_should_return_none() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32).unwrap();
        assert!(frame.channel_as_f32(0).is_none()); // F32 is packed, not F32p
    }

    #[test]
    fn channel_as_f32_mut_should_allow_typed_channel_modification() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::F32p).unwrap();
        if let Some(left) = frame.channel_as_f32_mut(0) {
            left[0] = 0.5;
        }
        assert!((frame.channel_as_f32(0).unwrap()[0] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn channel_as_i16_planar_should_return_typed_slice() {
        let frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16p).unwrap();
        let left = frame.channel_as_i16(0).unwrap();
        assert_eq!(left.len(), 1024);
    }

    #[test]
    fn channel_as_i16_mut_should_allow_typed_channel_modification() {
        let mut frame = AudioFrame::empty(1024, 2, 48000, SampleFormat::I16p).unwrap();
        if let Some(left) = frame.channel_as_i16_mut(0) {
            left[0] = 32767;
        }
        assert_eq!(frame.channel_as_i16(0).unwrap()[0], 32767);
    }

    // ==========================================================================
    // All Sample Formats Tests
    // ==========================================================================

    #[test]
    fn all_packed_formats_should_have_one_plane_and_nonempty_data() {
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
            assert!(!frame.data().is_empty());
        }
    }

    #[test]
    fn all_planar_formats_should_have_one_plane_per_channel_and_empty_data() {
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
            assert!(frame.data().is_empty());
            assert!(frame.channel(0).is_some());
            assert!(frame.channel(1).is_some());
        }
    }

    #[test]
    fn mono_planar_should_have_one_plane_with_correct_size() {
        let frame = AudioFrame::empty(1024, 1, 48000, SampleFormat::F32p).unwrap();
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(1024 * 4));
    }

    #[test]
    fn surround_51_planar_should_have_six_planes() {
        let frame = AudioFrame::empty(1024, 6, 48000, SampleFormat::F32p).unwrap();
        assert_eq!(frame.num_planes(), 6);
        for i in 0..6 {
            assert!(frame.plane(i).is_some());
        }
        assert!(frame.plane(6).is_none());
    }
}
