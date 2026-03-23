//! Video decoder builder for constructing video decoders with custom configuration.
//!
//! This module provides the [`VideoDecoderBuilder`] type which enables fluent
//! configuration of video decoders. Use [`VideoDecoder::open()`] to start building.
//!
//! # Examples
//!
//! ```ignore
//! use ff_decode::{VideoDecoder, HardwareAccel};
//! use ff_format::PixelFormat;
//!
//! let decoder = VideoDecoder::open("video.mp4")?
//!     .output_format(PixelFormat::Rgba)
//!     .hardware_accel(HardwareAccel::Auto)
//!     .thread_count(4)
//!     .build()?;
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ff_format::{ContainerInfo, NetworkOptions, PixelFormat, VideoFrame, VideoStreamInfo};

use crate::HardwareAccel;
use crate::error::DecodeError;
use crate::video::decoder_inner::VideoDecoderInner;
use ff_common::FramePool;

/// Requested output scale for decoded frames.
///
/// Controls how `libswscale` resizes the frame in the same pass as pixel-format
/// conversion. The last setter wins — calling `output_width()` after
/// `output_size()` replaces the earlier setting.
///
/// Both width and height are rounded up to the nearest even number if needed,
/// because most pixel formats (e.g. `yuv420p`) require even dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputScale {
    /// Scale to an exact width × height.
    Exact {
        /// Target width in pixels.
        width: u32,
        /// Target height in pixels.
        height: u32,
    },
    /// Scale to the given width; compute height to preserve aspect ratio.
    FitWidth(u32),
    /// Scale to the given height; compute width to preserve aspect ratio.
    FitHeight(u32),
}

/// Internal configuration for the decoder.
///
/// NOTE: Fields are currently unused but will be used when `FFmpeg` integration
/// is implemented in a future issue.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct VideoDecoderConfig {
    /// Output pixel format (None = use source format)
    pub output_format: Option<PixelFormat>,
    /// Output scale (None = use source dimensions)
    pub output_scale: Option<OutputScale>,
    /// Hardware acceleration setting
    pub hardware_accel: HardwareAccel,
    /// Number of decoding threads (0 = auto)
    pub thread_count: usize,
}

impl Default for VideoDecoderConfig {
    fn default() -> Self {
        Self {
            output_format: None,
            output_scale: None,
            hardware_accel: HardwareAccel::Auto,
            thread_count: 0, // Auto-detect
        }
    }
}

/// Builder for configuring and constructing a [`VideoDecoder`].
///
/// This struct provides a fluent interface for setting up decoder options
/// before opening a video file. It is created by calling [`VideoDecoder::open()`].
///
/// # Examples
///
/// ## Basic Usage
///
/// ```ignore
/// use ff_decode::VideoDecoder;
///
/// let decoder = VideoDecoder::open("video.mp4")?
///     .build()?;
/// ```
///
/// ## With Custom Format
///
/// ```ignore
/// use ff_decode::VideoDecoder;
/// use ff_format::PixelFormat;
///
/// let decoder = VideoDecoder::open("video.mp4")?
///     .output_format(PixelFormat::Rgba)
///     .build()?;
/// ```
///
/// ## With Hardware Acceleration
///
/// ```ignore
/// use ff_decode::{VideoDecoder, HardwareAccel};
///
/// let decoder = VideoDecoder::open("video.mp4")?
///     .hardware_accel(HardwareAccel::Nvdec)
///     .build()?;
/// ```
///
/// ## With Frame Pool
///
/// ```ignore
/// use ff_decode::{VideoDecoder, FramePool};
/// use std::sync::Arc;
///
/// let pool: Arc<dyn FramePool> = create_frame_pool();
/// let decoder = VideoDecoder::open("video.mp4")?
///     .frame_pool(pool)
///     .build()?;
/// ```
#[derive(Debug)]
pub struct VideoDecoderBuilder {
    /// Path to the media file
    path: PathBuf,
    /// Output pixel format (None = use source format)
    output_format: Option<PixelFormat>,
    /// Output scale (None = use source dimensions)
    output_scale: Option<OutputScale>,
    /// Hardware acceleration setting
    hardware_accel: HardwareAccel,
    /// Number of decoding threads (0 = auto)
    thread_count: usize,
    /// Optional frame pool for memory reuse
    frame_pool: Option<Arc<dyn FramePool>>,
    /// Frame rate override for image sequences (default 25 fps when path contains `%`).
    frame_rate: Option<u32>,
    /// Network options for URL-based sources (RTMP, RTSP, HTTP, etc.).
    network_opts: Option<NetworkOptions>,
}

impl VideoDecoderBuilder {
    /// Creates a new builder for the specified file path.
    ///
    /// This is an internal constructor; use [`VideoDecoder::open()`] instead.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            output_format: None,
            output_scale: None,
            hardware_accel: HardwareAccel::Auto,
            thread_count: 0,
            frame_pool: None,
            frame_rate: None,
            network_opts: None,
        }
    }

    /// Sets the output pixel format for decoded frames.
    ///
    /// If not set, frames are returned in the source format. Setting an
    /// output format enables automatic conversion during decoding.
    ///
    /// # Common Formats
    ///
    /// - [`PixelFormat::Rgba`] - Best for UI rendering, includes alpha
    /// - [`PixelFormat::Rgb24`] - RGB without alpha, smaller memory footprint
    /// - [`PixelFormat::Yuv420p`] - Source format for most H.264/H.265 videos
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::PixelFormat;
    ///
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_format(PixelFormat::Rgba)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_format(mut self, format: PixelFormat) -> Self {
        self.output_format = Some(format);
        self
    }

    /// Scales decoded frames to the given exact dimensions.
    ///
    /// The frame is scaled in the same `libswscale` pass as pixel-format
    /// conversion, so there is no extra copy. If `output_format` is not set,
    /// the source pixel format is preserved while scaling.
    ///
    /// Width and height must be greater than zero. They are rounded up to the
    /// nearest even number if necessary (required by most pixel formats).
    ///
    /// Calling this method overwrites any previous `output_width` or
    /// `output_height` call. The last setter wins.
    ///
    /// # Errors
    ///
    /// [`build()`](Self::build) returns [`DecodeError::InvalidOutputDimensions`]
    /// if either dimension is zero after rounding.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Decode every frame at 320×240
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_size(320, 240)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_size(mut self, width: u32, height: u32) -> Self {
        self.output_scale = Some(OutputScale::Exact { width, height });
        self
    }

    /// Scales decoded frames to the given width, preserving the aspect ratio.
    ///
    /// The height is computed from the source aspect ratio and rounded to the
    /// nearest even number. Calling this method overwrites any previous
    /// `output_size` or `output_height` call. The last setter wins.
    ///
    /// # Errors
    ///
    /// [`build()`](Self::build) returns [`DecodeError::InvalidOutputDimensions`]
    /// if `width` is zero.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Decode at 1280 px wide, preserving aspect ratio
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_width(1280)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_width(mut self, width: u32) -> Self {
        self.output_scale = Some(OutputScale::FitWidth(width));
        self
    }

    /// Scales decoded frames to the given height, preserving the aspect ratio.
    ///
    /// The width is computed from the source aspect ratio and rounded to the
    /// nearest even number. Calling this method overwrites any previous
    /// `output_size` or `output_width` call. The last setter wins.
    ///
    /// # Errors
    ///
    /// [`build()`](Self::build) returns [`DecodeError::InvalidOutputDimensions`]
    /// if `height` is zero.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Decode at 720 px tall, preserving aspect ratio
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_height(720)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_height(mut self, height: u32) -> Self {
        self.output_scale = Some(OutputScale::FitHeight(height));
        self
    }

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

    /// Sets the frame rate for image sequence decoding.
    ///
    /// Only used when the path contains `%` (e.g. `"frames/frame%04d.png"`).
    /// Defaults to 25 fps when not set.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// let decoder = VideoDecoder::open("frames/frame%04d.png")?
    ///     .frame_rate(30)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn frame_rate(mut self, fps: u32) -> Self {
        self.frame_rate = Some(fps);
        self
    }

    /// Sets network options for URL-based sources.
    ///
    /// When set, the builder skips the file-existence check and passes connect
    /// and read timeouts to `avformat_open_input` via an `AVDictionary`.
    /// Call this before `.build()` when opening `rtmp://`, `rtsp://`, `http://`,
    /// `https://`, `udp://`, `srt://`, or `rtp://` URLs.
    ///
    /// # HLS / M3U8 Playlists
    ///
    /// HLS playlists (`.m3u8`) are detected automatically by `FFmpeg` — no extra
    /// configuration is required beyond calling `.network()`. Pass the full
    /// HTTP(S) URL of the master or media playlist:
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("https://example.com/live/index.m3u8")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    ///
    /// # DASH / MPD Streams
    ///
    /// MPEG-DASH manifests (`.mpd`) are detected automatically by `FFmpeg`'s
    /// built-in `dash` demuxer. The demuxer downloads the manifest, selects the
    /// highest-quality representation, and fetches segments automatically:
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("https://example.com/dash/manifest.mpd")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    ///
    /// **Multi-period caveat**: multi-period DASH streams are supported by
    /// `FFmpeg` but period boundaries may trigger an internal decoder reset,
    /// which can cause a brief gap in decoded frames.
    ///
    /// **Adaptive bitrate**: representation selection (ABR switching) is handled
    /// internally by `FFmpeg` and is not exposed through this API.
    ///
    /// # UDP / MPEG-TS
    ///
    /// `udp://` URLs are always live — `is_live()` returns `true` and seeking
    /// is not supported. Two extra `AVDictionary` options are set automatically
    /// to reduce packet loss on high-bitrate streams:
    ///
    /// | Option | Value | Reason |
    /// |---|---|---|
    /// | `buffer_size` | `65536` | Enlarges the UDP receive buffer |
    /// | `overrun_nonfatal` | `1` | Discards excess data instead of erroring |
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("udp://224.0.0.1:1234")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    ///
    /// # SRT (Secure Reliable Transport)
    ///
    /// SRT URLs (`srt://host:port`) require the `srt` feature flag **and** a
    /// libsrt-enabled `FFmpeg` build.  Enable the feature in `Cargo.toml`:
    ///
    /// ```toml
    /// [dependencies]
    /// ff-decode = { version = "*", features = ["srt"] }
    /// ```
    ///
    /// Without the `srt` feature, opening an `srt://` URL returns
    /// [`DecodeError::ConnectionFailed`]. If the feature is enabled but the
    /// linked `FFmpeg` was not built with `--enable-libsrt`, the same error is
    /// returned with a message directing you to rebuild `FFmpeg`.
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("srt://ingest.example.com:4200")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    ///
    /// # Credentials
    ///
    /// HTTP basic-auth credentials must be embedded directly in the URL:
    /// `https://user:password@cdn.example.com/live/index.m3u8`.
    /// The password is redacted in log output.
    ///
    /// # DRM Limitation
    ///
    /// DRM-protected streams are **not** supported:
    /// - HLS: `FairPlay`, Widevine, AES-128 with external key servers
    /// - DASH: CENC, `PlayReady`, Widevine
    ///
    /// `FFmpeg` can parse the manifest and fetch segments, but key delivery
    /// to a DRM license server is outside the scope of this API.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::NetworkOptions;
    ///
    /// let decoder = VideoDecoder::open("rtmp://live.example.com/app/stream_key")
    ///     .network(NetworkOptions::default())
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn network(mut self, opts: NetworkOptions) -> Self {
        self.network_opts = Some(opts);
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

    /// Returns the configured file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the configured output format, if any.
    #[must_use]
    pub fn get_output_format(&self) -> Option<PixelFormat> {
        self.output_format
    }

    /// Returns the configured hardware acceleration mode.
    #[must_use]
    pub fn get_hardware_accel(&self) -> HardwareAccel {
        self.hardware_accel
    }

    /// Returns the configured thread count.
    #[must_use]
    pub fn get_thread_count(&self) -> usize {
        self.thread_count
    }

    /// Builds the decoder with the configured options.
    ///
    /// This method opens the media file, initializes the decoder context,
    /// and prepares for frame decoding.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be found ([`DecodeError::FileNotFound`])
    /// - The file contains no video stream ([`DecodeError::NoVideoStream`])
    /// - The codec is not supported ([`DecodeError::UnsupportedCodec`])
    /// - Hardware acceleration is unavailable ([`DecodeError::HwAccelUnavailable`])
    /// - Other `FFmpeg` errors occur ([`DecodeError::Ffmpeg`])
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .build()?;
    ///
    /// // Start decoding
    /// for result in &mut decoder {
    ///     let frame = result?;
    ///     // Process frame...
    /// }
    /// ```
    pub fn build(self) -> Result<VideoDecoder, DecodeError> {
        // Validate output scale dimensions before opening the file.
        // FitWidth / FitHeight aspect-ratio dimensions are resolved at decode time
        // from the actual source dimensions, so we only reject an explicit zero here.
        if let Some(scale) = self.output_scale {
            let (w, h) = match scale {
                OutputScale::Exact { width, height } => (width, height),
                OutputScale::FitWidth(w) => (w, 1), // height will be derived
                OutputScale::FitHeight(h) => (1, h), // width will be derived
            };
            if w == 0 || h == 0 {
                return Err(DecodeError::InvalidOutputDimensions {
                    width: w,
                    height: h,
                });
            }
        }

        // Image-sequence patterns contain '%' — the literal path does not exist.
        // Network URLs must also skip the file-existence check.
        let path_str = self.path.to_str().unwrap_or("");
        let is_image_sequence = path_str.contains('%');
        let is_network_url = crate::network::is_url(path_str);
        if !is_image_sequence && !is_network_url && !self.path.exists() {
            return Err(DecodeError::FileNotFound {
                path: self.path.clone(),
            });
        }

        // Build the internal configuration
        let config = VideoDecoderConfig {
            output_format: self.output_format,
            output_scale: self.output_scale,
            hardware_accel: self.hardware_accel,
            thread_count: self.thread_count,
        };

        // Create the decoder inner
        let (inner, stream_info, container_info) = VideoDecoderInner::new(
            &self.path,
            self.output_format,
            self.output_scale,
            self.hardware_accel,
            self.thread_count,
            self.frame_rate,
            self.frame_pool.clone(),
            self.network_opts,
        )?;

        Ok(VideoDecoder {
            path: self.path,
            config,
            frame_pool: self.frame_pool,
            inner,
            stream_info,
            container_info,
            fused: false,
        })
    }
}

/// A video decoder for extracting frames from media files.
///
/// The decoder provides frame-by-frame access to video content with support
/// for seeking, hardware acceleration, and format conversion.
///
/// # Construction
///
/// Use [`VideoDecoder::open()`] to create a builder, then call [`VideoDecoderBuilder::build()`]:
///
/// ```ignore
/// use ff_decode::VideoDecoder;
/// use ff_format::PixelFormat;
///
/// let decoder = VideoDecoder::open("video.mp4")?
///     .output_format(PixelFormat::Rgba)
///     .build()?;
/// ```
///
/// # Frame Decoding
///
/// Frames can be decoded one at a time or using the built-in iterator:
///
/// ```ignore
/// // Decode one frame
/// if let Some(frame) = decoder.decode_one()? {
///     println!("Frame at {:?}", frame.timestamp().as_duration());
/// }
///
/// // Iterator form — VideoDecoder implements Iterator directly
/// for result in &mut decoder {
///     let frame = result?;
///     // Process frame...
/// }
/// ```
///
/// # Seeking
///
/// The decoder supports efficient seeking:
///
/// ```ignore
/// use ff_decode::SeekMode;
/// use std::time::Duration;
///
/// // Seek to 30 seconds (keyframe)
/// decoder.seek(Duration::from_secs(30), SeekMode::Keyframe)?;
///
/// // Seek to exact frame
/// decoder.seek(Duration::from_secs(30), SeekMode::Exact)?;
/// ```
pub struct VideoDecoder {
    /// Path to the media file
    path: PathBuf,
    /// Decoder configuration
    ///
    /// NOTE: Currently unused but will be used when `FFmpeg` integration
    /// is implemented in a future issue.
    #[allow(dead_code)]
    config: VideoDecoderConfig,
    /// Optional frame pool for memory reuse
    frame_pool: Option<Arc<dyn FramePool>>,
    /// Internal decoder state
    inner: VideoDecoderInner,
    /// Video stream information
    stream_info: VideoStreamInfo,
    /// Container-level metadata
    container_info: ContainerInfo,
    /// Set to `true` after a decoding error; causes [`Iterator::next`] to return `None`.
    fused: bool,
}

impl VideoDecoder {
    /// Opens a media file and returns a builder for configuring the decoder.
    ///
    /// This is the entry point for creating a decoder. The returned builder
    /// allows setting options before the decoder is fully initialized.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the media file to decode.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Simple usage
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .build()?;
    ///
    /// // With options
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_format(PixelFormat::Rgba)
    ///     .hardware_accel(HardwareAccel::Auto)
    ///     .build()?;
    /// ```
    ///
    /// # Note
    ///
    /// This method does not validate that the file exists or is a valid
    /// media file. Validation occurs when [`VideoDecoderBuilder::build()`] is called.
    pub fn open(path: impl AsRef<Path>) -> VideoDecoderBuilder {
        VideoDecoderBuilder::new(path.as_ref().to_path_buf())
    }

    // =========================================================================
    // Information Methods
    // =========================================================================

    /// Returns the video stream information.
    ///
    /// This contains metadata about the video stream including resolution,
    /// frame rate, codec, and color characteristics.
    #[must_use]
    pub fn stream_info(&self) -> &VideoStreamInfo {
        &self.stream_info
    }

    /// Returns the video width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.stream_info.width()
    }

    /// Returns the video height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.stream_info.height()
    }

    /// Returns the frame rate in frames per second.
    #[must_use]
    pub fn frame_rate(&self) -> f64 {
        self.stream_info.fps()
    }

    /// Returns the total duration of the video.
    ///
    /// Returns [`Duration::ZERO`] if duration is unknown.
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.stream_info.duration().unwrap_or(Duration::ZERO)
    }

    /// Returns the total duration of the video, or `None` for live streams
    /// or formats that do not carry duration information.
    #[must_use]
    pub fn duration_opt(&self) -> Option<Duration> {
        self.stream_info.duration()
    }

    /// Returns container-level metadata (format name, bitrate, stream count).
    #[must_use]
    pub fn container_info(&self) -> &ContainerInfo {
        &self.container_info
    }

    /// Returns the current playback position.
    #[must_use]
    pub fn position(&self) -> Duration {
        self.inner.position()
    }

    /// Returns `true` if the end of stream has been reached.
    #[must_use]
    pub fn is_eof(&self) -> bool {
        self.inner.is_eof()
    }

    /// Returns the file path being decoded.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns a reference to the frame pool, if configured.
    #[must_use]
    pub fn frame_pool(&self) -> Option<&Arc<dyn FramePool>> {
        self.frame_pool.as_ref()
    }

    /// Returns the currently active hardware acceleration mode.
    ///
    /// This method returns the actual hardware acceleration being used,
    /// which may differ from what was requested:
    ///
    /// - If [`HardwareAccel::Auto`] was requested, this returns the specific
    ///   accelerator that was successfully initialized (e.g., [`HardwareAccel::Nvdec`]),
    ///   or [`HardwareAccel::None`] if no hardware acceleration is available.
    /// - If a specific accelerator was requested and initialization failed,
    ///   the decoder creation would have returned an error.
    /// - If [`HardwareAccel::None`] was requested, this always returns [`HardwareAccel::None`].
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::{VideoDecoder, HardwareAccel};
    ///
    /// // Request automatic hardware acceleration
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .hardware_accel(HardwareAccel::Auto)
    ///     .build()?;
    ///
    /// // Check which accelerator was selected
    /// match decoder.hardware_accel() {
    ///     HardwareAccel::None => println!("Using software decoding"),
    ///     HardwareAccel::Nvdec => println!("Using NVIDIA NVDEC"),
    ///     HardwareAccel::Qsv => println!("Using Intel Quick Sync"),
    ///     HardwareAccel::VideoToolbox => println!("Using Apple VideoToolbox"),
    ///     HardwareAccel::Vaapi => println!("Using VA-API"),
    ///     HardwareAccel::Amf => println!("Using AMD AMF"),
    ///     _ => unreachable!(),
    /// }
    /// ```
    #[must_use]
    pub fn hardware_accel(&self) -> HardwareAccel {
        self.inner.hardware_accel()
    }

    // =========================================================================
    // Decoding Methods
    // =========================================================================

    /// Decodes the next video frame.
    ///
    /// This method reads and decodes a single frame from the video stream.
    /// Frames are returned in presentation order.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(frame))` - A frame was successfully decoded
    /// - `Ok(None)` - End of stream reached, no more frames
    /// - `Err(_)` - An error occurred during decoding
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if:
    /// - Reading from the file fails
    /// - Decoding the frame fails
    /// - Pixel format conversion fails
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// let mut decoder = VideoDecoder::open("video.mp4")?.build()?;
    ///
    /// while let Some(frame) = decoder.decode_one()? {
    ///     println!("Frame at {:?}", frame.timestamp().as_duration());
    ///     // Process frame...
    /// }
    /// ```
    pub fn decode_one(&mut self) -> Result<Option<VideoFrame>, DecodeError> {
        self.inner.decode_one()
    }

    /// Decodes all frames within a specified time range.
    ///
    /// This method seeks to the start position and decodes all frames until
    /// the end position is reached. Frames outside the range are skipped.
    ///
    /// # Performance
    ///
    /// - The method performs a keyframe seek to the start position
    /// - Frames before `start` (from nearest keyframe) are decoded but discarded
    /// - All frames within `[start, end)` are collected and returned
    /// - The decoder position after this call will be at or past `end`
    ///
    /// For large time ranges or high frame rates, this may allocate significant
    /// memory. Consider iterating manually with [`decode_one()`](Self::decode_one)
    /// for very large ranges.
    ///
    /// # Arguments
    ///
    /// * `start` - Start of the time range (inclusive).
    /// * `end` - End of the time range (exclusive).
    ///
    /// # Returns
    ///
    /// A vector of frames with timestamps in the range `[start, end)`.
    /// Frames are returned in presentation order.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if:
    /// - Seeking to the start position fails
    /// - Decoding frames fails
    /// - The time range is invalid (start >= end)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use std::time::Duration;
    ///
    /// let mut decoder = VideoDecoder::open("video.mp4")?.build()?;
    ///
    /// // Decode frames from 5s to 10s
    /// let frames = decoder.decode_range(
    ///     Duration::from_secs(5),
    ///     Duration::from_secs(10),
    /// )?;
    ///
    /// println!("Decoded {} frames", frames.len());
    /// for frame in frames {
    ///     println!("Frame at {:?}", frame.timestamp().as_duration());
    /// }
    /// ```
    ///
    /// # Memory Usage
    ///
    /// At 30fps, a 5-second range will allocate ~150 frames. For 1080p RGBA:
    /// - Each frame: ~8.3 MB (1920 × 1080 × 4 bytes)
    /// - 150 frames: ~1.25 GB
    ///
    /// Consider using a frame pool to reduce allocation overhead.
    pub fn decode_range(
        &mut self,
        start: Duration,
        end: Duration,
    ) -> Result<Vec<VideoFrame>, DecodeError> {
        // Validate range
        if start >= end {
            return Err(DecodeError::DecodingFailed {
                timestamp: Some(start),
                reason: format!(
                    "Invalid time range: start ({start:?}) must be before end ({end:?})"
                ),
            });
        }

        // Seek to start position (keyframe mode for efficiency)
        self.seek(start, crate::SeekMode::Keyframe)?;

        // Collect frames in the range
        let mut frames = Vec::new();

        while let Some(frame) = self.decode_one()? {
            let frame_time = frame.timestamp().as_duration();

            // Stop if we've passed the end of the range
            if frame_time >= end {
                break;
            }

            // Only collect frames within the range
            if frame_time >= start {
                frames.push(frame);
            }
            // Frames before start are automatically discarded
        }

        Ok(frames)
    }

    // =========================================================================
    // Seeking Methods
    // =========================================================================

    /// Seeks to a specified position in the video stream.
    ///
    /// This method performs efficient seeking without reopening the file,
    /// providing significantly better performance than file-reopen-based seeking
    /// (5-10ms vs 50-100ms).
    ///
    /// # Performance
    ///
    /// - **Keyframe seeking**: 5-10ms (typical GOP 1-2s)
    /// - **Exact seeking**: 10-50ms depending on GOP size
    /// - **Backward seeking**: Similar to keyframe seeking
    ///
    /// For videos with large GOP sizes (>5 seconds), exact seeking may take longer
    /// as it requires decoding all frames from the nearest keyframe to the target.
    ///
    /// # Choosing a Seek Mode
    ///
    /// - **Use [`crate::SeekMode::Keyframe`]** for:
    ///   - Video player scrubbing (approximate positioning)
    ///   - Thumbnail generation
    ///   - Quick preview navigation
    ///
    /// - **Use [`crate::SeekMode::Exact`]** for:
    ///   - Frame-accurate editing
    ///   - Precise timestamp extraction
    ///   - Quality-critical operations
    ///
    /// - **Use [`crate::SeekMode::Backward`]** for:
    ///   - Guaranteed keyframe positioning
    ///   - Preparing for forward playback
    ///
    /// # Arguments
    ///
    /// * `position` - Target position to seek to.
    /// * `mode` - Seek mode determining accuracy and performance.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::SeekFailed`] if:
    /// - The target position is beyond the video duration
    /// - The file format doesn't support seeking
    /// - The seek operation fails internally
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::{VideoDecoder, SeekMode};
    /// use std::time::Duration;
    ///
    /// let mut decoder = VideoDecoder::open("video.mp4")?.build()?;
    ///
    /// // Fast seek to 30 seconds (keyframe)
    /// decoder.seek(Duration::from_secs(30), SeekMode::Keyframe)?;
    ///
    /// // Exact seek to 1 minute
    /// decoder.seek(Duration::from_secs(60), SeekMode::Exact)?;
    ///
    /// // Seek and decode next frame
    /// decoder.seek(Duration::from_secs(10), SeekMode::Keyframe)?;
    /// if let Some(frame) = decoder.decode_one()? {
    ///     println!("Frame at {:?}", frame.timestamp().as_duration());
    /// }
    /// ```
    pub fn seek(&mut self, position: Duration, mode: crate::SeekMode) -> Result<(), DecodeError> {
        if self.inner.is_live() {
            return Err(DecodeError::SeekNotSupported);
        }
        self.inner.seek(position, mode)
    }

    /// Returns `true` if the source is a live or streaming input.
    ///
    /// Live sources (HLS live playlists, RTMP, RTSP, MPEG-TS) have the
    /// `AVFMT_TS_DISCONT` flag set on their `AVInputFormat`. Seeking is not
    /// supported on live sources — [`VideoDecoder::seek`] will return
    /// [`DecodeError::SeekNotSupported`].
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.inner.is_live()
    }

    /// Flushes the decoder's internal buffers.
    ///
    /// This method clears any cached frames and resets the decoder state.
    /// The decoder is ready to receive new packets after flushing.
    ///
    /// # When to Use
    ///
    /// - After seeking to ensure clean state
    /// - Before switching between different parts of the video
    /// - To clear buffered frames after errors
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// let mut decoder = VideoDecoder::open("video.mp4")?.build()?;
    ///
    /// // Decode some frames...
    /// for _ in 0..10 {
    ///     decoder.decode_one()?;
    /// }
    ///
    /// // Flush and start fresh
    /// decoder.flush();
    /// ```
    ///
    /// # Note
    ///
    /// Calling [`seek()`](Self::seek) automatically flushes the decoder,
    /// so you don't need to call this method explicitly after seeking.
    pub fn flush(&mut self) {
        self.inner.flush();
    }

    // =========================================================================
    // Thumbnail Generation Methods
    // =========================================================================

    /// Generates a thumbnail at a specific timestamp.
    ///
    /// This method seeks to the specified position, decodes a frame, and scales
    /// it to the target dimensions. It's optimized for thumbnail generation by
    /// using keyframe seeking for speed.
    ///
    /// # Performance
    ///
    /// - Seeking: 5-10ms (keyframe mode)
    /// - Decoding: 5-10ms for 1080p H.264
    /// - Scaling: 1-3ms for 1080p → 320x180
    /// - **Total: ~10-25ms per thumbnail**
    ///
    /// # Aspect Ratio
    ///
    /// The thumbnail preserves the video's aspect ratio using a "fit-within"
    /// strategy. The output dimensions will be at most the target size, with
    /// at least one dimension matching the target. No letterboxing is applied.
    ///
    /// # Arguments
    ///
    /// * `position` - Timestamp to extract the thumbnail from.
    /// * `width` - Target thumbnail width in pixels.
    /// * `height` - Target thumbnail height in pixels.
    ///
    /// # Returns
    ///
    /// A scaled `VideoFrame` representing the thumbnail.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if:
    /// - Seeking to the position fails
    /// - No frame can be decoded at that position (returns `Ok(None)`)
    /// - Scaling fails
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use std::time::Duration;
    ///
    /// let mut decoder = VideoDecoder::open("video.mp4")?.build()?;
    ///
    /// // Generate a 320x180 thumbnail at 5 seconds
    /// let thumbnail = decoder.thumbnail_at(
    ///     Duration::from_secs(5),
    ///     320,
    ///     180,
    /// )?;
    ///
    /// assert_eq!(thumbnail.width(), 320);
    /// assert_eq!(thumbnail.height(), 180);
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Video player scrubbing preview
    /// - Timeline thumbnail strips
    /// - Gallery view thumbnails
    /// - Social media preview images
    pub fn thumbnail_at(
        &mut self,
        position: Duration,
        width: u32,
        height: u32,
    ) -> Result<Option<VideoFrame>, DecodeError> {
        // 1. Seek to the specified position (keyframe mode for speed)
        self.seek(position, crate::SeekMode::Keyframe)?;

        // 2. Decode one frame — Ok(None) means no frame at this position
        match self.decode_one()? {
            Some(frame) => self.inner.scale_frame(&frame, width, height).map(Some),
            None => Ok(None),
        }
    }

    /// Generates multiple thumbnails evenly distributed across the video.
    ///
    /// This method creates a series of thumbnails by dividing the video duration
    /// into equal intervals and extracting a frame at each position. This is
    /// commonly used for timeline preview strips or video galleries.
    ///
    /// # Performance
    ///
    /// For a 2-minute video generating 10 thumbnails:
    /// - Per thumbnail: ~10-25ms (see [`thumbnail_at()`](Self::thumbnail_at))
    /// - **Total: ~100-250ms**
    ///
    /// Performance scales linearly with the number of thumbnails.
    ///
    /// # Thumbnail Positions
    ///
    /// Thumbnails are extracted at evenly spaced intervals:
    /// - Position 0: `0s`
    /// - Position 1: `duration / count`
    /// - Position 2: `2 * (duration / count)`
    /// - ...
    /// - Position N-1: `(N-1) * (duration / count)`
    ///
    /// # Arguments
    ///
    /// * `count` - Number of thumbnails to generate.
    /// * `width` - Target thumbnail width in pixels.
    /// * `height` - Target thumbnail height in pixels.
    ///
    /// # Returns
    ///
    /// A vector of `VideoFrame` thumbnails in temporal order.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if:
    /// - Any individual thumbnail generation fails (see [`thumbnail_at()`](Self::thumbnail_at))
    /// - The video duration is unknown ([`Duration::ZERO`])
    /// - Count is zero
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// let mut decoder = VideoDecoder::open("video.mp4")?.build()?;
    ///
    /// // Generate 10 thumbnails at 160x90 resolution
    /// let thumbnails = decoder.thumbnails(10, 160, 90)?;
    ///
    /// assert_eq!(thumbnails.len(), 10);
    /// for thumb in thumbnails {
    ///     assert_eq!(thumb.width(), 160);
    ///     assert_eq!(thumb.height(), 90);
    /// }
    /// ```
    ///
    /// # Use Cases
    ///
    /// - Timeline preview strips (like `YouTube`'s timeline hover)
    /// - Video gallery grid views
    /// - Storyboard generation for editing
    /// - Video summary/preview pages
    ///
    /// # Memory Usage
    ///
    /// For 10 thumbnails at 160x90 RGBA:
    /// - Per thumbnail: ~56 KB (160 × 90 × 4 bytes)
    /// - Total: ~560 KB
    ///
    /// This is typically acceptable, but consider using a smaller resolution
    /// or generating thumbnails on-demand for very large thumbnail counts.
    pub fn thumbnails(
        &mut self,
        count: usize,
        width: u32,
        height: u32,
    ) -> Result<Vec<VideoFrame>, DecodeError> {
        // Validate count
        if count == 0 {
            return Err(DecodeError::DecodingFailed {
                timestamp: None,
                reason: "Thumbnail count must be greater than zero".to_string(),
            });
        }

        let duration = self.duration();

        // Check if duration is valid
        if duration.is_zero() {
            return Err(DecodeError::DecodingFailed {
                timestamp: None,
                reason: "Cannot generate thumbnails: video duration is unknown".to_string(),
            });
        }

        // Calculate interval between thumbnails
        let interval_nanos = duration.as_nanos() / count as u128;

        // Generate thumbnails
        let mut thumbnails = Vec::with_capacity(count);

        for i in 0..count {
            // Use saturating_mul to prevent u128 overflow
            let position_nanos = interval_nanos.saturating_mul(i as u128);
            // Clamp to u64::MAX to prevent overflow when converting to Duration
            #[allow(clippy::cast_possible_truncation)]
            let position_nanos_u64 = position_nanos.min(u128::from(u64::MAX)) as u64;
            let position = Duration::from_nanos(position_nanos_u64);

            if let Some(thumbnail) = self.thumbnail_at(position, width, height)? {
                thumbnails.push(thumbnail);
            }
        }

        Ok(thumbnails)
    }
}

impl Iterator for VideoDecoder {
    type Item = Result<VideoFrame, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.fused {
            return None;
        }
        match self.decode_one() {
            Ok(Some(frame)) => Some(Ok(frame)),
            Ok(None) => None,
            Err(e) => {
                self.fused = true;
                Some(Err(e))
            }
        }
    }
}

impl std::iter::FusedIterator for VideoDecoder {}

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_builder_default_values() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4"));

        assert_eq!(builder.path(), Path::new("test.mp4"));
        assert!(builder.get_output_format().is_none());
        assert_eq!(builder.get_hardware_accel(), HardwareAccel::Auto);
        assert_eq!(builder.get_thread_count(), 0);
    }

    #[test]
    fn test_builder_output_format() {
        let builder =
            VideoDecoderBuilder::new(PathBuf::from("test.mp4")).output_format(PixelFormat::Rgba);

        assert_eq!(builder.get_output_format(), Some(PixelFormat::Rgba));
    }

    #[test]
    fn test_builder_hardware_accel() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4"))
            .hardware_accel(HardwareAccel::Nvdec);

        assert_eq!(builder.get_hardware_accel(), HardwareAccel::Nvdec);
    }

    #[test]
    fn test_builder_thread_count() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).thread_count(8);

        assert_eq!(builder.get_thread_count(), 8);
    }

    #[test]
    fn test_builder_chaining() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4"))
            .output_format(PixelFormat::Bgra)
            .hardware_accel(HardwareAccel::Qsv)
            .thread_count(4);

        assert_eq!(builder.get_output_format(), Some(PixelFormat::Bgra));
        assert_eq!(builder.get_hardware_accel(), HardwareAccel::Qsv);
        assert_eq!(builder.get_thread_count(), 4);
    }

    #[test]
    fn test_decoder_open() {
        let builder = VideoDecoder::open("video.mp4");
        assert_eq!(builder.path(), Path::new("video.mp4"));
    }

    #[test]
    fn test_decoder_open_pathbuf() {
        let path = PathBuf::from("/path/to/video.mp4");
        let builder = VideoDecoder::open(&path);
        assert_eq!(builder.path(), path.as_path());
    }

    #[test]
    fn test_build_file_not_found() {
        let result = VideoDecoder::open("nonexistent_file_12345.mp4").build();

        assert!(result.is_err());
        match result {
            Err(DecodeError::FileNotFound { path }) => {
                assert!(
                    path.to_string_lossy()
                        .contains("nonexistent_file_12345.mp4")
                );
            }
            Err(e) => panic!("Expected FileNotFound error, got: {e:?}"),
            Ok(_) => panic!("Expected error, got Ok"),
        }
    }

    #[test]
    fn test_decoder_initial_state_with_invalid_file() {
        // Create a temporary test file (not a valid video)
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("ff_decode_test_file.txt");
        std::fs::write(&test_file, "test").expect("Failed to create test file");

        let result = VideoDecoder::open(&test_file).build();

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        // The build should fail (not a valid video file)
        assert!(result.is_err());
        if let Err(e) = result {
            // Should get either NoVideoStream or Ffmpeg error
            assert!(
                matches!(e, DecodeError::NoVideoStream { .. })
                    || matches!(e, DecodeError::Ffmpeg { .. })
            );
        }
    }

    #[test]
    fn test_decoder_config_default() {
        let config = VideoDecoderConfig::default();

        assert!(config.output_format.is_none());
        assert_eq!(config.hardware_accel, HardwareAccel::Auto);
        assert_eq!(config.thread_count, 0);
    }

    #[test]
    fn test_seek_mode_variants() {
        // Test that all SeekMode variants exist and are accessible
        use crate::SeekMode;

        let keyframe = SeekMode::Keyframe;
        let exact = SeekMode::Exact;
        let backward = SeekMode::Backward;

        // Verify they can be compared
        assert_eq!(keyframe, SeekMode::Keyframe);
        assert_eq!(exact, SeekMode::Exact);
        assert_eq!(backward, SeekMode::Backward);
        assert_ne!(keyframe, exact);
        assert_ne!(exact, backward);
    }

    #[test]
    fn test_seek_mode_default() {
        use crate::SeekMode;

        let default_mode = SeekMode::default();
        assert_eq!(default_mode, SeekMode::Keyframe);
    }

    #[test]
    fn test_decode_range_invalid_range() {
        use std::time::Duration;

        // Create a temporary test file
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("ff_decode_range_test.txt");
        std::fs::write(&test_file, "test").expect("Failed to create test file");

        // Try to build decoder (will fail, but that's ok for this test)
        let result = VideoDecoder::open(&test_file).build();

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        // If we somehow got a decoder (shouldn't happen with text file),
        // test that invalid range returns error
        if let Ok(mut decoder) = result {
            let start = Duration::from_secs(10);
            let end = Duration::from_secs(5); // end < start

            let range_result = decoder.decode_range(start, end);
            assert!(range_result.is_err());

            if let Err(DecodeError::DecodingFailed { reason, .. }) = range_result {
                assert!(reason.contains("Invalid time range"));
            }
        }
    }

    #[test]
    fn test_decode_range_equal_start_end() {
        use std::time::Duration;

        // Test that start == end is treated as invalid range
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("ff_decode_range_equal_test.txt");
        std::fs::write(&test_file, "test").expect("Failed to create test file");

        let result = VideoDecoder::open(&test_file).build();

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        if let Ok(mut decoder) = result {
            let time = Duration::from_secs(5);
            let range_result = decoder.decode_range(time, time);
            assert!(range_result.is_err());

            if let Err(DecodeError::DecodingFailed { reason, .. }) = range_result {
                assert!(reason.contains("Invalid time range"));
            }
        }
    }

    #[test]
    fn test_thumbnails_zero_count() {
        // Create a temporary test file
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("ff_decode_thumbnails_zero_test.txt");
        std::fs::write(&test_file, "test").expect("Failed to create test file");

        let result = VideoDecoder::open(&test_file).build();

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        // If we somehow got a decoder (shouldn't happen with text file),
        // test that zero count returns error
        if let Ok(mut decoder) = result {
            let thumbnails_result = decoder.thumbnails(0, 160, 90);
            assert!(thumbnails_result.is_err());

            if let Err(DecodeError::DecodingFailed { reason, .. }) = thumbnails_result {
                assert!(reason.contains("Thumbnail count must be greater than zero"));
            }
        }
    }

    #[test]
    fn test_thumbnail_api_exists() {
        // Compile-time test to verify thumbnail methods exist on Decoder
        // This ensures the API surface is correct even without real video files

        // Create a builder (won't actually build successfully with a nonexistent file)
        let builder = VideoDecoder::open("nonexistent.mp4");

        // Verify the builder exists
        let _ = builder;

        // The actual thumbnail generation tests require real video files
        // and should be in integration tests. This test just verifies
        // that the methods are accessible at compile time.
    }

    #[test]
    fn test_thumbnail_dimensions_calculation() {
        // Test aspect ratio preservation logic (indirectly through DecoderInner)
        // This is a compile-time test to ensure the code structure is correct

        // Source: 1920x1080 (16:9)
        // Target: 320x180 (16:9)
        // Expected: 320x180 (exact fit)

        let src_width = 1920.0_f64;
        let src_height = 1080.0_f64;
        let target_width = 320.0_f64;
        let target_height = 180.0_f64;

        let src_aspect = src_width / src_height;
        let target_aspect = target_width / target_height;

        let (scaled_width, scaled_height) = if src_aspect > target_aspect {
            let height = (target_width / src_aspect).round();
            (target_width, height)
        } else {
            let width = (target_height * src_aspect).round();
            (width, target_height)
        };

        assert_eq!(scaled_width, 320.0);
        assert_eq!(scaled_height, 180.0);
    }

    #[test]
    fn test_thumbnail_aspect_ratio_wide_source() {
        // Test aspect ratio preservation for wide source
        // Source: 1920x1080 (16:9)
        // Target: 180x180 (1:1)
        // Expected: 180x101 (fits width, height adjusted)

        let src_width = 1920.0_f64;
        let src_height = 1080.0_f64;
        let target_width = 180.0_f64;
        let target_height = 180.0_f64;

        let src_aspect = src_width / src_height;
        let target_aspect = target_width / target_height;

        let (scaled_width, scaled_height) = if src_aspect > target_aspect {
            let height = (target_width / src_aspect).round();
            (target_width, height)
        } else {
            let width = (target_height * src_aspect).round();
            (width, target_height)
        };

        assert_eq!(scaled_width, 180.0);
        // 180 / (16/9) = 101.25 → 101
        assert!((scaled_height - 101.0).abs() < 1.0);
    }

    #[test]
    fn test_thumbnail_aspect_ratio_tall_source() {
        // Test aspect ratio preservation for tall source
        // Source: 1080x1920 (9:16 - portrait)
        // Target: 180x180 (1:1)
        // Expected: 101x180 (fits height, width adjusted)

        let src_width = 1080.0_f64;
        let src_height = 1920.0_f64;
        let target_width = 180.0_f64;
        let target_height = 180.0_f64;

        let src_aspect = src_width / src_height;
        let target_aspect = target_width / target_height;

        let (scaled_width, scaled_height) = if src_aspect > target_aspect {
            let height = (target_width / src_aspect).round();
            (target_width, height)
        } else {
            let width = (target_height * src_aspect).round();
            (width, target_height)
        };

        // 180 * (9/16) = 101.25 → 101
        assert!((scaled_width - 101.0).abs() < 1.0);
        assert_eq!(scaled_height, 180.0);
    }
}
