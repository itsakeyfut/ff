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

// ── FFmpeg error helper ───────────────────────────────────────────────────────

/// Convert a negative `FFmpeg` return code into a [`FilterError::Ffmpeg`].
fn ffmpeg_err(code: i32) -> FilterError {
    FilterError::Ffmpeg {
        code,
        message: ff_sys::av_error_string(code),
    }
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
}

// SAFETY: `FilterGraphInner` owns all raw pointers exclusively.  No other
// thread holds references to the underlying `AVFilterGraph` or any of its
// contexts while this struct is alive.
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
                Ok((src_ctxs, vsink_ctx)) => {
                    self.graph = Some(graph_nn);
                    self.src_ctxs = src_ctxs;
                    self.vsink_ctx = Some(vsink_ctx);
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
        _hw: Option<&HwAccel>,
    ) -> BuildResult {
        let graph = graph_nn.as_ptr();

        // SAFETY: `avfilter_get_by_name` returns a borrowed pointer valid for
        // the process lifetime; we never free it.
        let buffersrc = ff_sys::avfilter_get_by_name(c"buffer".as_ptr());
        if buffersrc.is_null() {
            return Err(FilterError::BuildFailed);
        }

        let src_args =
            std::ffi::CString::new(buffersrc_args).map_err(|_| FilterError::BuildFailed)?;
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
            return Err(FilterError::BuildFailed);
        }
        log::debug!("filter added name=buffersrc slot=0");
        // SAFETY: ret >= 0 guarantees non-null.
        src_ctxs.push(Some(NonNull::new_unchecked(raw_ctx0)));

        // Create in1..inN-1 (for overlay etc.)
        for slot in 1..num_inputs {
            let ctx_name = std::ffi::CString::new(format!("in{slot}"))
                .map_err(|_| FilterError::BuildFailed)?;
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
                return Err(FilterError::BuildFailed);
            }
            log::debug!("filter added name=buffersrc slot={slot}");
            // SAFETY: ret >= 0 guarantees non-null.
            src_ctxs.push(Some(NonNull::new_unchecked(raw_ctx)));
        }

        // 2. Create buffersink ("buffersink").
        let buffersink = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
        if buffersink.is_null() {
            return Err(FilterError::BuildFailed);
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
            return Err(FilterError::BuildFailed);
        }

        // 3-5. Add each `FilterStep`, link the main chain (in0 → step[0] → …),
        // and wire extra input pads for multi-input filters.
        let mut prev_ctx: *mut ff_sys::AVFilterContext = raw_ctx0;
        for (i, step) in steps.iter().enumerate() {
            prev_ctx = add_and_link_step(graph, prev_ctx, step, i, "step")?;

            // Overlay consumes a second input on pad 1.
            if matches!(step, FilterStep::Overlay { .. })
                && let Some(Some(extra_src)) = src_ctxs.get(1)
            {
                let ret = ff_sys::avfilter_link(extra_src.as_ptr(), 0, prev_ctx, 1);
                if ret < 0 {
                    return Err(FilterError::BuildFailed);
                }
                log::debug!("filter linked extra_input=in1 to overlay pad=1");
            }
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

        // SAFETY: `avfilter_graph_create_filter` with ret >= 0 guarantees
        // non-null pointers.
        let sink_nn = NonNull::new_unchecked(sink_ctx);
        Ok((src_ctxs, sink_nn))
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
            (*raw_frame).pts =
                (frame.timestamp().as_secs_f64() * f64::from(VIDEO_TIME_BASE_DEN)) as i64;

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
            (*raw_frame).pts =
                (frame.timestamp().as_secs_f64() * f64::from(frame.sample_rate())) as i64;
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
            unsafe {
                let mut raw = ptr.as_ptr();
                ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(raw));
            }
        }
    }
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
    let secs = pts_raw as f64 / f64::from(VIDEO_TIME_BASE_DEN);
    let timestamp = Timestamp::from_secs_f64(
        secs,
        Rational::new(VIDEO_TIME_BASE_NUM, VIDEO_TIME_BASE_DEN),
    );
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
        let stride = (*raw_frame).linesize[i] as usize;
        let rows = plane_height(format, i, height as usize);
        let byte_count = stride * rows;

        // SAFETY: `av_frame_get_buffer` / `av_buffersink_get_frame` guarantees
        // at least `linesize[i] * rows` bytes per plane pointer.
        let data = std::slice::from_raw_parts(src_ptr, byte_count).to_vec();
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
    let secs = if sample_rate > 0 {
        pts_raw as f64 / f64::from(sample_rate)
    } else {
        0.0
    };
    let time_base = Rational::new(
        1,
        if sample_rate > 0 {
            sample_rate as i32
        } else {
            1
        },
    );
    let timestamp = Timestamp::from_secs_f64(secs, time_base);

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
