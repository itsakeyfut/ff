//! Hardware acceleration and threading options for [`VideoDecoderBuilder`].

use std::sync::Arc;

use ff_common::FramePool;

use crate::HardwareAccel;

use super::VideoDecoderBuilder;

impl VideoDecoderBuilder {
    /// Sets the hardware acceleration mode.
    ///
    /// Hardware acceleration can significantly improve decoding performance,
    /// especially for high-resolution video (4K and above).
    ///
    /// # Available Modes
    ///
    /// - [`HardwareAccel::Auto`] - Automatically detect and use available hardware (default)
    /// - [`HardwareAccel::None`] - Disable hardware acceleration (CPU only)
    /// - [`HardwareAccel::Nvdec`] - NVIDIA NVDEC (requires NVIDIA GPU)
    /// - [`HardwareAccel::Qsv`] - Intel Quick Sync Video
    /// - [`HardwareAccel::Amf`] - AMD Advanced Media Framework
    /// - [`HardwareAccel::VideoToolbox`] - Apple `VideoToolbox` (macOS/iOS)
    /// - [`HardwareAccel::Vaapi`] - VA-API (Linux)
    ///
    /// # Fallback Behavior
    ///
    /// If the requested hardware accelerator is unavailable, the decoder
    /// will fall back to software decoding unless
    /// [`DecodeError::HwAccelUnavailable`] is explicitly requested.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::{VideoDecoder, HardwareAccel};
    ///
    /// // Use NVIDIA NVDEC if available
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .hardware_accel(HardwareAccel::Nvdec)
    ///     .build()?;
    ///
    /// // Force CPU decoding
    /// let cpu_decoder = Decoder::open("video.mp4")?
    ///     .hardware_accel(HardwareAccel::None)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn hardware_accel(mut self, accel: HardwareAccel) -> Self {
        self.hardware_accel = accel;
        self
    }

    /// Sets the number of decoding threads.
    ///
    /// More threads can improve decoding throughput, especially for
    /// high-resolution videos or codecs that support parallel decoding.
    ///
    /// # Thread Count Values
    ///
    /// - `0` - Auto-detect based on CPU cores (default)
    /// - `1` - Single-threaded decoding
    /// - `N` - Use N threads for decoding
    ///
    /// # Performance Notes
    ///
    /// - H.264/H.265: Benefit significantly from multi-threading
    /// - VP9: Good parallel decoding support
    /// - `ProRes`: Limited threading benefit
    ///
    /// Setting too many threads may increase memory usage without
    /// proportional performance gains.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Use 4 threads for decoding
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .thread_count(4)
    ///     .build()?;
    ///
    /// // Single-threaded for minimal memory
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .thread_count(1)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn thread_count(mut self, count: usize) -> Self {
        self.thread_count = count;
        self
    }

    /// Sets a frame pool for memory reuse.
    ///
    /// Using a frame pool can significantly reduce allocation overhead
    /// during continuous video playback by reusing frame buffers.
    ///
    /// # Memory Management
    ///
    /// When a frame pool is set:
    /// - Decoded frames attempt to acquire buffers from the pool
    /// - When frames are dropped, their buffers are returned to the pool
    /// - If the pool is exhausted, new buffers are allocated normally
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::{VideoDecoder, FramePool, PooledBuffer};
    /// use std::sync::{Arc, Mutex};
    ///
    /// // Create a simple frame pool
    /// struct SimplePool {
    ///     buffers: Mutex<Vec<Vec<u8>>>,
    /// }
    ///
    /// impl FramePool for SimplePool {
    ///     fn acquire(&self, size: usize) -> Option<PooledBuffer> {
    ///         // Implementation...
    ///         None
    ///     }
    /// }
    ///
    /// let pool = Arc::new(SimplePool {
    ///     buffers: Mutex::new(vec![]),
    /// });
    ///
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .frame_pool(pool)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn frame_pool(mut self, pool: Arc<dyn FramePool>) -> Self {
        self.frame_pool = Some(pool);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builder_hardware_accel_should_override_default() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4"))
            .hardware_accel(HardwareAccel::Nvdec);

        assert_eq!(builder.get_hardware_accel(), HardwareAccel::Nvdec);
    }

    #[test]
    fn builder_hardware_accel_none_should_disable_hw() {
        let builder =
            VideoDecoderBuilder::new(PathBuf::from("test.mp4")).hardware_accel(HardwareAccel::None);

        assert_eq!(builder.get_hardware_accel(), HardwareAccel::None);
    }

    #[test]
    fn builder_thread_count_should_set_thread_count() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).thread_count(8);

        assert_eq!(builder.get_thread_count(), 8);
    }

    #[test]
    fn builder_thread_count_single_thread_should_be_accepted() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).thread_count(1);

        assert_eq!(builder.get_thread_count(), 1);
    }

    #[test]
    fn builder_thread_count_zero_means_auto() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).thread_count(0);

        assert_eq!(builder.get_thread_count(), 0);
    }
}
