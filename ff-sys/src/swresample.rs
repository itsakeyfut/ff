//! SwResample wrapper functions for audio resampling and format conversion.
//!
//! This module provides thin wrapper functions around FFmpeg's libswresample API
//! for resampling audio data and converting between sample formats and channel layouts.
//!
//! # Safety
//!
//! Callers are responsible for:
//! - Ensuring pointers are valid before passing to these functions
//! - Properly freeing resources using the corresponding free functions
//! - Not using pointers after they have been freed
//! - Ensuring source and destination buffers match the expected sample counts and formats

use std::os::raw::c_int;

use crate::{
    AVChannelLayout, AVSampleFormat, SwrContext, ensure_initialized, swr_alloc as ffi_swr_alloc,
    swr_alloc_set_opts2 as ffi_swr_alloc_set_opts2, swr_convert as ffi_swr_convert,
    swr_free as ffi_swr_free, swr_get_delay as ffi_swr_get_delay, swr_init as ffi_swr_init,
    swr_is_initialized as ffi_swr_is_initialized,
};

// ============================================================================
// Constants
// ============================================================================

/// Headroom samples added to output buffer estimates.
///
/// This accounts for resampling filter delay and rounding errors.
/// The value of 256 samples provides sufficient headroom for most
/// resampling scenarios without significant memory overhead.
const RESAMPLE_HEADROOM_SAMPLES: i32 = 256;

// ============================================================================
// Common channel layout constants
// ============================================================================

/// Common channel layout helpers and constants.
///
/// FFmpeg 7.x uses `AVChannelLayout` struct instead of the deprecated
/// `int64_t` channel masks. Use `av_channel_layout_*` functions to work
/// with channel layouts.
pub mod channel_layout {
    use crate::{
        AVChannelLayout, AVChannelOrder_AV_CHANNEL_ORDER_NATIVE, av_channel_layout_compare,
        av_channel_layout_copy, av_channel_layout_default, av_channel_layout_uninit,
    };

    /// Initialize a channel layout with a default layout for the given number of channels.
    ///
    /// # Arguments
    ///
    /// * `ch_layout` - Pointer to the channel layout to initialize
    /// * `nb_channels` - Number of channels
    ///
    /// # Safety
    ///
    /// - The channel layout pointer must be valid and uninitialized (or zeroed).
    /// - If `ch_layout` is null, this function is a no-op (silently ignored).
    pub unsafe fn set_default(ch_layout: *mut AVChannelLayout, nb_channels: i32) {
        if !ch_layout.is_null() {
            av_channel_layout_default(ch_layout, nb_channels);
        }
    }

    /// Uninitialize a channel layout and reset it to a zeroed state.
    ///
    /// # Arguments
    ///
    /// * `ch_layout` - Pointer to the channel layout to uninitialize
    ///
    /// # Safety
    ///
    /// - The channel layout pointer must be valid.
    /// - If `ch_layout` is null, this function is a no-op (silently ignored).
    pub unsafe fn uninit(ch_layout: *mut AVChannelLayout) {
        if !ch_layout.is_null() {
            av_channel_layout_uninit(ch_layout);
        }
    }

    /// Copy a channel layout.
    ///
    /// # Arguments
    ///
    /// * `dst` - Destination channel layout
    /// * `src` - Source channel layout
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error code on failure.
    ///
    /// # Safety
    ///
    /// Both pointers must be valid.
    pub unsafe fn copy(dst: *mut AVChannelLayout, src: *const AVChannelLayout) -> Result<(), i32> {
        if dst.is_null() || src.is_null() {
            return Err(crate::error_codes::EINVAL);
        }

        let ret = av_channel_layout_copy(dst, src);
        if ret < 0 { Err(ret) } else { Ok(()) }
    }

    /// Compare two channel layouts.
    ///
    /// # Arguments
    ///
    /// * `chl` - First channel layout
    /// * `chl1` - Second channel layout
    ///
    /// # Returns
    ///
    /// Returns `true` if the layouts are identical, `false` otherwise.
    ///
    /// # Safety
    ///
    /// Both pointers must be valid.
    pub unsafe fn is_equal(chl: *const AVChannelLayout, chl1: *const AVChannelLayout) -> bool {
        if chl.is_null() || chl1.is_null() {
            return false;
        }

        av_channel_layout_compare(chl, chl1) == 0
    }

    /// Create a mono channel layout.
    ///
    /// # Returns
    ///
    /// Returns a mono (1 channel) layout.
    pub fn mono() -> AVChannelLayout {
        let mut layout = AVChannelLayout::default();
        unsafe {
            av_channel_layout_default(&mut layout, 1);
        }
        layout
    }

    /// Create a stereo channel layout.
    ///
    /// # Returns
    ///
    /// Returns a stereo (2 channel) layout.
    pub fn stereo() -> AVChannelLayout {
        let mut layout = AVChannelLayout::default();
        unsafe {
            av_channel_layout_default(&mut layout, 2);
        }
        layout
    }

    /// Create a channel layout with the specified number of channels.
    ///
    /// Uses the default layout for that channel count (e.g., 2 = stereo, 6 = 5.1).
    ///
    /// # Arguments
    ///
    /// * `nb_channels` - Number of channels
    ///
    /// # Returns
    ///
    /// Returns a channel layout with the default configuration for `nb_channels`.
    pub fn with_channels(nb_channels: i32) -> AVChannelLayout {
        let mut layout = AVChannelLayout::default();
        unsafe {
            av_channel_layout_default(&mut layout, nb_channels);
        }
        layout
    }

    /// Check if a channel layout is valid.
    ///
    /// # Arguments
    ///
    /// * `ch_layout` - The channel layout to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the layout has at least one channel, `false` otherwise.
    pub fn is_valid(ch_layout: &AVChannelLayout) -> bool {
        ch_layout.nb_channels > 0
    }

    /// Get the number of channels in a layout.
    ///
    /// # Arguments
    ///
    /// * `ch_layout` - The channel layout
    ///
    /// # Returns
    ///
    /// Returns the number of channels.
    pub fn nb_channels(ch_layout: &AVChannelLayout) -> i32 {
        ch_layout.nb_channels
    }

    /// Check if a channel layout uses native order.
    ///
    /// # Arguments
    ///
    /// * `ch_layout` - The channel layout to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the layout uses native channel order.
    pub fn is_native_order(ch_layout: &AVChannelLayout) -> bool {
        ch_layout.order == AVChannelOrder_AV_CHANNEL_ORDER_NATIVE
    }
}

// ============================================================================
// Sample format helpers
// ============================================================================

/// Sample format helpers and utilities.
pub mod sample_format {
    use crate::{
        AVSampleFormat, AVSampleFormat_AV_SAMPLE_FMT_DBL, AVSampleFormat_AV_SAMPLE_FMT_DBLP,
        AVSampleFormat_AV_SAMPLE_FMT_FLT, AVSampleFormat_AV_SAMPLE_FMT_FLTP,
        AVSampleFormat_AV_SAMPLE_FMT_NONE, AVSampleFormat_AV_SAMPLE_FMT_S16,
        AVSampleFormat_AV_SAMPLE_FMT_S16P, AVSampleFormat_AV_SAMPLE_FMT_S32,
        AVSampleFormat_AV_SAMPLE_FMT_S32P, AVSampleFormat_AV_SAMPLE_FMT_S64,
        AVSampleFormat_AV_SAMPLE_FMT_S64P, AVSampleFormat_AV_SAMPLE_FMT_U8,
        AVSampleFormat_AV_SAMPLE_FMT_U8P, av_get_bytes_per_sample as ffi_av_get_bytes_per_sample,
        av_sample_fmt_is_planar as ffi_av_sample_fmt_is_planar,
    };

    // Re-export common sample formats
    pub const NONE: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_NONE;
    pub const U8: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_U8;
    pub const S16: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_S16;
    pub const S32: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_S32;
    pub const S64: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_S64;
    pub const FLT: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_FLT;
    pub const DBL: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_DBL;

    // Planar formats
    pub const U8P: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_U8P;
    pub const S16P: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_S16P;
    pub const S32P: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_S32P;
    pub const S64P: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_S64P;
    pub const FLTP: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_FLTP;
    pub const DBLP: AVSampleFormat = AVSampleFormat_AV_SAMPLE_FMT_DBLP;

    /// Get the number of bytes per sample for a given format.
    ///
    /// # Arguments
    ///
    /// * `sample_fmt` - The sample format
    ///
    /// # Returns
    ///
    /// Returns the number of bytes per sample, or a negative value for invalid formats.
    pub fn bytes_per_sample(sample_fmt: AVSampleFormat) -> i32 {
        unsafe { ffi_av_get_bytes_per_sample(sample_fmt) }
    }

    /// Check if a sample format is planar.
    ///
    /// Planar formats store each channel in a separate plane,
    /// while packed (interleaved) formats store all channels together.
    ///
    /// # Arguments
    ///
    /// * `sample_fmt` - The sample format to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the format is planar, `false` if packed.
    pub fn is_planar(sample_fmt: AVSampleFormat) -> bool {
        unsafe { ffi_av_sample_fmt_is_planar(sample_fmt) != 0 }
    }

    /// Check if a sample format is valid (not NONE).
    ///
    /// # Arguments
    ///
    /// * `sample_fmt` - The sample format to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the format is valid.
    pub fn is_valid(sample_fmt: AVSampleFormat) -> bool {
        sample_fmt != NONE
    }
}

// ============================================================================
// Context allocation and management
// ============================================================================

/// Allocate an empty SwrContext.
///
/// This allocates an uninitialized context that must be configured
/// using `alloc_set_opts2()` or `swr_alloc_set_opts2()` before use.
///
/// # Returns
///
/// Returns a pointer to the allocated context on success,
/// or an error code on failure (typically `ENOMEM`).
///
/// # Safety
///
/// The returned context must be freed using `free()` when no longer needed.
pub unsafe fn alloc() -> Result<*mut SwrContext, c_int> {
    ensure_initialized();

    let ctx = ffi_swr_alloc();
    if ctx.is_null() {
        Err(crate::error_codes::ENOMEM)
    } else {
        Ok(ctx)
    }
}

/// Allocate and configure a SwrContext for audio conversion.
///
/// This is the recommended way to create a resampling context in FFmpeg 7.x.
/// It uses the new `AVChannelLayout` API instead of the deprecated channel masks.
///
/// # Arguments
///
/// * `out_ch_layout` - Output channel layout
/// * `out_sample_fmt` - Output sample format
/// * `out_sample_rate` - Output sample rate in Hz
/// * `in_ch_layout` - Input channel layout
/// * `in_sample_fmt` - Input sample format
/// * `in_sample_rate` - Input sample rate in Hz
///
/// # Returns
///
/// Returns a pointer to the configured context on success,
/// or an error code on failure.
///
/// # Safety
///
/// - The returned context must be initialized with `init()` before use.
/// - The returned context must be freed using `free()` when no longer needed.
/// - Channel layout pointers must be valid for the duration of this call.
///
/// # Example
///
/// ```ignore
/// use ff_sys::swresample::{alloc_set_opts2, init, free, channel_layout, sample_format};
///
/// unsafe {
///     let out_layout = channel_layout::stereo();
///     let in_layout = channel_layout::mono();
///
///     let ctx = alloc_set_opts2(
///         &out_layout, sample_format::FLTP, 48000,
///         &in_layout, sample_format::S16, 44100,
///     )?;
///
///     init(ctx)?;
///     // Use context for conversion...
///     free(&mut ctx);
/// }
/// ```
pub unsafe fn alloc_set_opts2(
    out_ch_layout: *const AVChannelLayout,
    out_sample_fmt: AVSampleFormat,
    out_sample_rate: c_int,
    in_ch_layout: *const AVChannelLayout,
    in_sample_fmt: AVSampleFormat,
    in_sample_rate: c_int,
) -> Result<*mut SwrContext, c_int> {
    ensure_initialized();

    // Validate inputs
    if out_ch_layout.is_null() || in_ch_layout.is_null() {
        return Err(crate::error_codes::EINVAL);
    }

    if out_sample_rate <= 0 || in_sample_rate <= 0 {
        return Err(crate::error_codes::EINVAL);
    }

    let mut ctx: *mut SwrContext = std::ptr::null_mut();

    let ret = ffi_swr_alloc_set_opts2(
        &mut ctx,
        out_ch_layout,
        out_sample_fmt,
        out_sample_rate,
        in_ch_layout,
        in_sample_fmt,
        in_sample_rate,
        0,                    // log_offset
        std::ptr::null_mut(), // log_ctx
    );

    if ret < 0 {
        Err(ret)
    } else if ctx.is_null() {
        Err(crate::error_codes::ENOMEM)
    } else {
        Ok(ctx)
    }
}

/// Initialize a resampling context after all options have been set.
///
/// This must be called after `alloc_set_opts2()` or after manually setting
/// options on an allocated context.
///
/// # Arguments
///
/// * `ctx` - The context to initialize
///
/// # Returns
///
/// Returns `Ok(())` on success, or an FFmpeg error code on failure.
///
/// # Safety
///
/// - The context must be allocated and configured.
/// - The context must not already be initialized.
///
/// # Errors
///
/// Returns a negative error code if:
/// - Context is null (`error_codes::EINVAL`)
/// - Initialization fails (e.g., invalid configuration)
pub unsafe fn init(ctx: *mut SwrContext) -> Result<(), c_int> {
    if ctx.is_null() {
        return Err(crate::error_codes::EINVAL);
    }

    let ret = ffi_swr_init(ctx);

    if ret < 0 { Err(ret) } else { Ok(()) }
}

/// Check if a context is initialized.
///
/// # Arguments
///
/// * `ctx` - The context to check
///
/// # Returns
///
/// Returns `true` if the context is initialized and ready for conversion.
///
/// # Safety
///
/// The context pointer must be valid or null.
pub unsafe fn is_initialized(ctx: *const SwrContext) -> bool {
    if ctx.is_null() {
        return false;
    }

    ffi_swr_is_initialized(ctx.cast_mut()) != 0
}

/// Free a resampling context.
///
/// # Arguments
///
/// * `ctx` - Pointer to a pointer to the context to free
///
/// # Safety
///
/// - The context must have been allocated by `alloc()` or `alloc_set_opts2()`.
/// - After this call, `*ctx` will be set to null.
/// - The context pointer must not be used after this call.
///
/// # Null Safety
///
/// This function safely handles:
/// - `ctx` being null
/// - `*ctx` being null
pub unsafe fn free(ctx: *mut *mut SwrContext) {
    if !ctx.is_null() && !(*ctx).is_null() {
        ffi_swr_free(ctx);
    }
}

// ============================================================================
// Conversion operations
// ============================================================================

/// Convert audio samples.
///
/// Converts input audio samples to the output format configured in the context.
///
/// # Arguments
///
/// * `ctx` - The initialized resampling context
/// * `out` - Array of pointers to output sample planes (planar) or single buffer (packed)
/// * `out_count` - Maximum number of output samples per channel
/// * `in_` - Array of pointers to input sample planes (planar) or single buffer (packed)
/// * `in_count` - Number of input samples per channel
///
/// # Returns
///
/// Returns the number of samples output per channel on success,
/// or a negative error code on failure.
///
/// # Safety
///
/// - The context must be initialized.
/// - Output buffers must be large enough to hold `out_count` samples.
/// - Input buffers must contain at least `in_count` samples.
/// - For planar formats, each plane pointer must be valid.
/// - For packed formats, only the first pointer is used.
///
/// # Flushing
///
/// To flush remaining samples after all input has been processed,
/// call with `in_` set to null and `in_count` set to 0.
///
/// # Example
///
/// ```ignore
/// unsafe {
///     let samples_out = convert(
///         ctx,
///         out_planes.as_ptr(),
///         max_out_samples,
///         in_planes.as_ptr(),
///         in_samples,
///     )?;
/// }
/// ```
pub unsafe fn convert(
    ctx: *mut SwrContext,
    out: *mut *mut u8,
    out_count: c_int,
    in_: *const *const u8,
    in_count: c_int,
) -> Result<c_int, c_int> {
    if ctx.is_null() {
        return Err(crate::error_codes::EINVAL);
    }

    // Note: out and in_ can be null for certain operations (e.g., flushing)
    // but out_count must be valid

    if out_count < 0 || in_count < 0 {
        return Err(crate::error_codes::EINVAL);
    }

    let ret = ffi_swr_convert(ctx, out, out_count, in_, in_count);

    if ret < 0 { Err(ret) } else { Ok(ret) }
}

/// Get the delay (in samples) caused by the resampling filter.
///
/// This is useful for determining how many samples are buffered internally
/// and need to be flushed at the end of a stream.
///
/// # Arguments
///
/// * `ctx` - The resampling context
/// * `base` - Time base for the returned delay (e.g., output sample rate)
///
/// # Returns
///
/// Returns the delay in units of `1/base` seconds.
///
/// # Example
///
/// ```ignore
/// // Get delay in output samples
/// let delay = get_delay(ctx, out_sample_rate);
///
/// // Get delay in input samples
/// let delay = get_delay(ctx, in_sample_rate);
/// ```
///
/// # Safety
///
/// The context pointer must be valid.
pub unsafe fn get_delay(ctx: *mut SwrContext, base: i64) -> i64 {
    if ctx.is_null() {
        return 0;
    }

    ffi_swr_get_delay(ctx, base)
}

// ============================================================================
// Helper functions for buffer size calculation
// ============================================================================

/// Calculate the required output buffer size for a given input size.
///
/// This accounts for the sample rate conversion ratio and any delay
/// in the resampling filter.
///
/// # Arguments
///
/// * `ctx` - The initialized resampling context
/// * `out_sample_rate` - Output sample rate
/// * `in_sample_rate` - Input sample rate
/// * `in_samples` - Number of input samples per channel
///
/// # Returns
///
/// Returns the recommended number of output samples to allocate,
/// or 0 if inputs are invalid.
///
/// # Safety
///
/// The context must be initialized if provided.
pub fn estimate_output_samples(out_sample_rate: i32, in_sample_rate: i32, in_samples: i32) -> i32 {
    if out_sample_rate <= 0 || in_sample_rate <= 0 || in_samples < 0 {
        return 0;
    }

    // Calculate output samples based on sample rate ratio
    // Add extra samples to account for rounding and filter delay
    let ratio = out_sample_rate as f64 / in_sample_rate as f64;
    let estimated = (in_samples as f64 * ratio).ceil() as i32;

    // Add headroom for filter delay
    estimated + RESAMPLE_HEADROOM_SAMPLES
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Channel layout tests
    // ========================================================================

    #[test]
    fn test_channel_layout_mono() {
        let layout = channel_layout::mono();
        assert_eq!(channel_layout::nb_channels(&layout), 1);
        assert!(channel_layout::is_valid(&layout));
    }

    #[test]
    fn test_channel_layout_stereo() {
        let layout = channel_layout::stereo();
        assert_eq!(channel_layout::nb_channels(&layout), 2);
        assert!(channel_layout::is_valid(&layout));
    }

    #[test]
    fn test_channel_layout_with_channels() {
        for n in 1..=8 {
            let layout = channel_layout::with_channels(n);
            assert_eq!(channel_layout::nb_channels(&layout), n);
            assert!(channel_layout::is_valid(&layout));
        }
    }

    #[test]
    fn test_channel_layout_copy() {
        let src = channel_layout::stereo();
        let mut dst = AVChannelLayout::default();

        unsafe {
            let result = channel_layout::copy(&mut dst, &src);
            assert!(result.is_ok());
            assert_eq!(channel_layout::nb_channels(&dst), 2);
            assert!(channel_layout::is_equal(&src, &dst));

            channel_layout::uninit(&mut dst);
        }
    }

    #[test]
    fn test_channel_layout_copy_null() {
        unsafe {
            let result = channel_layout::copy(std::ptr::null_mut(), std::ptr::null());
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_channel_layout_is_equal() {
        let layout1 = channel_layout::stereo();
        let layout2 = channel_layout::stereo();
        let layout3 = channel_layout::mono();

        unsafe {
            assert!(channel_layout::is_equal(&layout1, &layout2));
            assert!(!channel_layout::is_equal(&layout1, &layout3));
        }
    }

    #[test]
    fn test_channel_layout_is_equal_null() {
        let layout = channel_layout::stereo();
        unsafe {
            assert!(!channel_layout::is_equal(std::ptr::null(), &layout));
            assert!(!channel_layout::is_equal(&layout, std::ptr::null()));
        }
    }

    // ========================================================================
    // Sample format tests
    // ========================================================================

    #[test]
    fn test_sample_format_bytes() {
        assert_eq!(sample_format::bytes_per_sample(sample_format::U8), 1);
        assert_eq!(sample_format::bytes_per_sample(sample_format::S16), 2);
        assert_eq!(sample_format::bytes_per_sample(sample_format::S32), 4);
        assert_eq!(sample_format::bytes_per_sample(sample_format::FLT), 4);
        assert_eq!(sample_format::bytes_per_sample(sample_format::DBL), 8);
    }

    #[test]
    fn test_sample_format_is_planar() {
        // Packed formats
        assert!(!sample_format::is_planar(sample_format::U8));
        assert!(!sample_format::is_planar(sample_format::S16));
        assert!(!sample_format::is_planar(sample_format::FLT));

        // Planar formats
        assert!(sample_format::is_planar(sample_format::U8P));
        assert!(sample_format::is_planar(sample_format::S16P));
        assert!(sample_format::is_planar(sample_format::FLTP));
    }

    #[test]
    fn test_sample_format_is_valid() {
        assert!(sample_format::is_valid(sample_format::S16));
        assert!(sample_format::is_valid(sample_format::FLT));
        assert!(!sample_format::is_valid(sample_format::NONE));
    }

    // ========================================================================
    // Context allocation tests
    // ========================================================================

    #[test]
    fn test_alloc_and_free() {
        unsafe {
            let ctx_result = alloc();
            assert!(ctx_result.is_ok(), "Context allocation should succeed");

            let mut ctx = ctx_result.unwrap();
            assert!(!ctx.is_null());

            free(&mut ctx);
            assert!(ctx.is_null(), "Context should be null after free");
        }
    }

    #[test]
    fn test_free_null_safety() {
        unsafe {
            // Passing a null pointer should not crash
            free(std::ptr::null_mut());

            // Passing a pointer to null should not crash
            let mut null_ctx: *mut SwrContext = std::ptr::null_mut();
            free(&mut null_ctx);
        }
    }

    #[test]
    fn test_alloc_set_opts2_and_init() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let ctx_result = alloc_set_opts2(
                &out_layout,
                sample_format::FLTP,
                48000,
                &in_layout,
                sample_format::S16,
                44100,
            );

            assert!(ctx_result.is_ok(), "Context creation should succeed");

            let mut ctx = ctx_result.unwrap();
            assert!(!ctx.is_null());

            // Context should not be initialized yet
            assert!(!is_initialized(ctx));

            // Initialize the context
            let init_result = init(ctx);
            assert!(init_result.is_ok(), "Context initialization should succeed");

            // Now it should be initialized
            assert!(is_initialized(ctx));

            free(&mut ctx);
        }
    }

    #[test]
    fn test_alloc_set_opts2_mono_to_stereo() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::mono();

            let ctx_result = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                &in_layout,
                sample_format::FLT,
                48000,
            );

            assert!(
                ctx_result.is_ok(),
                "Mono to stereo conversion should be supported"
            );

            let mut ctx = ctx_result.unwrap();
            let init_result = init(ctx);
            assert!(init_result.is_ok());

            free(&mut ctx);
        }
    }

    #[test]
    fn test_alloc_set_opts2_invalid_null_layout() {
        unsafe {
            let out_layout = channel_layout::stereo();

            let result = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                std::ptr::null(),
                sample_format::FLT,
                44100,
            );

            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);
        }
    }

    #[test]
    fn test_alloc_set_opts2_invalid_sample_rate() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            // Zero sample rate
            let result = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                0,
                &in_layout,
                sample_format::FLT,
                44100,
            );

            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);

            // Negative sample rate
            let result = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                -1,
                &in_layout,
                sample_format::FLT,
                44100,
            );

            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);
        }
    }

    #[test]
    fn test_init_null() {
        unsafe {
            let result = init(std::ptr::null_mut());
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);
        }
    }

    #[test]
    fn test_is_initialized_null() {
        unsafe {
            assert!(!is_initialized(std::ptr::null()));
        }
    }

    // ========================================================================
    // Conversion tests
    // ========================================================================

    #[test]
    fn test_convert_null_context() {
        unsafe {
            let result = convert(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
                0,
            );

            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);
        }
    }

    #[test]
    fn test_convert_invalid_counts() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let mut ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                &in_layout,
                sample_format::FLT,
                48000,
            )
            .unwrap();

            init(ctx).unwrap();

            // Negative out_count
            let result = convert(ctx, std::ptr::null_mut(), -1, std::ptr::null(), 0);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);

            // Negative in_count
            let result = convert(ctx, std::ptr::null_mut(), 0, std::ptr::null(), -1);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), crate::error_codes::EINVAL);

            free(&mut ctx);
        }
    }

    #[test]
    fn test_convert_s16_to_float() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let mut ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLTP,
                48000,
                &in_layout,
                sample_format::S16,
                48000,
            )
            .unwrap();

            init(ctx).unwrap();

            // Create input data (S16 stereo, interleaved)
            let num_samples = 1024;
            let mut in_data: Vec<i16> = vec![0; num_samples * 2]; // 2 channels

            // Generate a simple sine wave
            for i in 0..num_samples {
                let sample = ((i as f32 / 48.0).sin() * 16000.0) as i16;
                in_data[i * 2] = sample; // Left
                in_data[i * 2 + 1] = sample; // Right
            }

            // Create output buffers (FLTP = planar float, separate planes)
            let out_samples = estimate_output_samples(48000, 48000, num_samples as i32);
            let mut out_left: Vec<f32> = vec![0.0; out_samples as usize];
            let mut out_right: Vec<f32> = vec![0.0; out_samples as usize];

            let in_ptr = in_data.as_ptr() as *const u8;
            let in_ptrs: [*const u8; 1] = [in_ptr];

            let mut out_ptr_left = out_left.as_mut_ptr() as *mut u8;
            let mut out_ptr_right = out_right.as_mut_ptr() as *mut u8;
            let mut out_ptrs: [*mut u8; 2] = [out_ptr_left, out_ptr_right];

            let result = convert(
                ctx,
                out_ptrs.as_mut_ptr(),
                out_samples,
                in_ptrs.as_ptr(),
                num_samples as c_int,
            );

            assert!(result.is_ok(), "Conversion should succeed");
            let samples_out = result.unwrap();
            assert!(samples_out > 0, "Should produce output samples");
            assert!(samples_out <= out_samples, "Should not exceed buffer size");

            // Verify some output data is non-zero
            let non_zero_left = out_left
                .iter()
                .take(samples_out as usize)
                .any(|&x| x != 0.0);
            let non_zero_right = out_right
                .iter()
                .take(samples_out as usize)
                .any(|&x| x != 0.0);
            assert!(
                non_zero_left || non_zero_right,
                "Output should contain data"
            );

            free(&mut ctx);
        }
    }

    #[test]
    fn test_convert_resample_44100_to_48000() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let mut ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                &in_layout,
                sample_format::FLT,
                44100,
            )
            .unwrap();

            init(ctx).unwrap();

            // Create input data (FLT stereo, interleaved)
            let num_samples = 1024;
            let mut in_data: Vec<f32> = vec![0.0; num_samples * 2];

            // Generate test data
            for i in 0..num_samples {
                let sample = (i as f32 / 100.0).sin();
                in_data[i * 2] = sample;
                in_data[i * 2 + 1] = sample;
            }

            // Calculate expected output size
            let out_samples = estimate_output_samples(48000, 44100, num_samples as i32);
            let mut out_data: Vec<f32> = vec![0.0; out_samples as usize * 2];

            let in_ptr = in_data.as_ptr() as *const u8;
            let in_ptrs: [*const u8; 1] = [in_ptr];

            let mut out_ptr = out_data.as_mut_ptr() as *mut u8;
            let mut out_ptrs: [*mut u8; 1] = [out_ptr];

            let result = convert(
                ctx,
                out_ptrs.as_mut_ptr(),
                out_samples,
                in_ptrs.as_ptr(),
                num_samples as c_int,
            );

            assert!(result.is_ok(), "Resampling should succeed");
            let samples_out = result.unwrap();

            // 44100 -> 48000 should produce more samples
            // Expected ratio: 48000/44100 ≈ 1.088
            let expected_min = (num_samples as f32 * 1.05) as i32;
            let expected_max = (num_samples as f32 * 1.15) as i32;

            assert!(
                samples_out >= expected_min && samples_out <= expected_max,
                "Output sample count {} should be between {} and {}",
                samples_out,
                expected_min,
                expected_max
            );

            free(&mut ctx);
        }
    }

    // ========================================================================
    // Delay tests
    // ========================================================================

    #[test]
    fn test_get_delay_null() {
        unsafe {
            let delay = get_delay(std::ptr::null_mut(), 48000);
            assert_eq!(delay, 0);
        }
    }

    #[test]
    fn test_get_delay_after_init() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let mut ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                &in_layout,
                sample_format::FLT,
                44100,
            )
            .unwrap();

            init(ctx).unwrap();

            // Initial delay should be 0 (no samples buffered yet)
            let delay = get_delay(ctx, 48000);
            assert_eq!(delay, 0, "Initial delay should be 0");

            free(&mut ctx);
        }
    }

    // ========================================================================
    // Helper function tests
    // ========================================================================

    #[test]
    fn test_estimate_output_samples_same_rate() {
        let out = estimate_output_samples(48000, 48000, 1024);
        // Should be slightly more than input due to headroom
        assert!(out > 1024);
        assert!(out < 2048);
    }

    #[test]
    fn test_estimate_output_samples_upsampling() {
        // 44100 -> 48000
        let out = estimate_output_samples(48000, 44100, 1024);
        // Expected: 1024 * (48000/44100) ≈ 1115 + headroom
        assert!(out > 1100);
        assert!(out < 1500);
    }

    #[test]
    fn test_estimate_output_samples_downsampling() {
        // 48000 -> 44100
        let out = estimate_output_samples(44100, 48000, 1024);
        // Expected: 1024 * (44100/48000) ≈ 940 + headroom
        assert!(out > 900);
        assert!(out < 1300);
    }

    #[test]
    fn test_estimate_output_samples_invalid() {
        assert_eq!(estimate_output_samples(0, 48000, 1024), 0);
        assert_eq!(estimate_output_samples(48000, 0, 1024), 0);
        assert_eq!(estimate_output_samples(48000, 48000, -1), 0);
        assert_eq!(estimate_output_samples(-1, 48000, 1024), 0);
    }

    // ========================================================================
    // Multiple conversion passes tests
    // ========================================================================

    #[test]
    fn test_multiple_conversions() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let mut ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                &in_layout,
                sample_format::FLT,
                48000,
            )
            .unwrap();

            init(ctx).unwrap();

            let chunk_size = 256;
            let num_chunks = 4;
            let mut in_data: Vec<f32> = vec![0.5; chunk_size * 2];

            let out_samples = estimate_output_samples(48000, 48000, chunk_size as i32);
            let mut out_data: Vec<f32> = vec![0.0; out_samples as usize * 2];

            for _ in 0..num_chunks {
                let in_ptr = in_data.as_ptr() as *const u8;
                let in_ptrs: [*const u8; 1] = [in_ptr];

                let mut out_ptr = out_data.as_mut_ptr() as *mut u8;
                let mut out_ptrs: [*mut u8; 1] = [out_ptr];

                let result = convert(
                    ctx,
                    out_ptrs.as_mut_ptr(),
                    out_samples,
                    in_ptrs.as_ptr(),
                    chunk_size as c_int,
                );

                assert!(result.is_ok(), "Conversion should succeed on each pass");
            }

            free(&mut ctx);
        }
    }

    // ========================================================================
    // Flush test
    // ========================================================================

    #[test]
    fn test_flush_remaining_samples() {
        unsafe {
            let out_layout = channel_layout::stereo();
            let in_layout = channel_layout::stereo();

            let mut ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLT,
                48000,
                &in_layout,
                sample_format::FLT,
                44100, // Different rate to ensure buffering
            )
            .unwrap();

            init(ctx).unwrap();

            // Convert some data first
            let num_samples = 1024;
            let mut in_data: Vec<f32> = vec![0.5; num_samples * 2];
            let out_samples = estimate_output_samples(48000, 44100, num_samples as i32);
            let mut out_data: Vec<f32> = vec![0.0; out_samples as usize * 2];

            let in_ptr = in_data.as_ptr() as *const u8;
            let in_ptrs: [*const u8; 1] = [in_ptr];

            let mut out_ptr = out_data.as_mut_ptr() as *mut u8;
            let mut out_ptrs: [*mut u8; 1] = [out_ptr];

            convert(
                ctx,
                out_ptrs.as_mut_ptr(),
                out_samples,
                in_ptrs.as_ptr(),
                num_samples as c_int,
            )
            .unwrap();

            // Flush: call with null input
            let result = convert(ctx, out_ptrs.as_mut_ptr(), out_samples, std::ptr::null(), 0);

            // Flush should succeed (return >= 0)
            assert!(result.is_ok(), "Flush should succeed");

            free(&mut ctx);
        }
    }

    // ========================================================================
    // Integration tests with real audio file
    // ========================================================================

    /// Helper to load audio file path from assets.
    fn get_test_audio_path() -> std::path::PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        std::path::PathBuf::from(format!(
            "{}/../assets/audio/noma-brain-power.mp3",
            manifest_dir
        ))
    }

    /// Helper struct to manage decoded audio context.
    struct AudioDecoder {
        format_ctx: *mut crate::AVFormatContext,
        codec_ctx: *mut crate::AVCodecContext,
        stream_index: i32,
        frame: *mut crate::AVFrame,
        packet: *mut crate::AVPacket,
    }

    impl AudioDecoder {
        /// Open an audio file and prepare for decoding.
        unsafe fn open(path: &std::path::Path) -> Result<Self, i32> {
            use crate::{
                AVMediaType_AVMEDIA_TYPE_AUDIO, av_frame_alloc, av_packet_alloc, avcodec,
                avformat::{close_input, find_stream_info, open_input},
            };

            // Open input file
            let format_ctx = open_input(path)?;
            find_stream_info(format_ctx)?;

            // Find audio stream
            let mut stream_index = -1;
            let nb_streams = (*format_ctx).nb_streams;

            for i in 0..nb_streams {
                let stream = *(*format_ctx).streams.add(i as usize);
                let codecpar = (*stream).codecpar;
                if (*codecpar).codec_type == AVMediaType_AVMEDIA_TYPE_AUDIO {
                    stream_index = i as i32;
                    break;
                }
            }

            if stream_index < 0 {
                close_input(&mut (format_ctx as *mut _));
                return Err(crate::error_codes::EINVAL);
            }

            // Get codec parameters
            let stream = *(*format_ctx).streams.add(stream_index as usize);
            let codecpar = (*stream).codecpar;

            // Find decoder
            let codec =
                avcodec::find_decoder((*codecpar).codec_id).ok_or(crate::error_codes::EINVAL)?;

            // Allocate codec context
            let codec_ctx = avcodec::alloc_context3(codec)?;

            // Copy parameters
            avcodec::parameters_to_context(codec_ctx, codecpar)?;

            // Open codec
            avcodec::open2(codec_ctx, codec, std::ptr::null_mut())?;

            // Allocate frame and packet
            let frame = av_frame_alloc();
            if frame.is_null() {
                avcodec::free_context(&mut (codec_ctx as *mut _));
                close_input(&mut (format_ctx as *mut _));
                return Err(crate::error_codes::ENOMEM);
            }

            let packet = av_packet_alloc();
            if packet.is_null() {
                crate::av_frame_free(&mut (frame as *mut _));
                avcodec::free_context(&mut (codec_ctx as *mut _));
                close_input(&mut (format_ctx as *mut _));
                return Err(crate::error_codes::ENOMEM);
            }

            Ok(Self {
                format_ctx,
                codec_ctx,
                stream_index,
                frame,
                packet,
            })
        }

        /// Decode next audio frame.
        /// Returns None when EOF is reached.
        unsafe fn decode_frame(&mut self) -> Option<&crate::AVFrame> {
            use crate::{avcodec, avformat, error_codes};

            loop {
                // Try to receive a frame from the decoder
                match avcodec::receive_frame(self.codec_ctx, self.frame) {
                    Ok(()) => return Some(&*self.frame),
                    Err(e) if e == error_codes::EAGAIN => {
                        // Need more input
                    }
                    Err(_) => return None,
                }

                // Read next packet
                loop {
                    match avformat::read_frame(self.format_ctx, self.packet) {
                        Ok(()) => {
                            if (*self.packet).stream_index == self.stream_index {
                                break;
                            }
                            // Wrong stream, unref and continue
                            crate::av_packet_unref(self.packet);
                        }
                        Err(_) => {
                            // EOF or error - flush decoder
                            let _ = avcodec::send_packet(self.codec_ctx, std::ptr::null());
                            match avcodec::receive_frame(self.codec_ctx, self.frame) {
                                Ok(()) => return Some(&*self.frame),
                                Err(_) => return None,
                            }
                        }
                    }
                }

                // Send packet to decoder
                let _ = avcodec::send_packet(self.codec_ctx, self.packet);
                crate::av_packet_unref(self.packet);
            }
        }

        /// Get sample format of the decoded audio.
        unsafe fn sample_format(&self) -> crate::AVSampleFormat {
            (*self.codec_ctx).sample_fmt
        }

        /// Get sample rate of the decoded audio.
        unsafe fn sample_rate(&self) -> i32 {
            (*self.codec_ctx).sample_rate
        }

        /// Get channel layout of the decoded audio.
        unsafe fn channel_layout(&self) -> &crate::AVChannelLayout {
            &(*self.codec_ctx).ch_layout
        }
    }

    impl Drop for AudioDecoder {
        fn drop(&mut self) {
            unsafe {
                if !self.packet.is_null() {
                    crate::av_packet_free(&mut self.packet);
                }
                if !self.frame.is_null() {
                    crate::av_frame_free(&mut self.frame);
                }
                if !self.codec_ctx.is_null() {
                    crate::avcodec::free_context(&mut self.codec_ctx);
                }
                if !self.format_ctx.is_null() {
                    crate::avformat::close_input(&mut self.format_ctx);
                }
            }
        }
    }

    #[test]
    fn test_integration_decode_and_resample_mp3() {
        let audio_path = get_test_audio_path();
        if !audio_path.exists() {
            eprintln!("Skipping test: audio file not found at {:?}", audio_path);
            return;
        }

        unsafe {
            // Open and decode audio file
            let mut decoder = AudioDecoder::open(&audio_path).expect("Failed to open audio file");

            let in_sample_rate = decoder.sample_rate();
            let in_sample_fmt = decoder.sample_format();

            // Create resampler: convert to 48kHz stereo float
            let out_layout = channel_layout::stereo();
            let in_layout = decoder.channel_layout();

            let mut swr_ctx = alloc_set_opts2(
                &out_layout,
                sample_format::FLTP,
                48000,
                in_layout,
                in_sample_fmt,
                in_sample_rate,
            )
            .expect("Failed to create resampler context");

            init(swr_ctx).expect("Failed to initialize resampler");

            // Decode and resample a few frames
            let mut total_input_samples = 0;
            let mut total_output_samples = 0;
            let mut frames_processed = 0;

            while let Some(frame) = decoder.decode_frame() {
                let nb_samples = (*frame).nb_samples;
                if nb_samples <= 0 {
                    continue;
                }

                total_input_samples += nb_samples as usize;

                // Prepare output buffers (FLTP = planar)
                let out_count = estimate_output_samples(48000, in_sample_rate, nb_samples);
                let mut out_left: Vec<f32> = vec![0.0; out_count as usize];
                let mut out_right: Vec<f32> = vec![0.0; out_count as usize];
                let mut out_ptrs: [*mut u8; 2] =
                    [out_left.as_mut_ptr().cast(), out_right.as_mut_ptr().cast()];

                // Convert
                let result = convert(
                    swr_ctx,
                    out_ptrs.as_mut_ptr(),
                    out_count,
                    (*frame).extended_data.cast(),
                    nb_samples,
                );

                assert!(
                    result.is_ok(),
                    "Conversion failed at frame {}",
                    frames_processed
                );
                let samples_out = result.unwrap();
                total_output_samples += samples_out as usize;

                frames_processed += 1;

                // Process only first 100 frames for test speed
                if frames_processed >= 100 {
                    break;
                }
            }

            // Flush remaining samples
            let out_count = 4096;
            let mut out_left: Vec<f32> = vec![0.0; out_count as usize];
            let mut out_right: Vec<f32> = vec![0.0; out_count as usize];
            let mut out_ptrs: [*mut u8; 2] =
                [out_left.as_mut_ptr().cast(), out_right.as_mut_ptr().cast()];

            let flush_result = convert(
                swr_ctx,
                out_ptrs.as_mut_ptr(),
                out_count,
                std::ptr::null(),
                0,
            );
            if let Ok(flushed) = flush_result {
                total_output_samples += flushed as usize;
            }

            free(&mut swr_ctx);

            // Verify we processed some audio
            assert!(frames_processed > 0, "Should process at least one frame");
            assert!(total_input_samples > 0, "Should have input samples");
            assert!(total_output_samples > 0, "Should have output samples");

            println!(
                "Integration test: processed {} frames, {} input samples -> {} output samples",
                frames_processed, total_input_samples, total_output_samples
            );
        }
    }

    #[test]
    fn test_integration_format_conversion_chain() {
        let audio_path = get_test_audio_path();
        if !audio_path.exists() {
            eprintln!("Skipping test: audio file not found at {:?}", audio_path);
            return;
        }

        unsafe {
            let mut decoder = AudioDecoder::open(&audio_path).expect("Failed to open audio file");

            let in_sample_rate = decoder.sample_rate();
            let in_sample_fmt = decoder.sample_format();
            let in_layout = decoder.channel_layout();

            // Chain of conversions: input -> S16 -> FLT -> FLTP
            // First: input -> S16 (44.1kHz)
            let mid_layout = channel_layout::stereo();
            let mut swr_to_s16 = alloc_set_opts2(
                &mid_layout,
                sample_format::S16,
                44100,
                in_layout,
                in_sample_fmt,
                in_sample_rate,
            )
            .expect("Failed to create S16 resampler");
            init(swr_to_s16).expect("Failed to init S16 resampler");

            // Second: S16 -> FLTP (48kHz)
            let out_layout = channel_layout::stereo();
            let mut swr_to_fltp = alloc_set_opts2(
                &out_layout,
                sample_format::FLTP,
                48000,
                &mid_layout,
                sample_format::S16,
                44100,
            )
            .expect("Failed to create FLTP resampler");
            init(swr_to_fltp).expect("Failed to init FLTP resampler");

            // Process a few frames through the chain
            let mut frames_processed = 0;

            while let Some(frame) = decoder.decode_frame() {
                let nb_samples = (*frame).nb_samples;
                if nb_samples <= 0 {
                    continue;
                }

                // Stage 1: Convert to S16
                let mid_count = estimate_output_samples(44100, in_sample_rate, nb_samples);
                let mut mid_data: Vec<i16> = vec![0; mid_count as usize * 2];
                let mut mid_ptr = mid_data.as_mut_ptr() as *mut u8;
                let mut mid_ptrs: [*mut u8; 1] = [mid_ptr];

                let mid_result = convert(
                    swr_to_s16,
                    mid_ptrs.as_mut_ptr(),
                    mid_count,
                    (*frame).extended_data.cast(),
                    nb_samples,
                );
                assert!(mid_result.is_ok(), "S16 conversion failed");
                let mid_samples = mid_result.unwrap();

                // Stage 2: Convert S16 to FLTP
                let out_count = estimate_output_samples(48000, 44100, mid_samples);
                let mut out_left: Vec<f32> = vec![0.0; out_count as usize];
                let mut out_right: Vec<f32> = vec![0.0; out_count as usize];
                let mut out_ptrs: [*mut u8; 2] =
                    [out_left.as_mut_ptr().cast(), out_right.as_mut_ptr().cast()];

                let mid_in_ptr = mid_data.as_ptr() as *const u8;
                let mid_in_ptrs: [*const u8; 1] = [mid_in_ptr];

                let out_result = convert(
                    swr_to_fltp,
                    out_ptrs.as_mut_ptr(),
                    out_count,
                    mid_in_ptrs.as_ptr(),
                    mid_samples,
                );
                assert!(out_result.is_ok(), "FLTP conversion failed");

                frames_processed += 1;
                if frames_processed >= 50 {
                    break;
                }
            }

            free(&mut swr_to_s16);
            free(&mut swr_to_fltp);

            assert!(frames_processed > 0, "Should process frames through chain");
            println!(
                "Format chain test: processed {} frames through S16 -> FLTP pipeline",
                frames_processed
            );
        }
    }

    #[test]
    fn test_integration_mono_to_stereo_conversion() {
        let audio_path = get_test_audio_path();
        if !audio_path.exists() {
            eprintln!("Skipping test: audio file not found at {:?}", audio_path);
            return;
        }

        unsafe {
            let mut decoder = AudioDecoder::open(&audio_path).expect("Failed to open audio file");

            let in_sample_rate = decoder.sample_rate();
            let in_sample_fmt = decoder.sample_format();
            let in_layout = decoder.channel_layout();

            // First convert to mono
            let mono_layout = channel_layout::mono();
            let mut swr_to_mono = alloc_set_opts2(
                &mono_layout,
                sample_format::FLTP,
                in_sample_rate,
                in_layout,
                in_sample_fmt,
                in_sample_rate,
            )
            .expect("Failed to create mono resampler");
            init(swr_to_mono).expect("Failed to init mono resampler");

            // Then convert mono back to stereo
            let stereo_layout = channel_layout::stereo();
            let mut swr_to_stereo = alloc_set_opts2(
                &stereo_layout,
                sample_format::FLTP,
                48000,
                &mono_layout,
                sample_format::FLTP,
                in_sample_rate,
            )
            .expect("Failed to create stereo resampler");
            init(swr_to_stereo).expect("Failed to init stereo resampler");

            let mut frames_processed = 0;

            while let Some(frame) = decoder.decode_frame() {
                let nb_samples = (*frame).nb_samples;
                if nb_samples <= 0 {
                    continue;
                }

                // Convert to mono
                let mono_count = nb_samples + 256;
                let mut mono_data: Vec<f32> = vec![0.0; mono_count as usize];
                let mut mono_ptr = mono_data.as_mut_ptr() as *mut u8;
                let mut mono_ptrs: [*mut u8; 1] = [mono_ptr];

                let mono_result = convert(
                    swr_to_mono,
                    mono_ptrs.as_mut_ptr(),
                    mono_count,
                    (*frame).extended_data.cast(),
                    nb_samples,
                );
                assert!(mono_result.is_ok(), "Mono conversion failed");
                let mono_samples = mono_result.unwrap();

                // Convert mono to stereo
                let stereo_count = estimate_output_samples(48000, in_sample_rate, mono_samples);
                let mut stereo_left: Vec<f32> = vec![0.0; stereo_count as usize];
                let mut stereo_right: Vec<f32> = vec![0.0; stereo_count as usize];
                let mut stereo_ptrs: [*mut u8; 2] = [
                    stereo_left.as_mut_ptr().cast(),
                    stereo_right.as_mut_ptr().cast(),
                ];

                let mono_in_ptr = mono_data.as_ptr() as *const u8;
                let mono_in_ptrs: [*const u8; 1] = [mono_in_ptr];

                let stereo_result = convert(
                    swr_to_stereo,
                    stereo_ptrs.as_mut_ptr(),
                    stereo_count,
                    mono_in_ptrs.as_ptr(),
                    mono_samples,
                );
                assert!(stereo_result.is_ok(), "Stereo conversion failed");

                // Verify both channels have similar data (mono duplicated to stereo)
                let stereo_samples = stereo_result.unwrap() as usize;
                if stereo_samples > 10 {
                    // Check first few samples are similar between channels
                    for i in 0..10 {
                        let diff = (stereo_left[i] - stereo_right[i]).abs();
                        assert!(
                            diff < 0.001,
                            "Mono->stereo should have identical channels, diff={}",
                            diff
                        );
                    }
                }

                frames_processed += 1;
                if frames_processed >= 30 {
                    break;
                }
            }

            free(&mut swr_to_mono);
            free(&mut swr_to_stereo);

            assert!(frames_processed > 0, "Should process frames");
            println!(
                "Mono/stereo test: processed {} frames through mono->stereo pipeline",
                frames_processed
            );
        }
    }
}
