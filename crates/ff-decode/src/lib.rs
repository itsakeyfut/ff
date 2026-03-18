//! # ff-decode
//!
//! Video and audio decoding - the Rust way.
//!
//! This crate provides frame-by-frame video/audio decoding, efficient seeking,
//! and thumbnail generation. It completely hides `FFmpeg` internals and provides
//! a safe, ergonomic Rust API.
//!
//! ## Features
//!
//! - **Video Decoding**: Frame-by-frame decoding with Iterator pattern
//! - **Audio Decoding**: Sample-level audio extraction
//! - **Seeking**: Fast keyframe and exact seeking without file re-open
//! - **Thumbnails**: Efficient thumbnail generation for timelines
//! - **Hardware Acceleration**: Optional NVDEC, QSV, AMF, `VideoToolbox`, VAAPI support
//! - **Frame Pooling**: Memory reuse for reduced allocation overhead
//!
//! ## Usage
//!
//! ### Video Decoding
//!
//! ```ignore
//! use ff_decode::{VideoDecoder, SeekMode};
//! use ff_format::PixelFormat;
//! use std::time::Duration;
//!
//! // Open a video file and create decoder
//! let mut decoder = VideoDecoder::open("video.mp4")?
//!     .output_format(PixelFormat::Rgba)
//!     .build()?;
//!
//! // Get basic info
//! println!("Duration: {:?}", decoder.duration());
//! println!("Resolution: {}x{}", decoder.width(), decoder.height());
//!
//! // Decode frames sequentially
//! for frame in decoder.frames().take(100) {
//!     let frame = frame?;
//!     println!("Frame at {:?}", frame.timestamp().as_duration());
//! }
//!
//! // Seek to specific position
//! decoder.seek(Duration::from_secs(30), SeekMode::Keyframe)?;
//! ```
//!
//! ### Audio Decoding
//!
//! ```ignore
//! use ff_decode::AudioDecoder;
//! use ff_format::SampleFormat;
//!
//! let mut decoder = AudioDecoder::open("audio.mp3")?
//!     .output_format(SampleFormat::F32)
//!     .output_sample_rate(48000)
//!     .build()?;
//!
//! // Decode all audio samples
//! for frame in decoder.frames().take(100) {
//!     let frame = frame?;
//!     println!("Audio frame with {} samples", frame.samples());
//! }
//! ```
//!
//! ### Hardware Acceleration (Video)
//!
//! ```ignore
//! use ff_decode::{VideoDecoder, HardwareAccel};
//!
//! let decoder = VideoDecoder::open("video.mp4")?
//!     .hardware_accel(HardwareAccel::Auto)  // Auto-detect GPU
//!     .build()?;
//! ```
//!
//! ### Frame Pooling (Video)
//!
//! ```ignore
//! use ff_decode::{VideoDecoder, FramePool};
//! use std::sync::Arc;
//!
//! // Use a frame pool for memory reuse
//! let pool: Arc<dyn FramePool> = create_frame_pool(32);
//! let decoder = VideoDecoder::open("video.mp4")?
//!     .frame_pool(pool)
//!     .build()?;
//! ```
//!
//! ## Module Structure
//!
//! - [`audio`] - Audio decoder for extracting audio frames
//! - [`video`] - Video decoder for extracting video frames
//! - [`error`] - Error types for decoding operations
//! - Frame pool types (`FramePool`, `PooledBuffer`, `VecPool`) are provided by `ff-common`
//!
//! ## Re-exports
//!
//! This crate re-exports commonly used types from `ff-format` for convenience.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

// Module declarations
pub mod audio;
pub mod error;
pub mod image;
pub mod video;

// Re-exports for convenience
pub use audio::{AudioDecoder, AudioDecoderBuilder};
pub use error::DecodeError;
pub use ff_common::{FramePool, PooledBuffer};
pub use image::{ImageDecoder, ImageDecoderBuilder};
pub use video::{VideoDecoder, VideoDecoderBuilder};

/// Seek mode for positioning the decoder.
///
/// This enum determines how seeking is performed when navigating
/// through a media file.
///
/// # Performance Considerations
///
/// - [`Keyframe`](Self::Keyframe) is fastest but may land slightly before or after the target
/// - [`Exact`](Self::Exact) is slower but guarantees landing on the exact frame
/// - [`Backward`](Self::Backward) is useful for editing workflows where the previous keyframe is needed
///
/// # Examples
///
/// ```
/// use ff_decode::SeekMode;
///
/// // Default is Keyframe mode
/// let mode = SeekMode::default();
/// assert_eq!(mode, SeekMode::Keyframe);
///
/// // Use exact mode for frame-accurate positioning
/// let exact = SeekMode::Exact;
/// assert_eq!(format!("{:?}", exact), "Exact");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SeekMode {
    /// Seek to nearest keyframe (fast, may have small offset).
    ///
    /// This mode seeks to the closest keyframe to the target position.
    /// It's the fastest option but the actual position may differ from
    /// the requested position by up to the GOP (Group of Pictures) size.
    #[default]
    Keyframe = 0,

    /// Seek to exact frame (slower but precise).
    ///
    /// This mode first seeks to the previous keyframe, then decodes
    /// frames until reaching the exact target position. This guarantees
    /// frame-accurate positioning but is slower, especially for long GOPs.
    Exact = 1,

    /// Seek to keyframe at or before the target position.
    ///
    /// Similar to [`Keyframe`](Self::Keyframe), but guarantees the resulting
    /// position is at or before the target. Useful for editing workflows
    /// where you need to start decoding before a specific point.
    Backward = 2,
}

/// Hardware acceleration configuration.
///
/// This enum specifies which hardware acceleration method to use for
/// video decoding. Hardware acceleration can significantly improve
/// decoding performance, especially for high-resolution content.
///
/// # Platform Support
///
/// | Mode | Platform | GPU Required |
/// |------|----------|--------------|
/// | [`Nvdec`](Self::Nvdec) | Windows/Linux | NVIDIA |
/// | [`Qsv`](Self::Qsv) | Windows/Linux | Intel |
/// | [`Amf`](Self::Amf) | Windows/Linux | AMD |
/// | [`VideoToolbox`](Self::VideoToolbox) | macOS/iOS | Any |
/// | [`Vaapi`](Self::Vaapi) | Linux | Various |
///
/// # Fallback Behavior
///
/// When [`Auto`](Self::Auto) is used, the decoder will try available
/// accelerators in order of preference and fall back to software
/// decoding if none are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HardwareAccel {
    /// Automatically detect and use available hardware.
    ///
    /// The decoder will probe for available hardware accelerators
    /// and use the best one available. Falls back to software decoding
    /// if no hardware acceleration is available.
    #[default]
    Auto,

    /// Disable hardware acceleration (CPU only).
    ///
    /// Forces software decoding using the CPU. This may be useful for
    /// debugging, consistency, or when hardware acceleration causes issues.
    None,

    /// NVIDIA NVDEC.
    ///
    /// Uses NVIDIA's dedicated video decoding hardware. Supports most
    /// common codecs including H.264, H.265, VP9, and AV1 (on newer GPUs).
    /// Requires an NVIDIA GPU with NVDEC support.
    Nvdec,

    /// Intel Quick Sync Video.
    ///
    /// Uses Intel's integrated GPU video engine. Available on most
    /// Intel CPUs with integrated graphics. Supports H.264, H.265,
    /// VP9, and AV1 (on newer platforms).
    Qsv,

    /// AMD Advanced Media Framework.
    ///
    /// Uses AMD's dedicated video decoding hardware. Available on AMD
    /// GPUs and APUs. Supports H.264, H.265, and VP9.
    Amf,

    /// Apple `VideoToolbox`.
    ///
    /// Uses Apple's hardware video decoding on macOS and iOS. Works with
    /// both Intel and Apple Silicon Macs. Supports H.264, H.265, and `ProRes`.
    VideoToolbox,

    /// Video Acceleration API (Linux).
    ///
    /// A Linux-specific API that provides hardware-accelerated video
    /// decoding across different GPU vendors. Widely supported on
    /// Intel, AMD, and NVIDIA GPUs on Linux.
    Vaapi,
}

impl HardwareAccel {
    /// Returns `true` if this represents an enabled hardware accelerator.
    ///
    /// Returns `false` for [`None`](Self::None) and [`Auto`](Self::Auto).
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::HardwareAccel;
    ///
    /// assert!(!HardwareAccel::Auto.is_specific());
    /// assert!(!HardwareAccel::None.is_specific());
    /// assert!(HardwareAccel::Nvdec.is_specific());
    /// assert!(HardwareAccel::Qsv.is_specific());
    /// ```
    #[must_use]
    pub const fn is_specific(&self) -> bool {
        !matches!(self, Self::Auto | Self::None)
    }

    /// Returns the name of the hardware accelerator.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_decode::HardwareAccel;
    ///
    /// assert_eq!(HardwareAccel::Auto.name(), "auto");
    /// assert_eq!(HardwareAccel::Nvdec.name(), "nvdec");
    /// assert_eq!(HardwareAccel::VideoToolbox.name(), "videotoolbox");
    /// ```
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Nvdec => "nvdec",
            Self::Qsv => "qsv",
            Self::Amf => "amf",
            Self::VideoToolbox => "videotoolbox",
            Self::Vaapi => "vaapi",
        }
    }
}

/// Prelude module for convenient imports.
///
/// This module re-exports all commonly used types:
///
/// ```ignore
/// use ff_decode::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        AudioDecoder, AudioDecoderBuilder, DecodeError, FramePool, HardwareAccel, ImageDecoder,
        ImageDecoderBuilder, PooledBuffer, SeekMode, VideoDecoder, VideoDecoderBuilder,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seek_mode_default() {
        let mode = SeekMode::default();
        assert_eq!(mode, SeekMode::Keyframe);
    }

    #[test]
    fn test_hardware_accel_default() {
        let accel = HardwareAccel::default();
        assert_eq!(accel, HardwareAccel::Auto);
    }

    #[test]
    fn test_hardware_accel_is_specific() {
        assert!(!HardwareAccel::Auto.is_specific());
        assert!(!HardwareAccel::None.is_specific());
        assert!(HardwareAccel::Nvdec.is_specific());
        assert!(HardwareAccel::Qsv.is_specific());
        assert!(HardwareAccel::Amf.is_specific());
        assert!(HardwareAccel::VideoToolbox.is_specific());
        assert!(HardwareAccel::Vaapi.is_specific());
    }

    #[test]
    fn test_hardware_accel_name() {
        assert_eq!(HardwareAccel::Auto.name(), "auto");
        assert_eq!(HardwareAccel::None.name(), "none");
        assert_eq!(HardwareAccel::Nvdec.name(), "nvdec");
        assert_eq!(HardwareAccel::Qsv.name(), "qsv");
        assert_eq!(HardwareAccel::Amf.name(), "amf");
        assert_eq!(HardwareAccel::VideoToolbox.name(), "videotoolbox");
        assert_eq!(HardwareAccel::Vaapi.name(), "vaapi");
    }

    #[test]
    fn test_seek_mode_variants() {
        let modes = [SeekMode::Keyframe, SeekMode::Exact, SeekMode::Backward];
        for mode in modes {
            // Ensure all variants are accessible
            let _ = format!("{mode:?}");
        }
    }

    #[test]
    fn test_hardware_accel_variants() {
        let accels = [
            HardwareAccel::Auto,
            HardwareAccel::None,
            HardwareAccel::Nvdec,
            HardwareAccel::Qsv,
            HardwareAccel::Amf,
            HardwareAccel::VideoToolbox,
            HardwareAccel::Vaapi,
        ];
        for accel in accels {
            // Ensure all variants are accessible
            let _ = format!("{accel:?}");
        }
    }

    #[test]
    fn test_decode_error_display() {
        use std::path::PathBuf;

        let error = DecodeError::FileNotFound {
            path: PathBuf::from("/path/to/video.mp4"),
        };
        assert!(error.to_string().contains("File not found"));

        let error = DecodeError::NoVideoStream {
            path: PathBuf::from("/path/to/audio.mp3"),
        };
        assert!(error.to_string().contains("No video stream"));

        let error = DecodeError::UnsupportedCodec {
            codec: "unknown_codec".to_string(),
        };
        assert!(error.to_string().contains("Codec not supported"));

        let error = DecodeError::EndOfStream;
        assert_eq!(error.to_string(), "End of stream");
    }

    #[test]
    fn test_prelude_imports() {
        // Verify prelude exports all expected types
        use crate::prelude::*;

        let _mode: SeekMode = SeekMode::default();
        let _accel: HardwareAccel = HardwareAccel::default();

        // Video builder can be created
        let _video_builder: VideoDecoderBuilder = VideoDecoder::open("test.mp4");

        // Audio builder can be created
        let _audio_builder: AudioDecoderBuilder = AudioDecoder::open("test.mp3");
    }
}
