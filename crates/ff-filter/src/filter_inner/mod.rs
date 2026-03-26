//! Low-level filter graph wrapper ([`FilterGraphInner`]).

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

mod build;
mod convert;

use build::{
    add_and_link_step, add_atempo_chain, add_fit_to_aspect_pad, add_overlay_image_step,
    add_setpts_after_trim, audio_buffersrc_args, create_hw_filter, hw_accel_to_device_type,
    video_buffersrc_args,
};
use convert::{
    audio_pts_ticks, av_frame_to_audio_frame, av_frame_to_video_frame, copy_audio_planes_to_av,
    copy_video_planes_to_av, pixel_format_to_av, sample_format_to_av, sample_format_to_av_name,
    video_pts_ticks,
};

use std::os::raw::c_int;
use std::ptr::NonNull;

use ff_format::{AudioFrame, VideoFrame};

use crate::error::FilterError;
use crate::graph::{FilterStep, HwAccel};

// ── Time base used for the buffersrc ─────────────────────────────────────────

/// The time base numerator used for the video buffersrc (1/90000).
pub(super) const VIDEO_TIME_BASE_NUM: i32 = 1;
/// The time base denominator used for the video buffersrc (1/90000).
pub(super) const VIDEO_TIME_BASE_DEN: i32 = 90_000;

/// The time base numerator used for the audio abuffersrc (`1/sample_rate`).
pub(super) const AUDIO_TIME_BASE_NUM: i32 = 1;

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
pub(super) fn ffmpeg_err(code: i32) -> FilterError {
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
            // AReverse is audio-only; skip it in the video graph.
            if matches!(step, FilterStep::AReverse) {
                continue;
            }

            // OverlayImage is a compound step (movie → lut → overlay).  It
            // creates its own internal source node via the `movie` filter and
            // does not consume a buffersrc slot, so it must bypass the standard
            // `add_and_link_step` path which assumes a single filter per step.
            if let FilterStep::OverlayImage {
                path,
                x,
                y,
                opacity,
            } = step
            {
                prev_ctx = match add_overlay_image_step(graph, prev_ctx, path, x, y, *opacity, i) {
                    Ok(ctx) => ctx,
                    Err(e) => bail!(e),
                };
                continue;
            }

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

            // FitToAspect is a compound step: the scale filter (added by
            // add_and_link_step above) preserves the source aspect ratio; the
            // pad filter added here centres the scaled frame on the target canvas.
            if let FilterStep::FitToAspect {
                width,
                height,
                color,
            } = step
            {
                prev_ctx = match add_fit_to_aspect_pad(graph, prev_ctx, *width, *height, color, i) {
                    Ok(ctx) => ctx,
                    Err(e) => bail!(e),
                };
            }

            // Overlay and xfade both consume a second input on pad 1.
            if matches!(step, FilterStep::Overlay { .. } | FilterStep::XFade { .. })
                && let Some(Some(extra_src)) = src_ctxs.get(1)
            {
                let ret = ff_sys::avfilter_link(extra_src.as_ptr(), 0, prev_ctx, 1);
                if ret < 0 {
                    bail!(FilterError::BuildFailed);
                }
                log::debug!("filter linked extra_input=in1 to two-input filter pad=1");
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
            if matches!(step, FilterStep::Overlay { .. } | FilterStep::XFade { .. }) {
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
            // Reverse is video-only; skip it in the audio graph.
            if matches!(step, FilterStep::Reverse) {
                continue;
            }

            // Speed uses `setpts` for video but `atempo` for audio.  Bypass the
            // standard `add_and_link_step` path and insert the atempo chain here.
            if let FilterStep::Speed { factor } = step {
                prev_ctx = add_atempo_chain(graph, prev_ctx, *factor, i)?;
                continue;
            }
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
                algorithm: crate::graph::ScaleAlgorithm::Fast,
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
                algorithm: crate::graph::ScaleAlgorithm::Fast,
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
                algorithm: crate::graph::ScaleAlgorithm::Fast,
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
                    algorithm: crate::graph::ScaleAlgorithm::Fast,
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
            algorithm: crate::graph::ScaleAlgorithm::Fast,
        }];
        assert!(
            validate_filter_steps(&steps).is_ok(),
            "validate_filter_steps must not return Err for a standard filter: \
             either the filter was found, or the registry is not yet initialised \
             and validation is deferred"
        );
    }
}
