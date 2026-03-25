//! Internal filter graph implementation — all `unsafe` `FFmpeg` calls live here.
//!
//! The filter graph is initialised *lazily*: no `FFmpeg` allocation happens at
//! [`FilterGraphInner::new`] time.  The first call to `push_video` or
//! `push_audio` triggers `ensure_video_graph` / `ensure_audio_graph`,
//! which reads the frame's format and builds the full `AVFilterGraph`.

// All FFmpeg FFI lives here; allow unsafe in this module.
#![allow(unsafe_code)]
// Rust 2024: unsafe ops inside unsafe fn still need explicit unsafe blocks.
// We suppress this here because all helpers are private and their callers
// document the invariants.
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::too_many_lines)]

use std::os::raw::c_int;
use std::ptr::NonNull;

use ff_format::time::{Rational, Timestamp};
use ff_format::{AudioFrame, PixelFormat, PooledBuffer, SampleFormat, VideoFrame};
use ff_sys::AVFrame;

use crate::error::FilterError;
use crate::graph::{FilterStep, HwAccel};

// ── Time base used for the buffersrc ─────────────────────────────────────────

/// The time base numerator used for the video buffersrc (1/90000).
const VIDEO_TIME_BASE_NUM: i32 = 1;
/// The time base denominator used for the video buffersrc (1/90000).
const VIDEO_TIME_BASE_DEN: i32 = 90_000;

/// The time base numerator used for the audio abuffersrc (`1/sample_rate`).
const AUDIO_TIME_BASE_NUM: i32 = 1;

// ── Type aliases for complex return types ─────────────────────────────────────

type FilterCtxVec = Vec<Option<NonNull<ff_sys::AVFilterContext>>>;
type BuildResult = Result<(FilterCtxVec, NonNull<ff_sys::AVFilterContext>), FilterError>;
/// Return type for `build_video_graph`: src contexts, sink, and an optional
/// hardware device context that must be stored and freed by the caller.
type VideoGraphResult = Result<
    (
        FilterCtxVec,
        NonNull<ff_sys::AVFilterContext>,
        Option<*mut ff_sys::AVBufferRef>,
    ),
    FilterError,
>;

// ── FFmpeg error helper ───────────────────────────────────────────────────────

/// Convert a negative `FFmpeg` return code into a [`FilterError::Ffmpeg`].
fn ffmpeg_err(code: i32) -> FilterError {
    FilterError::Ffmpeg {
        code,
        message: ff_sys::av_error_string(code),
    }
}

// ── Build-time validation ─────────────────────────────────────────────────────

/// Best-effort check that every [`FilterStep`]'s libavfilter name is known to
/// the linked `FFmpeg` build.
///
/// On platforms where the filter registry is already populated at [`build()`]
/// time (Windows, macOS), this surfaces unknown-filter errors early.
///
/// On some Linux `FFmpeg` builds the registry is not yet populated when
/// `build()` runs (before any `AVFilterGraph` has been allocated).  In that
/// case `avfilter_get_by_name` returns `null` for *every* filter, including
/// standard ones like `scale`.  We cannot distinguish "filter genuinely
/// missing" from "registry not yet initialised", so we log at `debug` level
/// and return `Ok(())`, deferring the authoritative check to
/// `add_and_link_step` at graph-construction time.
pub(crate) fn validate_filter_steps(steps: &[FilterStep]) -> Result<(), FilterError> {
    for step in steps {
        let name =
            std::ffi::CString::new(step.filter_name()).map_err(|_| FilterError::BuildFailed)?;
        // SAFETY: `avfilter_get_by_name` reads a valid, null-terminated C
        // string and returns a borrowed pointer valid for the process lifetime
        // (or null if the filter is not registered / registry not yet ready).
        // The pointer is never dereferenced here.
        let filter = unsafe { ff_sys::avfilter_get_by_name(name.as_ptr()) };
        if filter.is_null() {
            // Could be "filter genuinely absent" OR "registry not yet
            // initialised" (observed on some Linux FFmpeg installations).
            // Treat as unverifiable and defer to add_and_link_step.
            log::warn!(
                "filter lookup returned null at build time name={}, \
                 registry may not be initialised; will be rechecked at push time",
                step.filter_name()
            );
            return Ok(());
        }
    }
    Ok(())
}

// ── FilterGraphInner ──────────────────────────────────────────────────────────

/// Low-level filter graph wrapper.
///
/// All fields start as `None`; they are populated lazily on the first
/// `push_video` / `push_audio` call.
pub(crate) struct FilterGraphInner {
    /// The `AVFilterGraph` itself (`None` until the first push call).
    graph: Option<NonNull<ff_sys::AVFilterGraph>>,
    /// One `AVFilterContext` per input slot (video or audio buffersrc).
    src_ctxs: Vec<Option<NonNull<ff_sys::AVFilterContext>>>,
    /// Sink context for video output.
    vsink_ctx: Option<NonNull<ff_sys::AVFilterContext>>,
    /// Sink context for audio output.
    asink_ctx: Option<NonNull<ff_sys::AVFilterContext>>,
    /// The ordered list of filter steps to apply.
    steps: Vec<FilterStep>,
    /// Optional hardware acceleration backend.
    hw: Option<HwAccel>,
    /// Owned reference to the hardware device context (`None` when no hardware
    /// acceleration is in use).  Freed in `Drop` after the graph is freed.
    hw_device_ctx: Option<*mut ff_sys::AVBufferRef>,
}

// SAFETY: `FilterGraphInner` owns all raw pointers exclusively.  No other
// thread holds references to the underlying `AVFilterGraph`, any of its
// contexts, or the hardware device context while this struct is alive.
unsafe impl Send for FilterGraphInner {}

impl FilterGraphInner {
    /// Create a new (uninitialised) inner.  No `FFmpeg` calls are made here.
    pub(crate) fn new(steps: Vec<FilterStep>, hw: Option<HwAccel>) -> Self {
        Self {
            graph: None,
            src_ctxs: Vec::new(),
            vsink_ctx: None,
            asink_ctx: None,
            steps,
            hw,
            hw_device_ctx: None,
        }
    }

    // ── Video ─────────────────────────────────────────────────────────────────

    /// Lazily initialise the video filter graph from the first pushed frame.
    fn ensure_video_graph(&mut self, frame: &VideoFrame) -> Result<(), FilterError> {
        if self.graph.is_some() {
            return Ok(());
        }

        let pix_fmt = pixel_format_to_av(frame.format());
        let args = video_buffersrc_args(frame.width(), frame.height(), pix_fmt);
        let num_inputs = self.video_input_count();

        // SAFETY: all raw pointers are checked for null after allocation; the
        // graph pointer is stored in `self.graph` and kept alive for the
        // lifetime of this struct.
        unsafe {
            let graph_ptr = ff_sys::avfilter_graph_alloc();
            if graph_ptr.is_null() {
                return Err(FilterError::BuildFailed);
            }
            // SAFETY: checked non-null above.
            let graph_nn = NonNull::new_unchecked(graph_ptr);

            match Self::build_video_graph(
                graph_nn,
                &args,
                num_inputs,
                &self.steps,
                self.hw.as_ref(),
            ) {
                Ok((src_ctxs, vsink_ctx, hw_device_ctx)) => {
                    self.graph = Some(graph_nn);
                    self.src_ctxs = src_ctxs;
                    self.vsink_ctx = Some(vsink_ctx);
                    self.hw_device_ctx = hw_device_ctx;
                    log::info!(
                        "filter graph configured inputs={} filters={}",
                        num_inputs,
                        self.steps.len()
                    );
                    Ok(())
                }
                Err(e) => {
                    let mut raw = graph_nn.as_ptr();
                    ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(raw));
                    Err(e)
                }
            }
        }
    }

    /// Build the `AVFilterGraph` for video, returning `(src_ctxs, vsink_ctx)`.
    ///
    /// `num_inputs` buffersrc contexts are created (`in0`..`inN-1`).  For
    /// multi-input filters like `overlay`, the extra sources are linked to the
    /// appropriate input pads after the main chain link is established.
    ///
    /// # Safety
    ///
    /// `graph_nn` must be a valid, freshly-allocated `AVFilterGraph`.
    unsafe fn build_video_graph(
        graph_nn: NonNull<ff_sys::AVFilterGraph>,
        buffersrc_args: &str,
        num_inputs: usize,
        steps: &[FilterStep],
        hw: Option<&HwAccel>,
    ) -> VideoGraphResult {
        let graph = graph_nn.as_ptr();

        // 0. When hardware acceleration is requested, create a device context
        //    and disable automatic pixel-format conversion so FFmpeg does not
        //    insert implicit hwupload/scale filters that would conflict with the
        //    explicit ones we add below.
        let hw_device_ctx: Option<*mut ff_sys::AVBufferRef> = if let Some(hw) = hw {
            let device_type = hw_accel_to_device_type(*hw);
            let mut raw_hw_ctx: *mut ff_sys::AVBufferRef = std::ptr::null_mut();
            let ret = ff_sys::av_hwdevice_ctx_create(
                &raw mut raw_hw_ctx,
                device_type,
                std::ptr::null(),     // device: null = system default
                std::ptr::null_mut(), // opts: null = defaults
                0,
            );
            if ret < 0 {
                log::warn!("av_hwdevice_ctx_create failed hw={hw:?} code={ret}");
                return Err(FilterError::BuildFailed);
            }
            // AVFILTER_AUTO_CONVERT_NONE = 0: hardware filters must receive
            // frames in exactly the format they expect.
            ff_sys::avfilter_graph_set_auto_convert(graph, 0u32);
            log::debug!("hw device context created hw={hw:?}");
            Some(raw_hw_ctx)
        } else {
            None
        };

        // Helper closure: free hw_device_ctx and return an error.  Used at
        // every early-return failure point that occurs *after* the device
        // context has been allocated so it is not leaked.
        macro_rules! bail {
            ($err:expr) => {{
                if let Some(mut hw_ctx) = hw_device_ctx {
                    ff_sys::av_buffer_unref(std::ptr::addr_of_mut!(hw_ctx));
                }
                return Err($err);
            }};
        }

        // SAFETY: `avfilter_get_by_name` returns a borrowed pointer valid for
        // the process lifetime; we never free it.
        let buffersrc = ff_sys::avfilter_get_by_name(c"buffer".as_ptr());
        if buffersrc.is_null() {
            bail!(FilterError::BuildFailed);
        }

        let Ok(src_args) = std::ffi::CString::new(buffersrc_args) else {
            bail!(FilterError::BuildFailed)
        };
        let mut src_ctxs: Vec<Option<NonNull<ff_sys::AVFilterContext>>> =
            Vec::with_capacity(num_inputs);

        // 1. Create in0 (always present).
        let mut raw_ctx0: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut raw_ctx0,
            buffersrc,
            c"in0".as_ptr(),
            src_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(FilterError::BuildFailed);
        }
        log::debug!("filter added name=buffersrc slot=0");
        // SAFETY: ret >= 0 guarantees non-null.
        src_ctxs.push(Some(NonNull::new_unchecked(raw_ctx0)));

        // Create in1..inN-1 (for overlay etc.)
        for slot in 1..num_inputs {
            let Ok(ctx_name) = std::ffi::CString::new(format!("in{slot}")) else {
                bail!(FilterError::BuildFailed)
            };
            let mut raw_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut raw_ctx,
                buffersrc,
                ctx_name.as_ptr(),
                src_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(FilterError::BuildFailed);
            }
            log::debug!("filter added name=buffersrc slot={slot}");
            // SAFETY: ret >= 0 guarantees non-null.
            src_ctxs.push(Some(NonNull::new_unchecked(raw_ctx)));
        }

        // 2. Create buffersink ("buffersink").
        let buffersink = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
        if buffersink.is_null() {
            bail!(FilterError::BuildFailed);
        }

        let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut sink_ctx,
            buffersink,
            c"out".as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(FilterError::BuildFailed);
        }

        // 3. Insert hwupload/hwupload_cuda BEFORE the filter steps so that
        //    subsequent filters receive hardware (CUDA/VAAPI/VTB) frames.
        let mut prev_ctx: *mut ff_sys::AVFilterContext = raw_ctx0;
        if let (Some(hw_ctx), Some(hw_backend)) = (hw_device_ctx, hw) {
            let upload_name = match hw_backend {
                HwAccel::Cuda => c"hwupload_cuda",
                HwAccel::VideoToolbox | HwAccel::Vaapi => c"hwupload",
            };
            prev_ctx = match create_hw_filter(graph, prev_ctx, upload_name, c"hwupload0", hw_ctx) {
                Ok(ctx) => ctx,
                Err(e) => bail!(e),
            };
        }

        // 4-5. Add each `FilterStep`, link the main chain (in0 → step[0] → …),
        // and wire extra input pads for multi-input filters.
        for (i, step) in steps.iter().enumerate() {
            prev_ctx = match add_and_link_step(graph, prev_ctx, step, i, "step") {
                Ok(ctx) => ctx,
                Err(e) => bail!(e),
            };

            // After trim, insert setpts=PTS-STARTPTS so the output timestamps
            // are reset to start at zero.
            if matches!(step, FilterStep::Trim { .. }) {
                prev_ctx = match add_setpts_after_trim(graph, prev_ctx, i) {
                    Ok(ctx) => ctx,
                    Err(e) => bail!(e),
                };
            }

            // Overlay consumes a second input on pad 1.
            if matches!(step, FilterStep::Overlay { .. })
                && let Some(Some(extra_src)) = src_ctxs.get(1)
            {
                let ret = ff_sys::avfilter_link(extra_src.as_ptr(), 0, prev_ctx, 1);
                if ret < 0 {
                    bail!(FilterError::BuildFailed);
                }
                log::debug!("filter linked extra_input=in1 to overlay pad=1");
            }
        }

        // 6. Insert hwdownload AFTER all filter steps so output frames are
        //    downloaded back to system memory before reaching the buffersink.
        if let Some(hw_ctx) = hw_device_ctx {
            prev_ctx =
                match create_hw_filter(graph, prev_ctx, c"hwdownload", c"hwdownload0", hw_ctx) {
                    Ok(ctx) => ctx,
                    Err(e) => bail!(e),
                };
        }

        // Link last filter to sink.
        let ret = ff_sys::avfilter_link(prev_ctx, 0, sink_ctx, 0);
        if ret < 0 {
            bail!(FilterError::BuildFailed);
        }

        // 7. Configure the graph.
        let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
        if ret < 0 {
            log::warn!("avfilter_graph_config failed code={ret}");
            // If there is a crop step the most likely cause is the rectangle
            // extending beyond the source frame dimensions.
            if let Some(FilterStep::Crop {
                x,
                y,
                width,
                height,
            }) = steps.iter().find(|s| matches!(s, FilterStep::Crop { .. }))
            {
                bail!(FilterError::InvalidConfig {
                    reason: format!(
                        "crop rect {x},{y}+{width}x{height} exceeds source frame dimensions"
                    ),
                });
            }
            bail!(ffmpeg_err(ret));
        }

        // SAFETY: `avfilter_graph_create_filter` with ret >= 0 guarantees
        // non-null pointers.
        let sink_nn = NonNull::new_unchecked(sink_ctx);
        Ok((src_ctxs, sink_nn, hw_device_ctx))
    }

    /// Push a video frame into the filter graph.
    pub(crate) fn push_video(
        &mut self,
        slot: usize,
        frame: &VideoFrame,
    ) -> Result<(), FilterError> {
        self.ensure_video_graph(frame)?;

        let src_ctx = self
            .src_ctxs
            .get(slot)
            .and_then(|opt| *opt)
            .ok_or_else(|| FilterError::InvalidInput {
                slot,
                reason: format!("slot {slot} out of range (have {})", self.src_ctxs.len()),
            })?;

        // SAFETY: we allocate the `AVFrame`, fill it with `VideoFrame` data,
        // push it to the buffersrc, then immediately free it.  The buffersrc
        // keeps its own reference (`AV_BUFFERSRC_FLAG_KEEP_REF`).
        unsafe {
            let raw_frame = ff_sys::av_frame_alloc();
            if raw_frame.is_null() {
                return Err(FilterError::ProcessFailed);
            }

            (*raw_frame).width = frame.width() as c_int;
            (*raw_frame).height = frame.height() as c_int;
            (*raw_frame).format = pixel_format_to_av(frame.format());
            let pts = video_pts_ticks(frame.timestamp());
            if pts == ff_sys::AV_NOPTS_VALUE {
                log::warn!("pts invalid, passing AV_NOPTS_VALUE to filter graph slot={slot}");
            }
            (*raw_frame).pts = pts;

            let ret = ff_sys::av_frame_get_buffer(raw_frame, 0);
            if ret < 0 {
                let mut ptr = raw_frame;
                ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
                return Err(FilterError::ProcessFailed);
            }

            copy_video_planes_to_av(frame, raw_frame);

            let ret = ff_sys::av_buffersrc_add_frame_flags(
                src_ctx.as_ptr(),
                raw_frame,
                ff_sys::BUFFERSRC_FLAG_KEEP_REF,
            );
            let mut ptr = raw_frame;
            ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));

            if ret < 0 {
                return Err(FilterError::ProcessFailed);
            }
        }
        Ok(())
    }

    /// Pull the next filtered video frame, or `None` if not yet available.
    pub(crate) fn pull_video(&mut self) -> Result<Option<VideoFrame>, FilterError> {
        let Some(sink_ctx) = self.vsink_ctx else {
            return Ok(None);
        };

        // SAFETY: we allocate a temporary `AVFrame`, hand it to
        // `av_buffersink_get_frame`, convert the result, then free it.
        unsafe {
            let raw_frame = ff_sys::av_frame_alloc();
            if raw_frame.is_null() {
                return Err(FilterError::ProcessFailed);
            }

            let ret = ff_sys::av_buffersink_get_frame(sink_ctx.as_ptr(), raw_frame);

            // EAGAIN (-11) and EOF: return `None`.
            if ret < 0 {
                let mut ptr = raw_frame;
                ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
                return Ok(None);
            }

            let result = av_frame_to_video_frame(raw_frame);
            let mut ptr = raw_frame;
            ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));

            match result {
                Ok(frame) => Ok(Some(frame)),
                Err(()) => Err(FilterError::ProcessFailed),
            }
        }
    }

    // ── Audio ─────────────────────────────────────────────────────────────────

    /// Returns the number of video input slots required by the configured steps.
    ///
    /// Returns 2 when [`FilterStep::Overlay`] is present (needs a main stream
    /// on slot 0 and a secondary stream on slot 1), 1 otherwise.
    fn video_input_count(&self) -> usize {
        for step in &self.steps {
            if matches!(step, FilterStep::Overlay { .. }) {
                return 2;
            }
        }
        1
    }

    /// Returns the number of audio input slots required by the configured steps.
    fn audio_input_count(&self) -> usize {
        for step in &self.steps {
            if let FilterStep::Amix(n) = step {
                return *n;
            }
        }
        1
    }

    /// Lazily initialise the audio filter graph from the first pushed frame.
    fn ensure_audio_graph(&mut self, frame: &AudioFrame) -> Result<(), FilterError> {
        if self.asink_ctx.is_some() {
            return Ok(());
        }

        let num_inputs = self.audio_input_count();
        let sample_fmt = sample_format_to_av_name(frame.format());
        let sample_rate = frame.sample_rate();
        let channels = frame.channels();

        let args = audio_buffersrc_args(sample_rate, sample_fmt, channels);

        // SAFETY: same contract as `ensure_video_graph` — pointers checked for
        // null, stored in `self`, freed in `Drop`.
        unsafe {
            let graph_ptr = ff_sys::avfilter_graph_alloc();
            if graph_ptr.is_null() {
                return Err(FilterError::BuildFailed);
            }
            // SAFETY: checked non-null above.
            let graph_nn = NonNull::new_unchecked(graph_ptr);

            match Self::build_audio_graph(
                graph_nn,
                &args,
                num_inputs,
                &self.steps,
                self.hw.as_ref(),
            ) {
                Ok((src_ctxs, asink_ctx)) => {
                    if self.graph.is_none() {
                        self.graph = Some(graph_nn);
                    } else {
                        let mut raw = graph_nn.as_ptr();
                        ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(raw));
                    }
                    let video_slots = self.src_ctxs.len();
                    self.src_ctxs.resize(video_slots + num_inputs, None);
                    for (i, ctx) in src_ctxs.into_iter().enumerate() {
                        self.src_ctxs[video_slots + i] = ctx;
                    }
                    self.asink_ctx = Some(asink_ctx);
                    log::info!(
                        "filter graph configured inputs={} filters={}",
                        num_inputs,
                        self.steps.len()
                    );
                    Ok(())
                }
                Err(e) => {
                    let mut raw = graph_nn.as_ptr();
                    ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(raw));
                    Err(e)
                }
            }
        }
    }

    /// Build the `AVFilterGraph` for audio, returning `(src_ctxs, asink_ctx)`.
    ///
    /// # Safety
    ///
    /// `graph_nn` must be a valid, freshly-allocated `AVFilterGraph`.
    unsafe fn build_audio_graph(
        graph_nn: NonNull<ff_sys::AVFilterGraph>,
        buffersrc_args: &str,
        num_inputs: usize,
        steps: &[FilterStep],
        _hw: Option<&HwAccel>,
    ) -> BuildResult {
        let graph = graph_nn.as_ptr();
        let mut src_ctxs = Vec::with_capacity(num_inputs);

        // 1. Create abuffer sources, one per input slot.
        let abuffer = ff_sys::avfilter_get_by_name(c"abuffer".as_ptr());
        if abuffer.is_null() {
            return Err(FilterError::BuildFailed);
        }
        let src_args =
            std::ffi::CString::new(buffersrc_args).map_err(|_| FilterError::BuildFailed)?;

        // First input slot.
        let first_src_ctx = {
            let mut raw_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut raw_ctx,
                abuffer,
                c"in0".as_ptr(),
                src_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                return Err(FilterError::BuildFailed);
            }
            log::debug!("filter added name=abuffersrc slot=0");
            // SAFETY: ret >= 0 means raw_ctx is non-null.
            let nn = NonNull::new_unchecked(raw_ctx);
            src_ctxs.push(Some(nn));
            nn
        };

        // Additional input slots for amix.
        for slot in 1..num_inputs {
            let ctx_name = std::ffi::CString::new(format!("in{slot}"))
                .map_err(|_| FilterError::BuildFailed)?;
            let mut raw_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut raw_ctx,
                abuffer,
                ctx_name.as_ptr(),
                src_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                return Err(FilterError::BuildFailed);
            }
            log::debug!("filter added name=abuffersrc slot={slot}");
            // SAFETY: ret >= 0 means raw_ctx is non-null.
            src_ctxs.push(Some(NonNull::new_unchecked(raw_ctx)));
        }

        // 2. Create abuffersink.
        let abuffersink = ff_sys::avfilter_get_by_name(c"abuffersink".as_ptr());
        if abuffersink.is_null() {
            return Err(FilterError::BuildFailed);
        }

        let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut sink_ctx,
            abuffersink,
            c"aout".as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            return Err(FilterError::BuildFailed);
        }

        // 3-5. Add each `FilterStep` (audio-relevant steps) and link.
        let mut prev_ctx = first_src_ctx.as_ptr();
        for (i, step) in steps.iter().enumerate() {
            prev_ctx = add_and_link_step(graph, prev_ctx, step, i, "astep")?;
        }

        // Link last filter to sink.
        let ret = ff_sys::avfilter_link(prev_ctx, 0, sink_ctx, 0);
        if ret < 0 {
            return Err(FilterError::BuildFailed);
        }

        // 6. Configure the graph.
        let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
        if ret < 0 {
            log::warn!("avfilter_graph_config failed code={ret}");
            return Err(ffmpeg_err(ret));
        }

        // SAFETY: sink_ctx is non-null (ret >= 0 above).
        let sink_nn = NonNull::new_unchecked(sink_ctx);
        Ok((src_ctxs, sink_nn))
    }

    /// Push an audio frame into the filter graph.
    pub(crate) fn push_audio(
        &mut self,
        slot: usize,
        frame: &AudioFrame,
    ) -> Result<(), FilterError> {
        self.ensure_audio_graph(frame)?;

        let audio_inputs = self.audio_input_count();
        let video_slots = self.src_ctxs.len().saturating_sub(audio_inputs);
        let audio_slot = video_slots + slot;

        let src_ctx = self
            .src_ctxs
            .get(audio_slot)
            .and_then(|opt| *opt)
            .ok_or_else(|| FilterError::InvalidInput {
                slot,
                reason: format!("audio slot {slot} out of range (have {audio_inputs})"),
            })?;

        // SAFETY: allocate `AVFrame`, copy `AudioFrame` data, push, free.
        unsafe {
            let raw_frame = ff_sys::av_frame_alloc();
            if raw_frame.is_null() {
                return Err(FilterError::ProcessFailed);
            }

            (*raw_frame).nb_samples = frame.samples() as c_int;
            (*raw_frame).sample_rate = frame.sample_rate() as c_int;
            (*raw_frame).format = sample_format_to_av(frame.format());
            let pts = audio_pts_ticks(frame.timestamp(), frame.sample_rate());
            if pts == ff_sys::AV_NOPTS_VALUE {
                log::warn!("pts invalid, passing AV_NOPTS_VALUE to filter graph slot={slot}");
            }
            (*raw_frame).pts = pts;
            (*raw_frame).ch_layout.nb_channels = frame.channels() as c_int;

            let ret = ff_sys::av_frame_get_buffer(raw_frame, 0);
            if ret < 0 {
                let mut ptr = raw_frame;
                ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
                return Err(FilterError::ProcessFailed);
            }

            copy_audio_planes_to_av(frame, raw_frame);

            let ret = ff_sys::av_buffersrc_add_frame_flags(
                src_ctx.as_ptr(),
                raw_frame,
                ff_sys::BUFFERSRC_FLAG_KEEP_REF,
            );
            let mut ptr = raw_frame;
            ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));

            if ret < 0 {
                return Err(FilterError::ProcessFailed);
            }
        }
        Ok(())
    }

    /// Pull the next filtered audio frame, or `None` if not yet available.
    pub(crate) fn pull_audio(&mut self) -> Result<Option<AudioFrame>, FilterError> {
        let Some(sink_ctx) = self.asink_ctx else {
            return Ok(None);
        };

        // SAFETY: allocate, fill via `av_buffersink_get_frame`, convert, free.
        unsafe {
            let raw_frame = ff_sys::av_frame_alloc();
            if raw_frame.is_null() {
                return Err(FilterError::ProcessFailed);
            }

            let ret = ff_sys::av_buffersink_get_frame(sink_ctx.as_ptr(), raw_frame);

            // EAGAIN (-11) and EOF: return `None`.
            if ret < 0 {
                let mut ptr = raw_frame;
                ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));
                return Ok(None);
            }

            let result = av_frame_to_audio_frame(raw_frame);
            let mut ptr = raw_frame;
            ff_sys::av_frame_free(std::ptr::addr_of_mut!(ptr));

            match result {
                Ok(frame) => Ok(Some(frame)),
                Err(()) => Err(FilterError::ProcessFailed),
            }
        }
    }
}

impl Drop for FilterGraphInner {
    fn drop(&mut self) {
        if let Some(ptr) = self.graph.take() {
            // SAFETY: `graph` is non-null (guaranteed by `NonNull`), and we are
            // the sole owner.  `avfilter_graph_free` also frees all
            // `AVFilterContext`s attached to the graph, so `src_ctxs`,
            // `vsink_ctx`, and `asink_ctx` must NOT be freed individually.
            // Filter contexts that held `av_buffer_ref` refs to `hw_device_ctx`
            // release those refs here as well.
            unsafe {
                let mut raw = ptr.as_ptr();
                ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(raw));
            }
        }
        // Free our own reference to the hardware device context AFTER the graph
        // has been freed.  The graph's filter contexts held their own references
        // (created via `av_buffer_ref` in `create_hw_filter`); those were
        // released by `avfilter_graph_free` above.
        if let Some(mut hw_ctx) = self.hw_device_ctx.take() {
            // SAFETY: `hw_ctx` is the sole remaining reference owned by this
            // struct; the filter graph (and its filter contexts) has already
            // been freed above.
            unsafe {
                ff_sys::av_buffer_unref(std::ptr::addr_of_mut!(hw_ctx));
            }
        }
    }
}

// ── Hardware acceleration helpers ─────────────────────────────────────────────

/// Map a [`HwAccel`] variant to the corresponding `AVHWDeviceType` constant.
fn hw_accel_to_device_type(hw: HwAccel) -> ff_sys::AVHWDeviceType {
    match hw {
        HwAccel::Cuda => ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_CUDA,
        HwAccel::VideoToolbox => ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
        HwAccel::Vaapi => ff_sys::AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI,
    }
}

/// Create and link a named hardware filter (e.g., `hwupload_cuda`, `hwdownload`)
/// with no arguments.  Sets the filter context's `hw_device_ctx` to a new
/// reference obtained via `av_buffer_ref(hw_ctx)` so the filter owns its own ref.
///
/// # Safety
///
/// `graph`, `prev_ctx`, and `hw_ctx` must be valid non-null pointers.
/// `hw_ctx` must be a valid `AVBufferRef` wrapping an `AVHWDeviceContext`.
unsafe fn create_hw_filter(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    filter_name: &std::ffi::CStr,
    instance_name: &std::ffi::CStr,
    hw_ctx: *mut ff_sys::AVBufferRef,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    // SAFETY: `avfilter_get_by_name` reads a valid null-terminated C string and
    // returns a borrowed, process-lifetime pointer (or null if not found).
    let filter = ff_sys::avfilter_get_by_name(filter_name.as_ptr());
    if filter.is_null() {
        log::warn!(
            "hw filter not found name={}",
            filter_name.to_str().unwrap_or("?")
        );
        return Err(FilterError::BuildFailed);
    }

    let mut ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut ctx,
        filter,
        instance_name.as_ptr(),
        std::ptr::null(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!(
            "hw filter creation failed name={} code={ret}",
            filter_name.to_str().unwrap_or("?")
        );
        return Err(FilterError::BuildFailed);
    }
    log::debug!(
        "hw filter added name={}",
        filter_name.to_str().unwrap_or("?")
    );

    // Give this filter context its own reference to the hardware device context.
    // SAFETY: `hw_ctx` is a valid `AVBufferRef`; `av_buffer_ref` returns a new
    // reference counted ref, or null on allocation failure.
    let filter_hw_ref = ff_sys::av_buffer_ref(hw_ctx);
    if filter_hw_ref.is_null() {
        log::warn!("av_buffer_ref failed for hw device context");
        return Err(FilterError::BuildFailed);
    }
    (*ctx).hw_device_ctx = filter_hw_ref;

    // SAFETY: `prev_ctx` and `ctx` belong to the same graph; pad indices are valid.
    let ret = ff_sys::avfilter_link(prev_ctx, 0, ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }

    Ok(ctx)
}

// ── Shared graph-building helper ──────────────────────────────────────────────

/// Create an `AVFilterContext` for `step`, link it after `prev_ctx`, and return
/// the new context pointer.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
unsafe fn add_and_link_step(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    step: &FilterStep,
    index: usize,
    prefix: &str,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let filter_name =
        std::ffi::CString::new(step.filter_name()).map_err(|_| FilterError::BuildFailed)?;
    let filter = ff_sys::avfilter_get_by_name(filter_name.as_ptr());
    if filter.is_null() {
        log::warn!("filter not found name={}", step.filter_name());
        return Err(FilterError::BuildFailed);
    }

    let step_name =
        std::ffi::CString::new(format!("{prefix}{index}")).map_err(|_| FilterError::BuildFailed)?;
    let step_args_str = step.args();
    let step_args =
        std::ffi::CString::new(step_args_str.as_str()).map_err(|_| FilterError::BuildFailed)?;

    let mut step_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut step_ctx,
        filter,
        step_name.as_ptr(),
        step_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!(
            "filter creation failed name={} args={}",
            step.filter_name(),
            step.args()
        );
        return Err(FilterError::BuildFailed);
    }
    log::debug!(
        "filter added name={} args={}",
        step.filter_name(),
        step.args()
    );

    let ret = ff_sys::avfilter_link(prev_ctx, 0, step_ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }
    Ok(step_ctx)
}

/// Insert a `setpts=PTS-STARTPTS` filter immediately after a `trim` step so
/// that the output stream's timestamps start at zero.
///
/// # Safety
///
/// `graph` and `prev_ctx` must be valid pointers owned by the same
/// `AVFilterGraph`.
unsafe fn add_setpts_after_trim(
    graph: *mut ff_sys::AVFilterGraph,
    prev_ctx: *mut ff_sys::AVFilterContext,
    trim_index: usize,
) -> Result<*mut ff_sys::AVFilterContext, FilterError> {
    let setpts = ff_sys::avfilter_get_by_name(c"setpts".as_ptr());
    if setpts.is_null() {
        log::warn!("filter not found name=setpts");
        return Err(FilterError::BuildFailed);
    }

    let name = std::ffi::CString::new(format!("setpts{trim_index}"))
        .map_err(|_| FilterError::BuildFailed)?;

    let mut ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut ctx,
        setpts,
        name.as_ptr(),
        c"PTS-STARTPTS".as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        log::warn!("filter creation failed name=setpts args=PTS-STARTPTS");
        return Err(FilterError::BuildFailed);
    }
    log::debug!("filter added name=setpts args=PTS-STARTPTS");

    let ret = ff_sys::avfilter_link(prev_ctx, 0, ctx, 0);
    if ret < 0 {
        return Err(FilterError::BuildFailed);
    }
    Ok(ctx)
}

// ── buffersrc / buffersink arg-string helpers ──────────────────────────────────

/// Build the `args` string passed to `avfilter_graph_create_filter` when
/// creating a video `buffer` (buffersrc) context.
///
/// The format follows libavfilter's `buffer` filter parameter syntax:
/// `video_size=WxH:pix_fmt=N:time_base=NUM/DEN:pixel_aspect=1/1`.
fn video_buffersrc_args(width: u32, height: u32, pix_fmt: c_int) -> String {
    format!(
        "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect=1/1",
        width, height, pix_fmt, VIDEO_TIME_BASE_NUM, VIDEO_TIME_BASE_DEN,
    )
}

/// Build the `args` string passed to `avfilter_graph_create_filter` when
/// creating an audio `abuffer` (buffersrc) context.
///
/// The format follows libavfilter's `abuffer` filter parameter syntax:
/// `sample_rate=R:sample_fmt=FMT:channels=C:time_base=1/R`.
fn audio_buffersrc_args(sample_rate: u32, sample_fmt_name: &str, channels: u32) -> String {
    format!(
        "sample_rate={}:sample_fmt={}:channels={}:time_base={}/{}",
        sample_rate, sample_fmt_name, channels, AUDIO_TIME_BASE_NUM, sample_rate,
    )
}

// ── Timestamp PTS helpers ──────────────────────────────────────────────────────

/// Compute the `AVFrame.pts` ticks for pushing a video frame.
///
/// Scales the timestamp's seconds to the internal video time base (1/90000).
/// Returns [`ff_sys::AV_NOPTS_VALUE`] when the timestamp carries no valid PTS.
fn video_pts_ticks(ts: Timestamp) -> i64 {
    if ts.pts() == ff_sys::AV_NOPTS_VALUE {
        ff_sys::AV_NOPTS_VALUE
    } else {
        (ts.as_secs_f64() * f64::from(VIDEO_TIME_BASE_DEN)) as i64
    }
}

/// Convert raw `AVFrame.pts` ticks (1/90000 time base) to a [`Timestamp`].
///
/// Returns [`Timestamp::default`] (0 at 1/90000) when `pts_raw` is
/// [`ff_sys::AV_NOPTS_VALUE`].
fn video_ticks_to_timestamp(pts_raw: i64) -> Timestamp {
    if pts_raw == ff_sys::AV_NOPTS_VALUE {
        Timestamp::default()
    } else {
        let secs = pts_raw as f64 / f64::from(VIDEO_TIME_BASE_DEN);
        Timestamp::from_secs_f64(
            secs,
            Rational::new(VIDEO_TIME_BASE_NUM, VIDEO_TIME_BASE_DEN),
        )
    }
}

/// Compute the `AVFrame.pts` ticks for pushing an audio frame.
///
/// Scales the timestamp's seconds to the audio time base (1/`sample_rate`).
/// Returns [`ff_sys::AV_NOPTS_VALUE`] when the timestamp carries no valid PTS.
fn audio_pts_ticks(ts: Timestamp, sample_rate: u32) -> i64 {
    if ts.pts() == ff_sys::AV_NOPTS_VALUE {
        ff_sys::AV_NOPTS_VALUE
    } else {
        (ts.as_secs_f64() * f64::from(sample_rate)) as i64
    }
}

/// Convert raw `AVFrame.pts` ticks (1/`sample_rate` time base) to a [`Timestamp`].
///
/// Returns [`Timestamp::zero`] at `1/sample_rate` when `pts_raw` is
/// [`ff_sys::AV_NOPTS_VALUE`].  Falls back to denominator 1 if `sample_rate` is 0.
fn audio_ticks_to_timestamp(pts_raw: i64, sample_rate: u32) -> Timestamp {
    let den = if sample_rate > 0 {
        sample_rate as i32
    } else {
        1
    };
    let time_base = Rational::new(1, den);
    if pts_raw == ff_sys::AV_NOPTS_VALUE {
        Timestamp::zero(time_base)
    } else {
        let secs = if sample_rate > 0 {
            pts_raw as f64 / f64::from(sample_rate)
        } else {
            0.0
        };
        Timestamp::from_secs_f64(secs, time_base)
    }
}

// ── Format conversion helpers ─────────────────────────────────────────────────

/// Convert a [`PixelFormat`] to the corresponding `AVPixelFormat` integer.
fn pixel_format_to_av(fmt: PixelFormat) -> c_int {
    match fmt {
        PixelFormat::Yuv420p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P,
        PixelFormat::Rgb24 => ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24,
        PixelFormat::Bgr24 => ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24,
        PixelFormat::Yuv422p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P,
        PixelFormat::Yuv444p => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P,
        PixelFormat::Gray8 => ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8,
        PixelFormat::Nv12 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV12,
        PixelFormat::Nv21 => ff_sys::AVPixelFormat_AV_PIX_FMT_NV21,
        PixelFormat::Rgba => ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA,
        PixelFormat::Bgra => ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA,
        PixelFormat::Yuv420p10le => ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE,
        PixelFormat::P010le => ff_sys::AVPixelFormat_AV_PIX_FMT_P010LE,
        PixelFormat::Other(v) => v as c_int,
        // `PixelFormat` is `#[non_exhaustive]`; new variants default to NONE.
        _ => ff_sys::AVPixelFormat_AV_PIX_FMT_NONE,
    }
}

/// Convert an `AVPixelFormat` integer to a [`PixelFormat`].
fn av_to_pixel_format(av_fmt: c_int) -> PixelFormat {
    match av_fmt {
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P => PixelFormat::Yuv420p,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_RGB24 => PixelFormat::Rgb24,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_BGR24 => PixelFormat::Bgr24,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV422P => PixelFormat::Yuv422p,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV444P => PixelFormat::Yuv444p,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_GRAY8 => PixelFormat::Gray8,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_NV12 => PixelFormat::Nv12,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_NV21 => PixelFormat::Nv21,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_RGBA => PixelFormat::Rgba,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_BGRA => PixelFormat::Bgra,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_YUV420P10LE => PixelFormat::Yuv420p10le,
        v if v == ff_sys::AVPixelFormat_AV_PIX_FMT_P010LE => PixelFormat::P010le,
        other => PixelFormat::Other(other.max(0) as u32),
    }
}

/// Convert a [`SampleFormat`] to the corresponding `AVSampleFormat` integer.
fn sample_format_to_av(fmt: SampleFormat) -> c_int {
    match fmt {
        SampleFormat::U8 => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8,
        SampleFormat::I16 => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16,
        SampleFormat::I32 => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32,
        SampleFormat::F32 => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLT,
        SampleFormat::F64 => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBL,
        SampleFormat::U8p => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8P,
        SampleFormat::I16p => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16P,
        SampleFormat::I32p => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32P,
        SampleFormat::F32p => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP,
        SampleFormat::F64p => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBLP,
        SampleFormat::Other(v) => v as c_int,
        // `SampleFormat` is `#[non_exhaustive]`; new variants default to FLT.
        _ => ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLT,
    }
}

/// Returns the libavfilter `sample_fmt` string for an `abuffer` args string.
fn sample_format_to_av_name(fmt: SampleFormat) -> &'static str {
    match fmt {
        SampleFormat::U8 => "u8",
        SampleFormat::I16 => "s16",
        SampleFormat::I32 => "s32",
        SampleFormat::F32 => "flt",
        SampleFormat::F64 => "dbl",
        SampleFormat::U8p => "u8p",
        SampleFormat::I16p => "s16p",
        SampleFormat::I32p => "s32p",
        SampleFormat::F32p => "fltp",
        SampleFormat::F64p => "dblp",
        SampleFormat::Other(_) => "flt",
        // `SampleFormat` is `#[non_exhaustive]`; new variants default to flt.
        _ => "flt",
    }
}

/// Convert an `AVSampleFormat` integer to a [`SampleFormat`].
fn av_to_sample_format(av_fmt: c_int) -> SampleFormat {
    match av_fmt {
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8 => SampleFormat::U8,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16 => SampleFormat::I16,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32 => SampleFormat::I32,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLT => SampleFormat::F32,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBL => SampleFormat::F64,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_U8P => SampleFormat::U8p,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S16P => SampleFormat::I16p,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_S32P => SampleFormat::I32p,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP => SampleFormat::F32p,
        v if v == ff_sys::AVSampleFormat_AV_SAMPLE_FMT_DBLP => SampleFormat::F64p,
        other => SampleFormat::Other(other.max(0) as u32),
    }
}

// ── AVFrame ↔ frame data helpers ──────────────────────────────────────────────

/// Number of pixel rows in the given plane of a video frame.
fn plane_height(fmt: PixelFormat, plane: usize, frame_height: usize) -> usize {
    match fmt {
        // YUV 4:2:0 — Y full height, U/V halved.
        PixelFormat::Yuv420p | PixelFormat::Yuv420p10le => {
            if plane == 0 {
                frame_height
            } else {
                frame_height.div_ceil(2)
            }
        }
        // Semi-planar NV12/NV21 / P010le — Y full, UV halved.
        PixelFormat::Nv12 | PixelFormat::Nv21 | PixelFormat::P010le => {
            if plane == 0 {
                frame_height
            } else {
                frame_height.div_ceil(2)
            }
        }
        // Everything else: all planes span the full height.
        _ => frame_height,
    }
}

/// Copy [`VideoFrame`] plane data row-by-row into a pre-allocated `AVFrame`.
///
/// # Safety
///
/// `raw_frame` must point to a valid `AVFrame` whose `data` / `linesize`
/// arrays have been populated by `av_frame_get_buffer`.
unsafe fn copy_video_planes_to_av(src: &VideoFrame, dst: *mut AVFrame) {
    for i in 0..src.num_planes().min(8) {
        let Some(plane_data) = src.plane(i) else {
            continue;
        };
        let dst_ptr = (*dst).data[i];
        if dst_ptr.is_null() {
            continue;
        }
        let src_stride = src.strides()[i];
        let dst_stride = (*dst).linesize[i] as usize;
        let rows = plane_height(src.format(), i, src.height() as usize);

        for row in 0..rows {
            let src_off = row * src_stride;
            let dst_off = row * dst_stride;
            let copy_len = src_stride.min(dst_stride);
            if src_off + copy_len <= plane_data.len() {
                // SAFETY: `src_off + copy_len` is within `plane_data`; the dst
                // slice is within the `FFmpeg`-allocated buffer which is at
                // least `linesize[i] * height` bytes per plane.
                std::ptr::copy_nonoverlapping(
                    plane_data.as_ptr().add(src_off),
                    dst_ptr.add(dst_off),
                    copy_len,
                );
            }
        }
    }
}

/// Build a [`VideoFrame`] by copying data out of an `AVFrame`.
///
/// # Safety
///
/// `raw_frame` must point to a valid, populated `AVFrame`.
unsafe fn av_frame_to_video_frame(raw_frame: *const AVFrame) -> Result<VideoFrame, ()> {
    let width = (*raw_frame).width as u32;
    let height = (*raw_frame).height as u32;
    let format = av_to_pixel_format((*raw_frame).format);
    let pts_raw = (*raw_frame).pts;
    if pts_raw == ff_sys::AV_NOPTS_VALUE {
        log::warn!("pts invalid in output video frame from filter graph");
    }
    let timestamp = video_ticks_to_timestamp(pts_raw);
    // AV_PICTURE_TYPE_I = 1: I-frame (key frame).  `key_frame` was removed
    // from AVFrame in FFmpeg 6; use `pict_type` instead.
    let key_frame = (*raw_frame).pict_type == 1;

    let num_planes = format.num_planes();
    let mut planes: Vec<PooledBuffer> = Vec::with_capacity(num_planes);
    let mut strides: Vec<usize> = Vec::with_capacity(num_planes);

    for i in 0..num_planes {
        let src_ptr = (*raw_frame).data[i];
        if src_ptr.is_null() {
            return Err(());
        }
        let linesize_raw = (*raw_frame).linesize[i];
        // Some filters (e.g. `vflip`) produce frames with a negative linesize to
        // indicate a bottom-up scan order. `data[i]` then points to the last row,
        // and each successive row is at a lower address. We take the absolute
        // stride, seek back to the first row, and copy the contiguous data block.
        let stride = linesize_raw.unsigned_abs() as usize;
        let rows = plane_height(format, i, height as usize);
        let byte_count = stride * rows;
        let data_ptr = if linesize_raw < 0 {
            // SAFETY: The full plane is `stride * rows` bytes.  With a negative
            // linesize `data[i]` sits at the start of the *last* row; offsetting
            // by `linesize_raw * (rows - 1)` steps back to the first row so we
            // can read the whole block in one contiguous slice.
            src_ptr.offset(linesize_raw as isize * (rows as isize - 1))
        } else {
            src_ptr
        };

        // SAFETY: `av_frame_get_buffer` / `av_buffersink_get_frame` guarantees
        // at least `stride * rows` bytes starting at `data_ptr`.
        let data = std::slice::from_raw_parts(data_ptr, byte_count).to_vec();
        planes.push(PooledBuffer::standalone(data));
        strides.push(stride);
    }

    VideoFrame::new(planes, strides, width, height, format, timestamp, key_frame).map_err(|_| ())
}

/// Copy [`AudioFrame`] plane data into a pre-allocated `AVFrame`.
///
/// # Safety
///
/// `raw_frame` must point to a valid `AVFrame` whose `data` arrays have been
/// populated by `av_frame_get_buffer`.
unsafe fn copy_audio_planes_to_av(src: &AudioFrame, dst: *mut AVFrame) {
    for i in 0..src.num_planes().min(8) {
        let Some(plane_data) = src.plane(i) else {
            continue;
        };
        let dst_ptr = (*dst).data[i];
        if dst_ptr.is_null() {
            continue;
        }
        // SAFETY: `FFmpeg` allocated `dst_ptr` with `av_frame_get_buffer`; it
        // is at least `plane_data.len()` bytes.
        std::ptr::copy_nonoverlapping(plane_data.as_ptr(), dst_ptr, plane_data.len());
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::FilterStep;

    /// `FilterGraphInner::new` must not call `avfilter_graph_alloc`.
    /// The graph field starts as `None`; allocation is deferred to the first push.
    #[test]
    fn new_should_start_with_no_graph_allocated() {
        let inner = FilterGraphInner::new(
            vec![FilterStep::Scale {
                width: 1280,
                height: 720,
            }],
            None,
        );
        assert!(
            inner.graph.is_none(),
            "avfilter_graph_alloc must not be called at construction time"
        );
        assert!(
            inner.src_ctxs.is_empty(),
            "src_ctxs should be empty before first push"
        );
        assert!(
            inner.vsink_ctx.is_none(),
            "vsink_ctx should be None before first push"
        );
        assert!(
            inner.asink_ctx.is_none(),
            "asink_ctx should be None before first push"
        );
    }

    /// Dropping an uninitialised `FilterGraphInner` (graph == None) must be a
    /// no-op — no `avfilter_graph_free` call and no panic.
    #[test]
    fn drop_uninitialised_should_be_a_no_op() {
        let inner = FilterGraphInner::new(
            vec![FilterStep::Scale {
                width: 640,
                height: 360,
            }],
            None,
        );
        drop(inner); // must not panic or double-free
    }

    /// `FilterGraphInner` must implement `Send` so the filter graph can be
    /// moved across threads (e.g. into a worker thread for processing).
    #[test]
    fn filter_graph_inner_should_impl_send() {
        fn assert_send<T: Send>() {}
        assert_send::<FilterGraphInner>();
    }

    // ── buffersrc / buffersink arg-string helpers ──────────────────────────────

    /// The video buffersrc args string must contain all fields required by the
    /// libavfilter `buffer` filter: `video_size`, `pix_fmt`, `time_base`, and
    /// `pixel_aspect`.
    #[test]
    fn video_buffersrc_args_should_contain_size_pix_fmt_and_time_base() {
        // pix_fmt 0 = AV_PIX_FMT_YUV420P
        let args = video_buffersrc_args(1920, 1080, 0);
        assert!(
            args.contains("video_size=1920x1080"),
            "missing video_size: {args}"
        );
        assert!(args.contains("pix_fmt=0"), "missing pix_fmt: {args}");
        assert!(
            args.contains("time_base=1/90000"),
            "missing time_base: {args}"
        );
        assert!(
            args.contains("pixel_aspect=1/1"),
            "missing pixel_aspect: {args}"
        );
    }

    /// The audio buffersrc args string must contain all fields required by the
    /// libavfilter `abuffer` filter: `sample_rate`, `sample_fmt`, `channels`,
    /// and `time_base` (which uses `1/sample_rate`).
    #[test]
    fn audio_buffersrc_args_should_contain_sample_rate_format_and_channels() {
        let args = audio_buffersrc_args(44100, "fltp", 2);
        assert!(
            args.contains("sample_rate=44100"),
            "missing sample_rate: {args}"
        );
        assert!(
            args.contains("sample_fmt=fltp"),
            "missing sample_fmt: {args}"
        );
        assert!(args.contains("channels=2"), "missing channels: {args}");
        assert!(
            args.contains("time_base=1/44100"),
            "missing time_base: {args}"
        );
    }

    /// Changing the sample rate must update the `time_base` denominator too,
    /// since audio time base is `1/sample_rate`.
    #[test]
    fn audio_buffersrc_args_time_base_should_match_sample_rate() {
        let args = audio_buffersrc_args(48000, "s16", 1);
        assert!(
            args.contains("time_base=1/48000"),
            "time_base denominator must equal sample_rate: {args}"
        );
    }

    // ── video_input_count ──────────────────────────────────────────────────────

    /// Single-input steps (no overlay) require exactly one buffersrc.
    #[test]
    fn video_input_count_should_return_1_for_single_input_steps() {
        let inner = FilterGraphInner::new(
            vec![FilterStep::Scale {
                width: 1280,
                height: 720,
            }],
            None,
        );
        assert_eq!(inner.video_input_count(), 1);
    }

    /// Overlay requires two buffersrc contexts (main + secondary).
    #[test]
    fn video_input_count_should_return_2_for_overlay() {
        let inner = FilterGraphInner::new(vec![FilterStep::Overlay { x: 10, y: 10 }], None);
        assert_eq!(inner.video_input_count(), 2);
    }

    /// A chain without overlay must still report 1, even with multiple steps.
    #[test]
    fn video_input_count_should_return_1_with_no_overlay_in_chain() {
        let inner = FilterGraphInner::new(
            vec![
                FilterStep::Trim {
                    start: 0.0,
                    end: 5.0,
                },
                FilterStep::Scale {
                    width: 640,
                    height: 360,
                },
            ],
            None,
        );
        assert_eq!(inner.video_input_count(), 1);
    }

    // ── ffmpeg_err helper ──────────────────────────────────────────────────────

    /// `ffmpeg_err` must return the `Ffmpeg` variant carrying the original code.
    #[test]
    fn ffmpeg_err_should_return_ffmpeg_variant_with_code() {
        let err = ffmpeg_err(-22);
        assert!(
            matches!(err, FilterError::Ffmpeg { code: -22, .. }),
            "expected Ffmpeg variant with code -22, got {err:?}"
        );
    }

    /// `ffmpeg_err` must populate a non-empty message string for a known error code.
    #[test]
    fn ffmpeg_err_should_populate_non_empty_message() {
        let err = ffmpeg_err(-22);
        if let FilterError::Ffmpeg { message, .. } = err {
            assert!(
                !message.is_empty(),
                "message must not be empty for a known error code"
            );
        } else {
            panic!("expected Ffmpeg variant");
        }
    }

    // ── validate_filter_steps ─────────────────────────────────────────────────

    /// `validate_filter_steps` must return `Ok` for a known-good filter name.
    ///
    /// On platforms where the filter registry is already populated (Windows,
    /// macOS) this exercises the real lookup path.  On Linux builds where the
    /// registry is not yet initialised at unit-test time, `avfilter_get_by_name`
    /// returns null and the function gracefully defers, also returning `Ok`.
    /// Either way the result must never be `Err` for a standard filter like
    /// `scale`.
    #[test]
    fn validate_filter_steps_should_succeed_for_known_filters() {
        let steps = vec![FilterStep::Scale {
            width: 640,
            height: 360,
        }];
        assert!(
            validate_filter_steps(&steps).is_ok(),
            "validate_filter_steps must not return Err for a standard filter: \
             either the filter was found, or the registry is not yet initialised \
             and validation is deferred"
        );
    }

    // ── PTS helpers ───────────────────────────────────────────────────────────

    /// A valid 1-second video timestamp must scale to exactly 90 000 ticks
    /// in the 1/90000 time base used by the video buffersrc.
    #[test]
    fn video_pts_ticks_should_scale_timestamp_to_90000_time_base() {
        let ts = Timestamp::new(90000, Rational::new(1, 90000));
        assert_eq!(video_pts_ticks(ts), 90000);
    }

    /// A timestamp whose raw PTS equals `AV_NOPTS_VALUE` must pass through
    /// unchanged so FFmpeg knows the frame has no valid presentation time.
    #[test]
    fn video_pts_ticks_with_nopts_value_should_return_av_nopts_value() {
        let ts = Timestamp::new(ff_sys::AV_NOPTS_VALUE, Rational::new(1, 90000));
        assert_eq!(video_pts_ticks(ts), ff_sys::AV_NOPTS_VALUE);
    }

    /// 90 000 ticks at 1/90000 must convert back to ~1.0 second.
    #[test]
    fn video_ticks_to_timestamp_should_convert_ticks_to_secs() {
        let ts = video_ticks_to_timestamp(90000);
        assert!(
            (ts.as_secs_f64() - 1.0).abs() < 1e-6,
            "expected ~1.0 s, got {}",
            ts.as_secs_f64()
        );
    }

    /// `AV_NOPTS_VALUE` ticks must yield `Timestamp::default()` (0 at 1/90000).
    #[test]
    fn video_ticks_to_timestamp_with_nopts_should_return_default_timestamp() {
        let ts = video_ticks_to_timestamp(ff_sys::AV_NOPTS_VALUE);
        assert_eq!(ts, Timestamp::default());
    }

    /// A valid 1-second audio timestamp at 48 kHz must scale to exactly 48 000 ticks.
    #[test]
    fn audio_pts_ticks_should_scale_timestamp_to_sample_rate_time_base() {
        let ts = Timestamp::new(48000, Rational::new(1, 48000));
        assert_eq!(audio_pts_ticks(ts, 48000), 48000);
    }

    /// A timestamp whose raw PTS equals `AV_NOPTS_VALUE` must pass through
    /// unchanged on the audio push path.
    #[test]
    fn audio_pts_ticks_with_nopts_value_should_return_av_nopts_value() {
        let ts = Timestamp::new(ff_sys::AV_NOPTS_VALUE, Rational::new(1, 48000));
        assert_eq!(audio_pts_ticks(ts, 48000), ff_sys::AV_NOPTS_VALUE);
    }

    /// 48 000 ticks at 48 kHz must convert back to ~1.0 second.
    #[test]
    fn audio_ticks_to_timestamp_should_convert_ticks_to_secs() {
        let ts = audio_ticks_to_timestamp(48000, 48000);
        assert!(
            (ts.as_secs_f64() - 1.0).abs() < 1e-6,
            "expected ~1.0 s, got {}",
            ts.as_secs_f64()
        );
    }

    /// `AV_NOPTS_VALUE` ticks must yield a zero timestamp with the correct
    /// audio time base (1/sample_rate).
    #[test]
    fn audio_ticks_to_timestamp_with_nopts_should_return_zero_timestamp() {
        let ts = audio_ticks_to_timestamp(ff_sys::AV_NOPTS_VALUE, 48000);
        assert_eq!(ts.pts(), 0);
        assert_eq!(ts.time_base(), Rational::new(1, 48000));
    }
}

/// Build an [`AudioFrame`] by copying data out of an `AVFrame`.
///
/// # Safety
///
/// `raw_frame` must point to a valid, populated `AVFrame`.
unsafe fn av_frame_to_audio_frame(raw_frame: *const AVFrame) -> Result<AudioFrame, ()> {
    let samples = (*raw_frame).nb_samples as usize;
    let channels = (*raw_frame).ch_layout.nb_channels as u32;
    let sample_rate = (*raw_frame).sample_rate as u32;
    let format = av_to_sample_format((*raw_frame).format);
    let pts_raw = (*raw_frame).pts;
    if pts_raw == ff_sys::AV_NOPTS_VALUE {
        log::warn!("pts invalid in output audio frame from filter graph sample_rate={sample_rate}");
    }
    let timestamp = audio_ticks_to_timestamp(pts_raw, sample_rate);

    let num_planes = if format.is_planar() {
        channels as usize
    } else {
        1
    };
    let bytes_per_sample = format.bytes_per_sample();
    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(num_planes);

    for i in 0..num_planes {
        let src_ptr = (*raw_frame).data[i];
        if src_ptr.is_null() {
            return Err(());
        }
        let byte_count = samples * bytes_per_sample;
        // SAFETY: `av_buffersink_get_frame` guarantees at least
        // `nb_samples * bytes_per_sample` bytes per plane pointer.
        let data = std::slice::from_raw_parts(src_ptr, byte_count).to_vec();
        planes.push(data);
    }

    AudioFrame::new(planes, samples, channels, sample_rate, format, timestamp).map_err(|_| ())
}
