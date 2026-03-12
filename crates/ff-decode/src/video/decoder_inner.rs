//! Internal video decoder implementation using FFmpeg.
//!
//! This module contains the low-level decoder logic that directly interacts
//! with FFmpeg's C API through the ff-sys crate. It is not exposed publicly.

// Allow unsafe code in this module as it's necessary for FFmpeg FFI
#![allow(unsafe_code)]
// Allow specific clippy lints for FFmpeg FFI code
#![allow(clippy::similar_names)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::if_not_else)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::cast_lossless)]

use std::path::Path;
use std::ptr;
use std::sync::Arc;
use std::time::Duration;

use ff_format::PooledBuffer;
use ff_format::codec::VideoCodec;
use ff_format::color::{ColorPrimaries, ColorRange, ColorSpace};
use ff_format::time::{Rational, Timestamp};
use ff_format::{PixelFormat, VideoFrame, VideoStreamInfo};
use ff_sys::{
    AVBufferRef, AVCodecContext, AVCodecID, AVColorPrimaries, AVColorRange, AVColorSpace,
    AVFormatContext, AVFrame, AVHWDeviceType, AVMediaType_AVMEDIA_TYPE_VIDEO, AVPacket,
    AVPixelFormat, SwsContext,
};

use crate::HardwareAccel;
use crate::error::DecodeError;
use crate::pool::FramePool;

/// Tolerance in seconds for keyframe/backward seek modes.
///
/// When seeking in Keyframe or Backward mode, frames are skipped until we're within
/// this tolerance of the target position. This balances accuracy with performance for
/// typical GOP sizes (1-2 seconds).
const KEYFRAME_SEEK_TOLERANCE_SECS: u64 = 1;

/// RAII guard for `AVFormatContext` to ensure proper cleanup.
struct AvFormatContextGuard(*mut AVFormatContext);

impl AvFormatContextGuard {
    /// Creates a new guard by opening an input file.
    ///
    /// # Safety
    ///
    /// Caller must ensure FFmpeg is initialized and path is valid.
    unsafe fn new(path: &Path) -> Result<Self, DecodeError> {
        // SAFETY: Caller ensures FFmpeg is initialized and path is valid
        let format_ctx = unsafe {
            ff_sys::avformat::open_input(path).map_err(|e| {
                DecodeError::Ffmpeg(format!(
                    "Failed to open file: {}",
                    ff_sys::av_error_string(e)
                ))
            })?
        };
        Ok(Self(format_ctx))
    }

    /// Returns the raw pointer.
    const fn as_ptr(&self) -> *mut AVFormatContext {
        self.0
    }

    /// Consumes the guard and returns the raw pointer without dropping.
    fn into_raw(self) -> *mut AVFormatContext {
        let ptr = self.0;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for AvFormatContextGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: self.0 is valid and owned by this guard
            unsafe {
                ff_sys::avformat::close_input(&mut (self.0 as *mut _));
            }
        }
    }
}

/// RAII guard for `AVCodecContext` to ensure proper cleanup.
struct AvCodecContextGuard(*mut AVCodecContext);

impl AvCodecContextGuard {
    /// Creates a new guard by allocating a codec context.
    ///
    /// # Safety
    ///
    /// Caller must ensure codec pointer is valid.
    unsafe fn new(codec: *const ff_sys::AVCodec) -> Result<Self, DecodeError> {
        // SAFETY: Caller ensures codec pointer is valid
        let codec_ctx = unsafe {
            ff_sys::avcodec::alloc_context3(codec).map_err(|e| {
                DecodeError::Ffmpeg(format!("Failed to allocate codec context: {e}"))
            })?
        };
        Ok(Self(codec_ctx))
    }

    /// Returns the raw pointer.
    const fn as_ptr(&self) -> *mut AVCodecContext {
        self.0
    }

    /// Consumes the guard and returns the raw pointer without dropping.
    fn into_raw(self) -> *mut AVCodecContext {
        let ptr = self.0;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for AvCodecContextGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: self.0 is valid and owned by this guard
            unsafe {
                ff_sys::avcodec::free_context(&mut (self.0 as *mut _));
            }
        }
    }
}

/// RAII guard for `AVPacket` to ensure proper cleanup.
struct AvPacketGuard(*mut AVPacket);

impl AvPacketGuard {
    /// Creates a new guard by allocating a packet.
    ///
    /// # Safety
    ///
    /// Must be called after FFmpeg initialization.
    unsafe fn new() -> Result<Self, DecodeError> {
        // SAFETY: Caller ensures FFmpeg is initialized
        let packet = unsafe { ff_sys::av_packet_alloc() };
        if packet.is_null() {
            return Err(DecodeError::Ffmpeg("Failed to allocate packet".to_string()));
        }
        Ok(Self(packet))
    }

    /// Returns the raw pointer.
    #[allow(dead_code)]
    const fn as_ptr(&self) -> *mut AVPacket {
        self.0
    }

    /// Consumes the guard and returns the raw pointer without dropping.
    fn into_raw(self) -> *mut AVPacket {
        let ptr = self.0;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for AvPacketGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: self.0 is valid and owned by this guard
            unsafe {
                ff_sys::av_packet_free(&mut (self.0 as *mut _));
            }
        }
    }
}

/// RAII guard for `AVFrame` to ensure proper cleanup.
struct AvFrameGuard(*mut AVFrame);

impl AvFrameGuard {
    /// Creates a new guard by allocating a frame.
    ///
    /// # Safety
    ///
    /// Must be called after FFmpeg initialization.
    unsafe fn new() -> Result<Self, DecodeError> {
        // SAFETY: Caller ensures FFmpeg is initialized
        let frame = unsafe { ff_sys::av_frame_alloc() };
        if frame.is_null() {
            return Err(DecodeError::Ffmpeg("Failed to allocate frame".to_string()));
        }
        Ok(Self(frame))
    }

    /// Returns the raw pointer.
    const fn as_ptr(&self) -> *mut AVFrame {
        self.0
    }

    /// Consumes the guard and returns the raw pointer without dropping.
    fn into_raw(self) -> *mut AVFrame {
        let ptr = self.0;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for AvFrameGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: self.0 is valid and owned by this guard
            unsafe {
                ff_sys::av_frame_free(&mut (self.0 as *mut _));
            }
        }
    }
}

/// Internal decoder state holding FFmpeg contexts.
///
/// This structure manages the lifecycle of FFmpeg objects and is responsible
/// for proper cleanup when dropped.
pub(crate) struct VideoDecoderInner {
    /// Format context for reading the media file
    format_ctx: *mut AVFormatContext,
    /// Codec context for decoding video frames
    codec_ctx: *mut AVCodecContext,
    /// Video stream index in the format context
    stream_index: i32,
    /// SwScale context for pixel format conversion (optional)
    sws_ctx: Option<*mut SwsContext>,
    /// Target output pixel format (if conversion is needed)
    output_format: Option<PixelFormat>,
    /// Whether end of file has been reached
    eof: bool,
    /// Current playback position
    position: Duration,
    /// Reusable packet for reading from file
    packet: *mut AVPacket,
    /// Reusable frame for decoding
    frame: *mut AVFrame,
    /// Cached SwScale context for thumbnail generation
    thumbnail_sws_ctx: Option<*mut SwsContext>,
    /// Last thumbnail dimensions (for cache invalidation)
    thumbnail_cache_key: Option<(u32, u32, u32, u32, AVPixelFormat)>,
    /// Hardware device context (if hardware acceleration is active)
    hw_device_ctx: Option<*mut AVBufferRef>,
    /// Active hardware acceleration mode
    active_hw_accel: HardwareAccel,
    /// Optional frame pool for memory reuse
    frame_pool: Option<Arc<dyn FramePool>>,
}

impl VideoDecoderInner {
    /// Maps our `HardwareAccel` enum to the corresponding FFmpeg `AVHWDeviceType`.
    ///
    /// Returns `None` for `Auto` and `None` variants as they require special handling.
    fn hw_accel_to_device_type(accel: HardwareAccel) -> Option<AVHWDeviceType> {
        match accel {
            HardwareAccel::Auto => None,
            HardwareAccel::None => None,
            HardwareAccel::Nvdec => Some(ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_CUDA),
            HardwareAccel::Qsv => Some(ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_QSV),
            HardwareAccel::Amf => Some(ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA), // AMF uses D3D11
            HardwareAccel::VideoToolbox => {
                Some(ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX)
            }
            HardwareAccel::Vaapi => Some(ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI),
        }
    }

    /// Returns the hardware decoders to try in priority order for Auto mode.
    const fn hw_accel_auto_priority() -> &'static [HardwareAccel] {
        // Priority order: NVDEC, QSV, VideoToolbox, VA-API, AMF
        &[
            HardwareAccel::Nvdec,
            HardwareAccel::Qsv,
            HardwareAccel::VideoToolbox,
            HardwareAccel::Vaapi,
            HardwareAccel::Amf,
        ]
    }

    /// Attempts to initialize hardware acceleration.
    ///
    /// # Arguments
    ///
    /// * `codec_ctx` - The codec context to configure
    /// * `accel` - Requested hardware acceleration mode
    ///
    /// # Returns
    ///
    /// Returns `Ok((hw_device_ctx, active_accel))` if hardware acceleration was initialized,
    /// or `Ok((None, HardwareAccel::None))` if software decoding should be used.
    ///
    /// # Errors
    ///
    /// Returns an error only if a specific hardware accelerator was requested but failed to initialize.
    unsafe fn init_hardware_accel(
        codec_ctx: *mut AVCodecContext,
        accel: HardwareAccel,
    ) -> Result<(Option<*mut AVBufferRef>, HardwareAccel), DecodeError> {
        match accel {
            HardwareAccel::Auto => {
                // Try hardware accelerators in priority order
                for &hw_type in Self::hw_accel_auto_priority() {
                    // SAFETY: Caller ensures codec_ctx is valid and not yet configured with hardware
                    if let Ok((Some(ctx), active)) =
                        unsafe { Self::try_init_hw_device(codec_ctx, hw_type) }
                    {
                        return Ok((Some(ctx), active));
                    }
                    // Ignore errors in Auto mode and try the next one
                }
                // All hardware accelerators failed, fall back to software
                Ok((None, HardwareAccel::None))
            }
            HardwareAccel::None => {
                // Software decoding explicitly requested
                Ok((None, HardwareAccel::None))
            }
            _ => {
                // Specific hardware accelerator requested
                // SAFETY: Caller ensures codec_ctx is valid and not yet configured with hardware
                unsafe { Self::try_init_hw_device(codec_ctx, accel) }
            }
        }
    }

    /// Tries to initialize a specific hardware device.
    ///
    /// # Safety
    ///
    /// Caller must ensure `codec_ctx` is valid and not yet configured with a hardware device.
    unsafe fn try_init_hw_device(
        codec_ctx: *mut AVCodecContext,
        accel: HardwareAccel,
    ) -> Result<(Option<*mut AVBufferRef>, HardwareAccel), DecodeError> {
        // Get the FFmpeg device type
        let Some(device_type) = Self::hw_accel_to_device_type(accel) else {
            return Ok((None, HardwareAccel::None));
        };

        // Create hardware device context
        // SAFETY: FFmpeg is initialized, device_type is valid
        let mut hw_device_ctx: *mut AVBufferRef = ptr::null_mut();
        let ret = unsafe {
            ff_sys::av_hwdevice_ctx_create(
                ptr::addr_of_mut!(hw_device_ctx),
                device_type,
                ptr::null(),     // device: null for default device
                ptr::null_mut(), // opts: null for default options
                0,               // flags: currently unused by FFmpeg
            )
        };

        if ret < 0 {
            // Hardware device creation failed
            return Err(DecodeError::HwAccelUnavailable { accel });
        }

        // Assign hardware device context to codec context
        // We transfer ownership of the reference to codec_ctx
        // SAFETY: codec_ctx and hw_device_ctx are valid
        unsafe {
            (*codec_ctx).hw_device_ctx = hw_device_ctx;
        }

        // We keep our own reference for cleanup in Drop
        // SAFETY: hw_device_ctx is valid
        let our_ref = unsafe { ff_sys::av_buffer_ref(hw_device_ctx) };
        if our_ref.is_null() {
            // Failed to create our reference
            // codec_ctx still owns the original, so we don't need to clean it up here
            return Err(DecodeError::HwAccelUnavailable { accel });
        }

        Ok((Some(our_ref), accel))
    }

    /// Returns the currently active hardware acceleration mode.
    pub(crate) fn hardware_accel(&self) -> HardwareAccel {
        self.active_hw_accel
    }

    /// Checks if a pixel format is a hardware format.
    ///
    /// Hardware formats include: D3D11, CUDA, VAAPI, VideoToolbox, QSV, etc.
    const fn is_hardware_format(format: AVPixelFormat) -> bool {
        matches!(
            format,
            ff_sys::AVPixelFormat_AV_PIX_FMT_D3D11
                | ff_sys::AVPixelFormat_AV_PIX_FMT_CUDA
                | ff_sys::AVPixelFormat_AV_PIX_FMT_VAAPI
                | ff_sys::AVPixelFormat_AV_PIX_FMT_VIDEOTOOLBOX
                | ff_sys::AVPixelFormat_AV_PIX_FMT_QSV
                | ff_sys::AVPixelFormat_AV_PIX_FMT_VDPAU
                | ff_sys::AVPixelFormat_AV_PIX_FMT_DXVA2_VLD
                | ff_sys::AVPixelFormat_AV_PIX_FMT_OPENCL
                | ff_sys::AVPixelFormat_AV_PIX_FMT_MEDIACODEC
                | ff_sys::AVPixelFormat_AV_PIX_FMT_VULKAN
        )
    }

    /// Transfers a hardware frame to CPU memory if needed.
    ///
    /// If `self.frame` is a hardware frame, creates a new software frame
    /// and transfers the data from GPU to CPU memory.
    ///
    /// # Safety
    ///
    /// Caller must ensure `self.frame` contains a valid decoded frame.
    unsafe fn transfer_hardware_frame_if_needed(&mut self) -> Result<(), DecodeError> {
        // SAFETY: self.frame is valid and owned by this instance
        let frame_format = unsafe { (*self.frame).format };

        if !Self::is_hardware_format(frame_format) {
            // Not a hardware frame, no transfer needed
            return Ok(());
        }

        // Create a temporary software frame for transfer
        // SAFETY: FFmpeg is initialized
        let sw_frame = unsafe { ff_sys::av_frame_alloc() };
        if sw_frame.is_null() {
            return Err(DecodeError::Ffmpeg(
                "Failed to allocate software frame for hardware transfer".to_string(),
            ));
        }

        // Transfer data from hardware frame to software frame
        // SAFETY: self.frame and sw_frame are valid
        let ret = unsafe {
            ff_sys::av_hwframe_transfer_data(
                sw_frame, self.frame, 0, // flags: currently unused
            )
        };

        if ret < 0 {
            // Transfer failed, clean up
            unsafe {
                ff_sys::av_frame_free(&mut (sw_frame as *mut _));
            }
            return Err(DecodeError::Ffmpeg(format!(
                "Failed to transfer hardware frame to CPU memory: {}",
                ff_sys::av_error_string(ret)
            )));
        }

        // Copy metadata (pts, duration, etc.) from hardware frame to software frame
        // SAFETY: Both frames are valid
        unsafe {
            (*sw_frame).pts = (*self.frame).pts;
            (*sw_frame).pkt_dts = (*self.frame).pkt_dts;
            (*sw_frame).duration = (*self.frame).duration;
            (*sw_frame).time_base = (*self.frame).time_base;
        }

        // Replace self.frame with the software frame
        // SAFETY: self.frame is valid and owned by this instance
        unsafe {
            ff_sys::av_frame_unref(self.frame);
            ff_sys::av_frame_move_ref(self.frame, sw_frame);
            ff_sys::av_frame_free(&mut (sw_frame as *mut _));
        }

        Ok(())
    }

    /// Opens a media file and initializes the decoder.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the media file
    /// * `output_format` - Optional target pixel format for conversion
    /// * `hardware_accel` - Hardware acceleration mode
    /// * `thread_count` - Number of decoding threads (0 = auto)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened
    /// - No video stream is found
    /// - The codec is not supported
    /// - Decoder initialization fails
    pub(crate) fn new(
        path: &Path,
        output_format: Option<PixelFormat>,
        hardware_accel: HardwareAccel,
        thread_count: usize,
        frame_pool: Option<Arc<dyn FramePool>>,
    ) -> Result<(Self, VideoStreamInfo), DecodeError> {
        // Ensure FFmpeg is initialized (thread-safe and idempotent)
        ff_sys::ensure_initialized();

        // Open the input file (with RAII guard)
        // SAFETY: Path is valid, AvFormatContextGuard ensures cleanup
        let format_ctx_guard = unsafe { AvFormatContextGuard::new(path)? };
        let format_ctx = format_ctx_guard.as_ptr();

        // Read stream information
        // SAFETY: format_ctx is valid and owned by guard
        unsafe {
            ff_sys::avformat::find_stream_info(format_ctx).map_err(|e| {
                DecodeError::Ffmpeg(format!(
                    "Failed to find stream info: {}",
                    ff_sys::av_error_string(e)
                ))
            })?;
        }

        // Find the video stream
        // SAFETY: format_ctx is valid
        let (stream_index, codec_id) =
            unsafe { Self::find_video_stream(format_ctx) }.ok_or_else(|| {
                DecodeError::NoVideoStream {
                    path: path.to_path_buf(),
                }
            })?;

        // Find the decoder for this codec
        // SAFETY: codec_id is valid from FFmpeg
        let codec = unsafe {
            ff_sys::avcodec::find_decoder(codec_id).ok_or_else(|| {
                DecodeError::UnsupportedCodec {
                    codec: format!("codec_id={codec_id:?}"),
                }
            })?
        };

        // Allocate codec context (with RAII guard)
        // SAFETY: codec pointer is valid, AvCodecContextGuard ensures cleanup
        let codec_ctx_guard = unsafe { AvCodecContextGuard::new(codec)? };
        let codec_ctx = codec_ctx_guard.as_ptr();

        // Copy codec parameters from stream to context
        // SAFETY: format_ctx and codec_ctx are valid, stream_index is valid
        unsafe {
            let stream = (*format_ctx).streams.add(stream_index as usize);
            let codecpar = (*(*stream)).codecpar;
            ff_sys::avcodec::parameters_to_context(codec_ctx, codecpar).map_err(|e| {
                DecodeError::Ffmpeg(format!(
                    "Failed to copy codec parameters: {}",
                    ff_sys::av_error_string(e)
                ))
            })?;

            // Set thread count
            if thread_count > 0 {
                (*codec_ctx).thread_count = thread_count as i32;
            }
        }

        // Initialize hardware acceleration if requested
        // SAFETY: codec_ctx is valid and not yet opened
        let (hw_device_ctx, active_hw_accel) =
            unsafe { Self::init_hardware_accel(codec_ctx, hardware_accel)? };

        // Open the codec
        // SAFETY: codec_ctx and codec are valid, hardware device context is set if requested
        unsafe {
            ff_sys::avcodec::open2(codec_ctx, codec, ptr::null_mut()).map_err(|e| {
                // If codec opening failed, we still own our reference to hw_device_ctx
                // but it will be cleaned up when codec_ctx is freed (which happens
                // when codec_ctx_guard is dropped)
                // Our reference in hw_device_ctx will be cleaned up here
                if let Some(hw_ctx) = hw_device_ctx {
                    ff_sys::av_buffer_unref(&mut (hw_ctx as *mut _));
                }
                DecodeError::Ffmpeg(format!(
                    "Failed to open codec: {}",
                    ff_sys::av_error_string(e)
                ))
            })?;
        }

        // Extract stream information
        // SAFETY: All pointers are valid
        let stream_info =
            unsafe { Self::extract_stream_info(format_ctx, stream_index as i32, codec_ctx)? };

        // Allocate packet and frame (with RAII guards)
        // SAFETY: FFmpeg is initialized, guards ensure cleanup
        let packet_guard = unsafe { AvPacketGuard::new()? };
        let frame_guard = unsafe { AvFrameGuard::new()? };

        // All initialization successful - transfer ownership to VideoDecoderInner
        Ok((
            Self {
                format_ctx: format_ctx_guard.into_raw(),
                codec_ctx: codec_ctx_guard.into_raw(),
                stream_index: stream_index as i32,
                sws_ctx: None,
                output_format,
                eof: false,
                position: Duration::ZERO,
                packet: packet_guard.into_raw(),
                frame: frame_guard.into_raw(),
                thumbnail_sws_ctx: None,
                thumbnail_cache_key: None,
                hw_device_ctx,
                active_hw_accel,
                frame_pool,
            },
            stream_info,
        ))
    }

    /// Finds the first video stream in the format context.
    ///
    /// # Returns
    ///
    /// Returns `Some((index, codec_id))` if a video stream is found, `None` otherwise.
    ///
    /// # Safety
    ///
    /// Caller must ensure `format_ctx` is valid and initialized.
    unsafe fn find_video_stream(format_ctx: *mut AVFormatContext) -> Option<(usize, AVCodecID)> {
        // SAFETY: Caller ensures format_ctx is valid
        unsafe {
            let nb_streams = (*format_ctx).nb_streams as usize;

            for i in 0..nb_streams {
                let stream = (*format_ctx).streams.add(i);
                let codecpar = (*(*stream)).codecpar;

                if (*codecpar).codec_type == AVMediaType_AVMEDIA_TYPE_VIDEO {
                    return Some((i, (*codecpar).codec_id));
                }
            }

            None
        }
    }

    /// Extracts video stream information from FFmpeg structures.
    unsafe fn extract_stream_info(
        format_ctx: *mut AVFormatContext,
        stream_index: i32,
        codec_ctx: *mut AVCodecContext,
    ) -> Result<VideoStreamInfo, DecodeError> {
        // SAFETY: Caller ensures all pointers are valid
        let (
            width,
            height,
            fps_rational,
            duration_val,
            pix_fmt,
            color_space_val,
            color_range_val,
            color_primaries_val,
            codec_id,
        ) = unsafe {
            let stream = (*format_ctx).streams.add(stream_index as usize);
            let codecpar = (*(*stream)).codecpar;

            (
                (*codecpar).width as u32,
                (*codecpar).height as u32,
                (*(*stream)).avg_frame_rate,
                (*format_ctx).duration,
                (*codec_ctx).pix_fmt,
                (*codecpar).color_space,
                (*codecpar).color_range,
                (*codecpar).color_primaries,
                (*codecpar).codec_id,
            )
        };

        // Extract frame rate
        let frame_rate = if fps_rational.den != 0 {
            Rational::new(fps_rational.num as i32, fps_rational.den as i32)
        } else {
            log::warn!(
                "invalid frame rate, falling back to 30fps num={} den=0 fallback=30/1",
                fps_rational.num
            );
            Rational::new(30, 1)
        };

        // Extract duration
        let duration = if duration_val > 0 {
            let duration_secs = duration_val as f64 / 1_000_000.0;
            Some(Duration::from_secs_f64(duration_secs))
        } else {
            None
        };

        // Extract pixel format
        let pixel_format = Self::convert_pixel_format(pix_fmt);

        // Extract color information
        let color_space = Self::convert_color_space(color_space_val);
        let color_range = Self::convert_color_range(color_range_val);
        let color_primaries = Self::convert_color_primaries(color_primaries_val);

        // Extract codec
        let codec = Self::convert_codec(codec_id);

        // Build stream info
        let mut builder = VideoStreamInfo::builder()
            .index(stream_index as u32)
            .codec(codec)
            .width(width)
            .height(height)
            .frame_rate(frame_rate)
            .pixel_format(pixel_format)
            .color_space(color_space)
            .color_range(color_range)
            .color_primaries(color_primaries);

        if let Some(d) = duration {
            builder = builder.duration(d);
        }

        Ok(builder.build())
    }

    /// Converts FFmpeg pixel format to our PixelFormat enum.
    fn convert_pixel_format(fmt: AVPixelFormat) -> PixelFormat {
        if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P {
            PixelFormat::Yuv420p
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P {
            PixelFormat::Yuv422p
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P {
            PixelFormat::Yuv444p
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24 {
            PixelFormat::Rgb24
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24 {
            PixelFormat::Bgr24
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA {
            PixelFormat::Rgba
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA {
            PixelFormat::Bgra
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8 {
            PixelFormat::Gray8
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_NV12 {
            PixelFormat::Nv12
        } else if fmt == ff_sys::AVPixelFormat_AV_PIX_FMT_NV21 {
            PixelFormat::Nv21
        } else {
            log::warn!("pixel_format unsupported, falling back to Yuv420p requested={fmt} fallback=Yuv420p");
            PixelFormat::Yuv420p
        }
    }

    /// Converts FFmpeg color space to our ColorSpace enum.
    fn convert_color_space(space: AVColorSpace) -> ColorSpace {
        if space == ff_sys::AVColorSpace_AVCOL_SPC_BT709 {
            ColorSpace::Bt709
        } else if space == ff_sys::AVColorSpace_AVCOL_SPC_BT470BG
            || space == ff_sys::AVColorSpace_AVCOL_SPC_SMPTE170M
        {
            ColorSpace::Bt601
        } else if space == ff_sys::AVColorSpace_AVCOL_SPC_BT2020_NCL {
            ColorSpace::Bt2020
        } else {
            log::warn!("color_space unsupported, falling back to Bt709 requested={space} fallback=Bt709");
            ColorSpace::Bt709
        }
    }

    /// Converts FFmpeg color range to our ColorRange enum.
    fn convert_color_range(range: AVColorRange) -> ColorRange {
        if range == ff_sys::AVColorRange_AVCOL_RANGE_JPEG {
            ColorRange::Full
        } else if range == ff_sys::AVColorRange_AVCOL_RANGE_MPEG {
            ColorRange::Limited
        } else {
            log::warn!("color_range unsupported, falling back to Limited requested={range} fallback=Limited");
            ColorRange::Limited
        }
    }

    /// Converts FFmpeg color primaries to our ColorPrimaries enum.
    fn convert_color_primaries(primaries: AVColorPrimaries) -> ColorPrimaries {
        if primaries == ff_sys::AVColorPrimaries_AVCOL_PRI_BT709 {
            ColorPrimaries::Bt709
        } else if primaries == ff_sys::AVColorPrimaries_AVCOL_PRI_BT470BG
            || primaries == ff_sys::AVColorPrimaries_AVCOL_PRI_SMPTE170M
        {
            ColorPrimaries::Bt601
        } else if primaries == ff_sys::AVColorPrimaries_AVCOL_PRI_BT2020 {
            ColorPrimaries::Bt2020
        } else {
            log::warn!("color_primaries unsupported, falling back to Bt709 requested={primaries} fallback=Bt709");
            ColorPrimaries::Bt709
        }
    }

    /// Converts FFmpeg codec ID to our VideoCodec enum.
    fn convert_codec(codec_id: AVCodecID) -> VideoCodec {
        if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_H264 {
            VideoCodec::H264
        } else if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_HEVC {
            VideoCodec::H265
        } else if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_VP8 {
            VideoCodec::Vp8
        } else if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_VP9 {
            VideoCodec::Vp9
        } else if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_AV1 {
            VideoCodec::Av1
        } else if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_MPEG4 {
            VideoCodec::Mpeg4
        } else if codec_id == ff_sys::AVCodecID_AV_CODEC_ID_PRORES {
            VideoCodec::ProRes
        } else {
            log::warn!("video codec unsupported, falling back to H264 codec_id={codec_id} fallback=H264");
            VideoCodec::H264
        }
    }

    /// Decodes the next video frame.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(frame))` - Successfully decoded a frame
    /// - `Ok(None)` - End of stream reached
    /// - `Err(_)` - Decoding error occurred
    pub(crate) fn decode_one(&mut self) -> Result<Option<VideoFrame>, DecodeError> {
        if self.eof {
            return Ok(None);
        }

        unsafe {
            loop {
                // Try to receive a frame from the decoder
                let ret = ff_sys::avcodec_receive_frame(self.codec_ctx, self.frame);

                if ret == 0 {
                    // Successfully received a frame
                    // Check if this is a hardware frame and transfer to CPU memory if needed
                    self.transfer_hardware_frame_if_needed()?;

                    let video_frame = self.convert_frame_to_video_frame()?;

                    // Update position based on frame timestamp
                    let pts = (*self.frame).pts;
                    if pts != ff_sys::AV_NOPTS_VALUE {
                        let stream = (*self.format_ctx).streams.add(self.stream_index as usize);
                        let time_base = (*(*stream)).time_base;
                        let timestamp_secs =
                            pts as f64 * time_base.num as f64 / time_base.den as f64;
                        self.position = Duration::from_secs_f64(timestamp_secs);
                    }

                    return Ok(Some(video_frame));
                } else if ret == ff_sys::error_codes::EAGAIN {
                    // Need to send more packets to the decoder
                    // Read a packet from the file
                    let read_ret = ff_sys::av_read_frame(self.format_ctx, self.packet);

                    if read_ret == ff_sys::error_codes::EOF {
                        // End of file - flush the decoder
                        ff_sys::avcodec_send_packet(self.codec_ctx, ptr::null());
                        self.eof = true;
                        continue;
                    } else if read_ret < 0 {
                        return Err(DecodeError::Ffmpeg(format!(
                            "Failed to read frame: {}",
                            ff_sys::av_error_string(read_ret)
                        )));
                    }

                    // Check if this packet belongs to the video stream
                    if (*self.packet).stream_index == self.stream_index {
                        // Send the packet to the decoder
                        let send_ret = ff_sys::avcodec_send_packet(self.codec_ctx, self.packet);
                        ff_sys::av_packet_unref(self.packet);

                        if send_ret < 0 && send_ret != ff_sys::error_codes::EAGAIN {
                            return Err(DecodeError::Ffmpeg(format!(
                                "Failed to send packet: {}",
                                ff_sys::av_error_string(send_ret)
                            )));
                        }
                    } else {
                        // Not our stream, unref and continue
                        ff_sys::av_packet_unref(self.packet);
                    }
                } else if ret == ff_sys::error_codes::EOF {
                    // Decoder has been fully flushed
                    self.eof = true;
                    return Ok(None);
                } else {
                    return Err(DecodeError::DecodingFailed {
                        timestamp: Some(self.position),
                        reason: ff_sys::av_error_string(ret),
                    });
                }
            }
        }
    }

    /// Converts an AVFrame to a VideoFrame, applying pixel format conversion if needed.
    unsafe fn convert_frame_to_video_frame(&mut self) -> Result<VideoFrame, DecodeError> {
        // SAFETY: Caller ensures self.frame is valid
        unsafe {
            let width = (*self.frame).width as u32;
            let height = (*self.frame).height as u32;
            let src_format = (*self.frame).format;

            // Determine output format
            let dst_format = if let Some(fmt) = self.output_format {
                Self::pixel_format_to_av(fmt)
            } else {
                src_format
            };

            // Check if conversion is needed
            let needs_conversion = src_format != dst_format;

            if needs_conversion {
                self.convert_with_sws(width, height, src_format, dst_format)
            } else {
                self.av_frame_to_video_frame(self.frame)
            }
        }
    }

    /// Converts pixel format using SwScale.
    unsafe fn convert_with_sws(
        &mut self,
        width: u32,
        height: u32,
        src_format: i32,
        dst_format: i32,
    ) -> Result<VideoFrame, DecodeError> {
        // SAFETY: Caller ensures frame and context pointers are valid
        unsafe {
            // Get or create SwScale context
            if self.sws_ctx.is_none() {
                let ctx = ff_sys::swscale::get_context(
                    width as i32,
                    height as i32,
                    src_format,
                    width as i32,
                    height as i32,
                    dst_format,
                    ff_sys::swscale::scale_flags::BILINEAR,
                )
                .map_err(|e| DecodeError::Ffmpeg(format!("Failed to create sws context: {e}")))?;

                self.sws_ctx = Some(ctx);
            }

            let Some(sws_ctx) = self.sws_ctx else {
                return Err(DecodeError::Ffmpeg(
                    "SwsContext not initialized".to_string(),
                ));
            };

            // Allocate destination frame (with RAII guard)
            let dst_frame_guard = AvFrameGuard::new()?;
            let dst_frame = dst_frame_guard.as_ptr();

            (*dst_frame).width = width as i32;
            (*dst_frame).height = height as i32;
            (*dst_frame).format = dst_format;

            // Allocate buffer for destination frame
            let buffer_ret = ff_sys::av_frame_get_buffer(dst_frame, 0);
            if buffer_ret < 0 {
                return Err(DecodeError::Ffmpeg(format!(
                    "Failed to allocate frame buffer: {}",
                    ff_sys::av_error_string(buffer_ret)
                )));
            }

            // Perform conversion
            ff_sys::swscale::scale(
                sws_ctx,
                (*self.frame).data.as_ptr() as *const *const u8,
                (*self.frame).linesize.as_ptr(),
                0,
                height as i32,
                (*dst_frame).data.as_ptr() as *const *mut u8,
                (*dst_frame).linesize.as_ptr(),
            )
            .map_err(|e| DecodeError::Ffmpeg(format!("Failed to scale frame: {e}")))?;

            // Copy timestamp
            (*dst_frame).pts = (*self.frame).pts;

            // Convert to VideoFrame
            let video_frame = self.av_frame_to_video_frame(dst_frame)?;

            // dst_frame is automatically freed when guard drops

            Ok(video_frame)
        }
    }

    /// Converts an AVFrame to a VideoFrame.
    unsafe fn av_frame_to_video_frame(
        &self,
        frame: *const AVFrame,
    ) -> Result<VideoFrame, DecodeError> {
        // SAFETY: Caller ensures frame and format_ctx are valid
        unsafe {
            let width = (*frame).width as u32;
            let height = (*frame).height as u32;
            let format = Self::convert_pixel_format((*frame).format);

            // Extract timestamp
            let pts = (*frame).pts;
            let timestamp = if pts != ff_sys::AV_NOPTS_VALUE {
                let stream = (*self.format_ctx).streams.add(self.stream_index as usize);
                let time_base = (*(*stream)).time_base;
                Timestamp::new(
                    pts as i64,
                    Rational::new(time_base.num as i32, time_base.den as i32),
                )
            } else {
                Timestamp::default()
            };

            // Convert frame to planes and strides
            let (planes, strides) =
                self.extract_planes_and_strides(frame, width, height, format)?;

            VideoFrame::new(planes, strides, width, height, format, timestamp, false)
                .map_err(|e| DecodeError::Ffmpeg(format!("Failed to create VideoFrame: {e}")))
        }
    }

    /// Allocates a buffer, optionally using the frame pool.
    ///
    /// If a frame pool is configured and has available buffers, uses the pool.
    /// Otherwise, allocates a new Vec<u8>.
    ///
    /// Allocates a buffer for decoded frame data.
    ///
    /// If a frame pool is configured, attempts to acquire a buffer from the pool.
    /// The returned PooledBuffer will automatically be returned to the pool when dropped.
    fn allocate_buffer(&self, size: usize) -> PooledBuffer {
        if let Some(ref pool) = self.frame_pool
            && let Some(pooled_buffer) = pool.acquire(size)
        {
            // Return the pooled buffer directly - it will be automatically
            // returned to the pool when the VideoFrame is dropped
            return pooled_buffer;
        }

        // Pool not available or exhausted - allocate a standalone buffer
        PooledBuffer::standalone(vec![0u8; size])
    }

    /// Extracts planes and strides from an AVFrame.
    unsafe fn extract_planes_and_strides(
        &self,
        frame: *const AVFrame,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<(Vec<PooledBuffer>, Vec<usize>), DecodeError> {
        // Bytes per pixel constants for different pixel formats
        const BYTES_PER_PIXEL_RGBA: usize = 4;
        const BYTES_PER_PIXEL_RGB24: usize = 3;

        // SAFETY: Caller ensures frame is valid and format matches actual frame format
        unsafe {
            let mut planes = Vec::new();
            let mut strides = Vec::new();

            #[allow(clippy::match_same_arms)]
            match format {
                PixelFormat::Rgba | PixelFormat::Bgra | PixelFormat::Rgb24 | PixelFormat::Bgr24 => {
                    // Packed formats - single plane
                    let stride = (*frame).linesize[0] as usize;
                    let bytes_per_pixel = if matches!(format, PixelFormat::Rgba | PixelFormat::Bgra)
                    {
                        BYTES_PER_PIXEL_RGBA
                    } else {
                        BYTES_PER_PIXEL_RGB24
                    };
                    let row_size = (width as usize) * bytes_per_pixel;
                    let buffer_size = row_size * height as usize;
                    let mut plane_data = self.allocate_buffer(buffer_size);

                    for y in 0..height as usize {
                        let src_offset = y * stride;
                        let dst_offset = y * row_size;
                        let src_ptr = (*frame).data[0].add(src_offset);
                        let plane_slice = plane_data.as_mut();
                        // SAFETY: We copy exactly `row_size` bytes per row. The source pointer
                        // is valid (from FFmpeg frame data), destination has sufficient capacity
                        // (allocated with height * row_size), and ranges don't overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            plane_slice[dst_offset..].as_mut_ptr(),
                            row_size,
                        );
                    }

                    planes.push(plane_data);
                    strides.push(row_size);
                }
                PixelFormat::Yuv420p | PixelFormat::Yuv422p | PixelFormat::Yuv444p => {
                    // Planar YUV formats
                    let (chroma_width, chroma_height) = match format {
                        PixelFormat::Yuv420p => (width / 2, height / 2),
                        PixelFormat::Yuv422p => (width / 2, height),
                        PixelFormat::Yuv444p => (width, height),
                        _ => unreachable!(),
                    };

                    // Y plane
                    let y_stride = width as usize;
                    let y_size = y_stride * height as usize;
                    let mut y_data = self.allocate_buffer(y_size);
                    for y in 0..height as usize {
                        let src_offset = y * (*frame).linesize[0] as usize;
                        let dst_offset = y * y_stride;
                        let src_ptr = (*frame).data[0].add(src_offset);
                        let y_slice = y_data.as_mut();
                        // SAFETY: Copying Y plane row-by-row. Source is valid FFmpeg data,
                        // destination has sufficient capacity, no overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            y_slice[dst_offset..].as_mut_ptr(),
                            width as usize,
                        );
                    }
                    planes.push(y_data);
                    strides.push(y_stride);

                    // U plane
                    let u_stride = chroma_width as usize;
                    let u_size = u_stride * chroma_height as usize;
                    let mut u_data = self.allocate_buffer(u_size);
                    for y in 0..chroma_height as usize {
                        let src_offset = y * (*frame).linesize[1] as usize;
                        let dst_offset = y * u_stride;
                        let src_ptr = (*frame).data[1].add(src_offset);
                        let u_slice = u_data.as_mut();
                        // SAFETY: Copying U (chroma) plane row-by-row. Valid source,
                        // sufficient destination capacity, no overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            u_slice[dst_offset..].as_mut_ptr(),
                            chroma_width as usize,
                        );
                    }
                    planes.push(u_data);
                    strides.push(u_stride);

                    // V plane
                    let v_stride = chroma_width as usize;
                    let v_size = v_stride * chroma_height as usize;
                    let mut v_data = self.allocate_buffer(v_size);
                    for y in 0..chroma_height as usize {
                        let src_offset = y * (*frame).linesize[2] as usize;
                        let dst_offset = y * v_stride;
                        let src_ptr = (*frame).data[2].add(src_offset);
                        let v_slice = v_data.as_mut();
                        // SAFETY: Copying V (chroma) plane row-by-row. Valid source,
                        // sufficient destination capacity, no overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            v_slice[dst_offset..].as_mut_ptr(),
                            chroma_width as usize,
                        );
                    }
                    planes.push(v_data);
                    strides.push(v_stride);
                }
                PixelFormat::Gray8 => {
                    // Single plane grayscale
                    let stride = width as usize;
                    let mut plane_data = self.allocate_buffer(stride * height as usize);

                    for y in 0..height as usize {
                        let src_offset = y * (*frame).linesize[0] as usize;
                        let dst_offset = y * stride;
                        let src_ptr = (*frame).data[0].add(src_offset);
                        let plane_slice = plane_data.as_mut();
                        // SAFETY: Copying grayscale plane row-by-row. Valid source,
                        // sufficient destination capacity, no overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            plane_slice[dst_offset..].as_mut_ptr(),
                            width as usize,
                        );
                    }

                    planes.push(plane_data);
                    strides.push(stride);
                }
                PixelFormat::Nv12 | PixelFormat::Nv21 => {
                    // Semi-planar formats
                    let uv_height = height / 2;

                    // Y plane
                    let y_stride = width as usize;
                    let mut y_data = self.allocate_buffer(y_stride * height as usize);
                    for y in 0..height as usize {
                        let src_offset = y * (*frame).linesize[0] as usize;
                        let dst_offset = y * y_stride;
                        let src_ptr = (*frame).data[0].add(src_offset);
                        let y_slice = y_data.as_mut();
                        // SAFETY: Copying Y plane (semi-planar) row-by-row. Valid source,
                        // sufficient destination capacity, no overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            y_slice[dst_offset..].as_mut_ptr(),
                            width as usize,
                        );
                    }
                    planes.push(y_data);
                    strides.push(y_stride);

                    // UV plane
                    let uv_stride = width as usize;
                    let mut uv_data = self.allocate_buffer(uv_stride * uv_height as usize);
                    for y in 0..uv_height as usize {
                        let src_offset = y * (*frame).linesize[1] as usize;
                        let dst_offset = y * uv_stride;
                        let src_ptr = (*frame).data[1].add(src_offset);
                        let uv_slice = uv_data.as_mut();
                        // SAFETY: Copying interleaved UV plane (semi-planar) row-by-row.
                        // Valid source, sufficient destination capacity, no overlap.
                        std::ptr::copy_nonoverlapping(
                            src_ptr,
                            uv_slice[dst_offset..].as_mut_ptr(),
                            width as usize,
                        );
                    }
                    planes.push(uv_data);
                    strides.push(uv_stride);
                }
                _ => {
                    return Err(DecodeError::Ffmpeg(format!(
                        "Unsupported pixel format: {format:?}"
                    )));
                }
            }

            Ok((planes, strides))
        }
    }

    /// Converts our `PixelFormat` to FFmpeg `AVPixelFormat`.
    fn pixel_format_to_av(format: PixelFormat) -> AVPixelFormat {
        match format {
            PixelFormat::Yuv420p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P,
            PixelFormat::Yuv422p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P,
            PixelFormat::Yuv444p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P,
            PixelFormat::Rgb24 => ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24,
            PixelFormat::Bgr24 => ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24,
            PixelFormat::Rgba => ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA,
            PixelFormat::Bgra => ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA,
            PixelFormat::Gray8 => ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8,
            PixelFormat::Nv12 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV12,
            PixelFormat::Nv21 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV21,
            _ => {
                log::warn!("pixel_format has no AV mapping, falling back to Yuv420p format={format:?} fallback=AV_PIX_FMT_YUV420P");
                ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P
            }
        }
    }

    /// Returns the current playback position.
    pub(crate) fn position(&self) -> Duration {
        self.position
    }

    /// Returns whether end of file has been reached.
    pub(crate) fn is_eof(&self) -> bool {
        self.eof
    }

    /// Converts a `Duration` to a presentation timestamp (PTS) in stream time_base units.
    ///
    /// # Arguments
    ///
    /// * `duration` - The duration to convert.
    ///
    /// # Returns
    ///
    /// The timestamp in stream time_base units.
    ///
    /// # Note
    ///
    /// av_seek_frame expects timestamps in stream time_base units when using a specific stream_index.
    fn duration_to_pts(&self, duration: Duration) -> i64 {
        // Convert duration to stream time_base units for seeking
        // SAFETY:
        // - format_ctx is valid: owned by VideoDecoderInner, initialized in constructor via avformat_open_input
        // - stream_index is valid: validated during decoder creation (find_stream_info + codec opening)
        // - streams array access is valid: guaranteed by FFmpeg after successful avformat_open_input
        let time_base = unsafe {
            let stream = (*self.format_ctx).streams.add(self.stream_index as usize);
            (*(*stream)).time_base
        };

        // Convert: duration (seconds) * (time_base.den / time_base.num) = PTS
        let time_base_f64 = time_base.den as f64 / time_base.num as f64;
        (duration.as_secs_f64() * time_base_f64) as i64
    }

    /// Converts a presentation timestamp (PTS) to a `Duration`.
    ///
    /// # Arguments
    ///
    /// * `pts` - The presentation timestamp in stream time base units.
    ///
    /// # Returns
    ///
    /// The duration corresponding to the PTS.
    ///
    /// # Safety
    ///
    /// Caller must ensure that `format_ctx` and `stream_index` are valid.
    ///
    /// # Note
    ///
    /// Currently unused but kept for potential future use in more advanced seeking scenarios.
    #[allow(dead_code)]
    fn pts_to_duration(&self, pts: i64) -> Duration {
        // SAFETY: Caller ensures format_ctx and stream_index are valid
        unsafe {
            let stream = (*self.format_ctx).streams.add(self.stream_index as usize);
            let time_base = (*(*stream)).time_base;

            // Convert PTS to duration
            let duration_secs = pts as f64 * time_base.num as f64 / time_base.den as f64;
            Duration::from_secs_f64(duration_secs)
        }
    }

    /// Seeks to a specified position in the video stream.
    ///
    /// This method performs efficient seeking without reopening the file.
    /// It uses `av_seek_frame` internally and flushes the decoder buffers.
    ///
    /// # Performance Characteristics
    ///
    /// - **Keyframe seek**: 5-10ms for typical GOP sizes (1-2 seconds)
    /// - **Exact seek**: Proportional to distance from nearest keyframe
    /// - **Large GOP videos**: May require sequential decoding from distant keyframe
    ///
    /// For videos with sparse keyframes (GOP > 2 seconds), the method will
    /// decode frames sequentially from the nearest keyframe to reach the target.
    /// This ensures correct frame data but may take longer (10-50ms for very large GOPs).
    ///
    /// # Arguments
    ///
    /// * `position` - Target position to seek to.
    /// * `mode` - Seek mode (Keyframe, Exact, or Backward).
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::SeekFailed`] if the seek operation fails.
    pub(crate) fn seek(
        &mut self,
        position: Duration,
        mode: crate::SeekMode,
    ) -> Result<(), DecodeError> {
        use crate::SeekMode;

        let timestamp = self.duration_to_pts(position);

        // All seek modes use BACKWARD flag to find the nearest keyframe at or before target.
        // The difference between modes is in the post-seek processing below.
        let flags = ff_sys::avformat::seek_flags::BACKWARD;

        // 1. Clear any pending packet and frame to avoid reading stale data after seek
        // SAFETY:
        // - packet is valid: allocated in constructor, owned by VideoDecoderInner
        // - frame is valid: allocated in constructor, owned by VideoDecoderInner
        unsafe {
            ff_sys::av_packet_unref(self.packet);
            ff_sys::av_frame_unref(self.frame);
        }

        // 2. Seek in the format context (file is NOT reopened)
        // Use av_seek_frame with the stream index and timestamp in stream time_base units
        // SAFETY:
        // - format_ctx is valid: owned by VideoDecoderInner, initialized via avformat_open_input
        // - stream_index is valid: validated during decoder creation
        // - timestamp is valid: converted from Duration using stream's time_base
        unsafe {
            ff_sys::avformat::seek_frame(
                self.format_ctx,
                self.stream_index as i32,
                timestamp,
                flags,
            )
            .map_err(|e| DecodeError::SeekFailed {
                target: position,
                reason: ff_sys::av_error_string(e),
            })?;
        }

        // 3. Flush decoder buffers to clear any cached frames
        // SAFETY: codec_ctx is valid: owned by VideoDecoderInner, initialized via avcodec_open2
        unsafe {
            ff_sys::avcodec::flush_buffers(self.codec_ctx);
        }

        // 4. Drain any remaining frames from the decoder after flush
        // This ensures no stale frames are returned after the seek
        // SAFETY:
        // - codec_ctx is valid: owned by VideoDecoderInner, initialized via avcodec_open2
        // - frame is valid: allocated in constructor, owned by VideoDecoderInner
        unsafe {
            loop {
                let ret = ff_sys::avcodec_receive_frame(self.codec_ctx, self.frame);
                if ret == ff_sys::error_codes::EAGAIN || ret == ff_sys::error_codes::EOF {
                    // No more frames in the decoder buffer
                    break;
                } else if ret == 0 {
                    // Got a frame, unref it and continue draining
                    ff_sys::av_frame_unref(self.frame);
                } else {
                    // Other error, break out
                    break;
                }
            }
        }

        // 5. Reset internal state
        self.eof = false;
        // Note: We don't update self.position here because it will be updated
        // when the next frame is decoded. This ensures position reflects actual decoded position.

        // 6. Skip forward to the target position
        //
        // Context: av_seek_frame with BACKWARD flag seeks to the nearest keyframe *at or before*
        // the target timestamp. For videos with sparse keyframes (large GOP size), this may
        // land far from the target (e.g., at the first keyframe for GOP=entire video).
        //
        // Solution: Decode frames sequentially from the keyframe until reaching the target.
        // This is necessary because H.264/H.265 P-frames and B-frames depend on previous
        // frames for reconstruction, so we must decode all intermediate frames.
        //
        // Performance Impact:
        // - Typical GOP (1-2s): 30-60 frames to skip, ~5-10ms overhead
        // - Large GOP (5-10s): 150-300 frames to skip, ~20-50ms overhead
        // - Worst case (single keyframe): May decode entire video, ~100ms-1s
        if mode == SeekMode::Exact {
            // For exact mode, decode until we reach or pass the exact target
            self.skip_to_exact(position)?;
        } else {
            // For keyframe/backward modes, decode until we're reasonably close to the target
            // Rationale: Balances accuracy with performance for common use cases
            let tolerance = Duration::from_secs(KEYFRAME_SEEK_TOLERANCE_SECS);
            let min_position = position.saturating_sub(tolerance);

            while let Some(frame) = self.decode_one()? {
                let frame_time = frame.timestamp().as_duration();
                if frame_time >= min_position {
                    // We're close enough to the target
                    break;
                }
                // Continue decoding to get closer (frames are automatically dropped)
            }
        }

        Ok(())
    }

    /// Skips frames until reaching the exact target position.
    ///
    /// This is used by [`Self::seek`] when `SeekMode::Exact` is specified.
    /// It decodes and discards frames from the nearest keyframe until
    /// reaching the target position.
    ///
    /// # Performance
    ///
    /// Time complexity is O(n) where n is the number of frames between the
    /// keyframe and target. For a 30fps video with 2-second GOP:
    /// - Worst case: ~60 frames to decode, ~10-20ms
    /// - Average case: ~30 frames to decode, ~5-10ms
    ///
    /// # Arguments
    ///
    /// * `target` - The exact target position.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::SeekFailed`] if EOF is reached before the target position.
    fn skip_to_exact(&mut self, target: Duration) -> Result<(), DecodeError> {
        loop {
            match self.decode_one()? {
                Some(frame) => {
                    let frame_time = frame.timestamp().as_duration();
                    if frame_time >= target {
                        // Reached or passed the target frame
                        // Position will be updated by decode_one() which was just called
                        break;
                    }
                    // Continue decoding (frame is automatically dropped)
                }
                None => {
                    // Reached EOF before finding target frame
                    return Err(DecodeError::SeekFailed {
                        target,
                        reason: "Reached end of stream before target position".to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Flushes the decoder's internal buffers.
    ///
    /// This clears any cached frames and resets the decoder state.
    /// The decoder is ready to receive new packets after flushing.
    pub(crate) fn flush(&mut self) {
        // SAFETY: codec_ctx is valid and owned by this instance
        unsafe {
            ff_sys::avcodec::flush_buffers(self.codec_ctx);
        }
        self.eof = false;
    }

    /// Scales a video frame to the specified dimensions while preserving aspect ratio.
    ///
    /// This method uses SwScale to resize frames efficiently using a "fit-within"
    /// strategy that preserves the original aspect ratio.
    ///
    /// # Aspect Ratio Preservation
    ///
    /// The frame is scaled to fit within `(target_width, target_height)` while
    /// maintaining its original aspect ratio. The output dimensions will be at most
    /// the target size, with at least one dimension matching the target. No letterboxing
    /// or pillarboxing is applied - the frame is simply scaled down to fit.
    ///
    /// # Arguments
    ///
    /// * `frame` - The source frame to scale.
    /// * `target_width` - Desired width in pixels.
    /// * `target_height` - Desired height in pixels.
    ///
    /// # Returns
    ///
    /// A new `VideoFrame` scaled to fit within the target dimensions.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError`] if SwScale context creation or scaling fails.
    ///
    /// # Performance
    ///
    /// - Caches SwScale context for repeated calls with same dimensions
    /// - Context creation: ~0.1-0.5ms (only on first call or dimension change)
    /// - Typical scaling time: 1-3ms for 1080p → 320x180
    /// - Uses bilinear interpolation for quality/performance balance
    ///
    /// # Cache Behavior
    ///
    /// The SwScale context is cached based on source/target dimensions and format.
    /// When generating multiple thumbnails with the same size (e.g., via `thumbnails()`),
    /// the context is reused, eliminating the ~0.1-0.5ms creation overhead per thumbnail.
    pub(crate) fn scale_frame(
        &mut self,
        frame: &VideoFrame,
        target_width: u32,
        target_height: u32,
    ) -> Result<VideoFrame, DecodeError> {
        let src_width = frame.width();
        let src_height = frame.height();
        let src_format = frame.format();

        // Calculate scaled dimensions to preserve aspect ratio (fit within target)
        let src_aspect = src_width as f64 / src_height as f64;
        let target_aspect = target_width as f64 / target_height as f64;

        let (scaled_width, scaled_height) = if src_aspect > target_aspect {
            // Source is wider - fit to width
            let height = (target_width as f64 / src_aspect).round() as u32;
            (target_width, height)
        } else {
            // Source is taller or equal - fit to height
            let width = (target_height as f64 * src_aspect).round() as u32;
            (width, target_height)
        };

        // Convert pixel format to FFmpeg format
        let av_format = Self::pixel_format_to_av(src_format);

        // Cache key: (src_width, src_height, scaled_width, scaled_height, format)
        let cache_key = (
            src_width,
            src_height,
            scaled_width,
            scaled_height,
            av_format,
        );

        // SAFETY: We're creating temporary FFmpeg objects for scaling
        unsafe {
            // Check if we can reuse the cached SwScale context
            let (sws_ctx, is_cached) = if let (Some(cached_ctx), Some(cached_key)) =
                (self.thumbnail_sws_ctx, self.thumbnail_cache_key)
            {
                if cached_key == cache_key {
                    // Cache hit - reuse existing context
                    (cached_ctx, true)
                } else {
                    // Cache miss - free old context and create new one
                    ff_sys::swscale::free_context(cached_ctx);
                    // Clear cache immediately to prevent dangling pointer
                    self.thumbnail_sws_ctx = None;
                    self.thumbnail_cache_key = None;

                    let new_ctx = ff_sys::swscale::get_context(
                        src_width as i32,
                        src_height as i32,
                        av_format,
                        scaled_width as i32,
                        scaled_height as i32,
                        av_format,
                        ff_sys::swscale::scale_flags::BILINEAR,
                    )
                    .map_err(|e| {
                        DecodeError::Ffmpeg(format!("Failed to create scaling context: {e}"))
                    })?;

                    // Don't cache yet - will cache after successful scaling
                    (new_ctx, false)
                }
            } else {
                // No cache - create new context
                let new_ctx = ff_sys::swscale::get_context(
                    src_width as i32,
                    src_height as i32,
                    av_format,
                    scaled_width as i32,
                    scaled_height as i32,
                    av_format,
                    ff_sys::swscale::scale_flags::BILINEAR,
                )
                .map_err(|e| {
                    DecodeError::Ffmpeg(format!("Failed to create scaling context: {e}"))
                })?;

                // Don't cache yet - will cache after successful scaling
                (new_ctx, false)
            };

            // Set up source frame with VideoFrame data
            let src_frame_guard = AvFrameGuard::new()?;
            let src_frame = src_frame_guard.as_ptr();

            (*src_frame).width = src_width as i32;
            (*src_frame).height = src_height as i32;
            (*src_frame).format = av_format;

            // Set up source frame data pointers directly from VideoFrame (no copy)
            let planes = frame.planes();
            let strides = frame.strides();

            for (i, plane_data) in planes.iter().enumerate() {
                if i >= ff_sys::AV_NUM_DATA_POINTERS as usize {
                    break;
                }
                (*src_frame).data[i] = plane_data.as_ref().as_ptr().cast_mut();
                (*src_frame).linesize[i] = strides[i] as i32;
            }

            // Allocate destination frame
            let dst_frame_guard = AvFrameGuard::new()?;
            let dst_frame = dst_frame_guard.as_ptr();

            (*dst_frame).width = scaled_width as i32;
            (*dst_frame).height = scaled_height as i32;
            (*dst_frame).format = av_format;

            // Allocate buffer for destination frame
            let buffer_ret = ff_sys::av_frame_get_buffer(dst_frame, 0);
            if buffer_ret < 0 {
                // Clean up context if not cached
                if !is_cached {
                    ff_sys::swscale::free_context(sws_ctx);
                }
                return Err(DecodeError::Ffmpeg(format!(
                    "Failed to allocate destination frame buffer: {}",
                    ff_sys::av_error_string(buffer_ret)
                )));
            }

            // Perform scaling
            let scale_result = ff_sys::swscale::scale(
                sws_ctx,
                (*src_frame).data.as_ptr() as *const *const u8,
                (*src_frame).linesize.as_ptr(),
                0,
                src_height as i32,
                (*dst_frame).data.as_ptr() as *const *mut u8,
                (*dst_frame).linesize.as_ptr(),
            );

            if let Err(e) = scale_result {
                // Clean up context if not cached
                if !is_cached {
                    ff_sys::swscale::free_context(sws_ctx);
                }
                return Err(DecodeError::Ffmpeg(format!("Failed to scale frame: {e}")));
            }

            // Scaling successful - cache the context if it's new
            if !is_cached {
                self.thumbnail_sws_ctx = Some(sws_ctx);
                self.thumbnail_cache_key = Some(cache_key);
            }

            // Copy timestamp
            (*dst_frame).pts = frame.timestamp().pts();

            // Convert destination frame to VideoFrame
            let video_frame = self.av_frame_to_video_frame(dst_frame)?;

            Ok(video_frame)
        }
    }
}

impl Drop for VideoDecoderInner {
    fn drop(&mut self) {
        // Free SwScale context if allocated
        if let Some(sws_ctx) = self.sws_ctx {
            // SAFETY: sws_ctx is valid and owned by this instance
            unsafe {
                ff_sys::swscale::free_context(sws_ctx);
            }
        }

        // Free cached thumbnail SwScale context if allocated
        if let Some(thumbnail_ctx) = self.thumbnail_sws_ctx {
            // SAFETY: thumbnail_ctx is valid and owned by this instance
            unsafe {
                ff_sys::swscale::free_context(thumbnail_ctx);
            }
        }

        // Free hardware device context if allocated
        if let Some(hw_ctx) = self.hw_device_ctx {
            // SAFETY: hw_ctx is valid and owned by this instance
            unsafe {
                ff_sys::av_buffer_unref(&mut (hw_ctx as *mut _));
            }
        }

        // Free frame and packet
        if !self.frame.is_null() {
            // SAFETY: self.frame is valid and owned by this instance
            unsafe {
                ff_sys::av_frame_free(&mut (self.frame as *mut _));
            }
        }

        if !self.packet.is_null() {
            // SAFETY: self.packet is valid and owned by this instance
            unsafe {
                ff_sys::av_packet_free(&mut (self.packet as *mut _));
            }
        }

        // Free codec context
        if !self.codec_ctx.is_null() {
            // SAFETY: self.codec_ctx is valid and owned by this instance
            unsafe {
                ff_sys::avcodec::free_context(&mut (self.codec_ctx as *mut _));
            }
        }

        // Close format context
        if !self.format_ctx.is_null() {
            // SAFETY: self.format_ctx is valid and owned by this instance
            unsafe {
                ff_sys::avformat::close_input(&mut (self.format_ctx as *mut _));
            }
        }
    }
}

// SAFETY: VideoDecoderInner manages FFmpeg contexts which are thread-safe when not shared.
// We don't expose mutable access across threads, so Send is safe.
unsafe impl Send for VideoDecoderInner {}
