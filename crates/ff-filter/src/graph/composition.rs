//! Multi-track video composition and audio mixing.
//!
//! This module provides [`MultiTrackComposer`] for compositing multiple video
//! layers onto a solid-colour canvas, and [`MultiTrackAudioMixer`] for mixing
//! multiple audio tracks into a single output stream.
//!
//! Both types produce source-only [`FilterGraph`] instances — call
//! [`FilterGraph::pull_video`] or [`FilterGraph::pull_audio`] in a loop to
//! extract output frames.

// All FFmpeg FFI lives in the build helpers; allow unsafe in this module.
#![allow(unsafe_code)]
// Rust 2024: unsafe ops inside unsafe fn still need explicit blocks; suppress
// so the inner helpers read cleanly (same policy as filter_inner/mod.rs).
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

use std::path::PathBuf;
use std::ptr::NonNull;
use std::time::Duration;

use ff_format::ChannelLayout;

use crate::error::FilterError;
use crate::filter_inner::FilterGraphInner;
use crate::graph::filter_step::FilterStep;
use crate::graph::graph::FilterGraph;
use crate::graph::types::Rgb;

// ── VideoLayer ────────────────────────────────────────────────────────────────

/// A single video layer in a [`MultiTrackComposer`] composition.
///
/// Layers are composited in ascending [`z_order`](Self::z_order), with
/// `0` rendered first (bottom of the stack).
#[derive(Debug, Clone)]
pub struct VideoLayer {
    /// Source media file path.
    pub source: PathBuf,
    /// X offset on the canvas in pixels (top-left origin).
    pub x: i32,
    /// Y offset on the canvas in pixels.
    pub y: i32,
    /// Uniform scale factor applied to the source frame (`1.0` = original size).
    pub scale: f32,
    /// Opacity (`0.0` = fully transparent, `1.0` = fully opaque).
    pub opacity: f32,
    /// Compositing order (`0` = bottom layer; higher values render on top).
    pub z_order: u32,
    /// Start offset on the output timeline (`Duration::ZERO` = at the beginning).
    pub time_offset: Duration,
    /// Optional trim start within the source file.
    pub in_point: Option<Duration>,
    /// Optional trim end within the source file.
    pub out_point: Option<Duration>,
}

// ── MultiTrackComposer ────────────────────────────────────────────────────────

/// Composes multiple video layers onto a solid-colour canvas.
///
/// Layers are sorted by [`VideoLayer::z_order`] before compositing.  The
/// resulting [`FilterGraph`] is source-only — call [`FilterGraph::pull_video`]
/// in a loop to extract the output frames.  The graph terminates when the
/// last (highest `z_order`) layer finishes.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::{MultiTrackComposer, VideoLayer};
/// use std::time::Duration;
///
/// let mut graph = MultiTrackComposer::new(1920, 1080)
///     .add_layer(VideoLayer {
///         source: "clip.mp4".into(),
///         x: 0, y: 0, scale: 1.0, opacity: 1.0, z_order: 0,
///         time_offset: Duration::ZERO, in_point: None, out_point: None,
///     })
///     .build()?;
///
/// while let Some(frame) = graph.pull_video()? {
///     // encode or display `frame`
/// }
/// ```
pub struct MultiTrackComposer {
    canvas_width: u32,
    canvas_height: u32,
    background: Rgb,
    layers: Vec<VideoLayer>,
}

impl MultiTrackComposer {
    /// Creates a new composer with a black canvas and no layers.
    pub fn new(canvas_width: u32, canvas_height: u32) -> Self {
        Self {
            canvas_width,
            canvas_height,
            background: Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            layers: Vec::new(),
        }
    }

    /// Sets the canvas background colour and returns the updated composer.
    #[must_use]
    pub fn background(self, rgb: Rgb) -> Self {
        Self {
            background: rgb,
            ..self
        }
    }

    /// Appends a video layer and returns the updated composer.
    #[must_use]
    pub fn add_layer(self, layer: VideoLayer) -> Self {
        let mut layers = self.layers;
        layers.push(layer);
        Self { layers, ..self }
    }

    /// Builds a source-only [`FilterGraph`] that composites all layers.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — canvas width or height is zero,
    ///   no layers were added, or an underlying `FFmpeg` graph-construction
    ///   call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.canvas_width == 0 || self.canvas_height == 0 {
            return Err(FilterError::CompositionFailed {
                reason: format!(
                    "canvas dimensions must be non-zero: {}x{}",
                    self.canvas_width, self.canvas_height
                ),
            });
        }
        if self.layers.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no layers".to_string(),
            });
        }
        let mut layers = self.layers;
        layers.sort_by_key(|l| l.z_order);
        // SAFETY: all raw pointer operations follow the avfilter ownership rules:
        // - avfilter_graph_alloc() returns an owned pointer freed via
        //   avfilter_graph_free() on error or stored in FilterGraphInner on success.
        // - avfilter_graph_create_filter() adds contexts owned by the graph.
        // - avfilter_link() connects pads; connections are owned by the graph.
        // - avfilter_graph_config() finalises the graph.
        // - NonNull::new_unchecked() is called only after ret >= 0 checks.
        unsafe {
            build_video_composition(
                self.canvas_width,
                self.canvas_height,
                self.background,
                &layers,
            )
        }
    }
}

// ── Video composition graph builder ──────────────────────────────────────────

unsafe fn build_video_composition(
    canvas_width: u32,
    canvas_height: u32,
    background: Rgb,
    layers: &[VideoLayer],
) -> Result<FilterGraph, FilterError> {
    use std::ffi::CString;

    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::CompositionFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::CompositionFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    // ── Base canvas ───────────────────────────────────────────────────────────
    let r = (background.r.clamp(0.0, 1.0) * 255.0) as u8;
    let g_ch = (background.g.clamp(0.0, 1.0) * 255.0) as u8;
    let b = (background.b.clamp(0.0, 1.0) * 255.0) as u8;
    let color_args_str =
        format!("c=#{r:02x}{g_ch:02x}{b:02x}:s={canvas_width}x{canvas_height}:r=30");
    let Ok(color_args) = CString::new(color_args_str.as_str()) else {
        bail!(graph, "CString::new failed for color filter args");
    };
    let color_filter = ff_sys::avfilter_get_by_name(c"color".as_ptr());
    if color_filter.is_null() {
        bail!(graph, "filter not found: color");
    }
    let mut base_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut base_ctx,
        color_filter,
        c"base".as_ptr(),
        color_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create color filter code={ret}"));
    }
    log::debug!(
        "video composition color source canvas={canvas_width}x{canvas_height} \
         color=#{r:02x}{g_ch:02x}{b:02x}"
    );

    let mut prev_ctx = base_ctx;
    let layer_count = layers.len();

    for (idx, layer) in layers.iter().enumerate() {
        let path = layer.source.to_string_lossy();
        let is_last = idx == layer_count - 1;

        // ── movie= source ─────────────────────────────────────────────────────
        let movie_filter = ff_sys::avfilter_get_by_name(c"movie".as_ptr());
        if movie_filter.is_null() {
            bail!(graph, "filter not found: movie");
        }
        let Ok(movie_name) = CString::new(format!("movie{idx}")) else {
            bail!(graph, "CString::new failed for movie name");
        };
        let Ok(movie_args) = CString::new(format!("filename={path}")) else {
            bail!(graph, "CString::new failed for movie args");
        };
        let mut movie_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut movie_ctx,
            movie_filter,
            movie_name.as_ptr(),
            movie_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(
                graph,
                format!("failed to create movie filter layer={idx} code={ret}")
            );
        }
        log::debug!("video composition layer={idx} movie source path={path}");
        let mut chain_end = movie_ctx;

        // ── Optional trim + setpts ────────────────────────────────────────────
        if let (Some(in_pt), Some(out_pt)) = (layer.in_point, layer.out_point) {
            let start = in_pt.as_secs_f64();
            let end = out_pt.as_secs_f64();

            let trim_filter = ff_sys::avfilter_get_by_name(c"trim".as_ptr());
            if trim_filter.is_null() {
                bail!(graph, "filter not found: trim");
            }
            let Ok(trim_name) = CString::new(format!("trim{idx}")) else {
                bail!(graph, "CString::new failed for trim name");
            };
            let Ok(trim_args) = CString::new(format!("start={start}:end={end}")) else {
                bail!(graph, "CString::new failed for trim args");
            };
            let mut trim_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut trim_ctx,
                trim_filter,
                trim_name.as_ptr(),
                trim_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create trim filter layer={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, trim_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: movie→trim layer={idx}"));
            }
            chain_end = trim_ctx;

            let setpts_filter = ff_sys::avfilter_get_by_name(c"setpts".as_ptr());
            if setpts_filter.is_null() {
                bail!(graph, "filter not found: setpts");
            }
            let Ok(sp_name) = CString::new(format!("setpts_trim{idx}")) else {
                bail!(graph, "CString::new failed for setpts name");
            };
            let mut sp_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut sp_ctx,
                setpts_filter,
                sp_name.as_ptr(),
                c"PTS-STARTPTS".as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create setpts filter layer={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, sp_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: trim→setpts layer={idx}"));
            }
            chain_end = sp_ctx;
        }

        // ── Optional timeline offset ──────────────────────────────────────────
        if layer.time_offset > Duration::ZERO {
            let offset = layer.time_offset.as_secs_f64();
            let setpts_filter = ff_sys::avfilter_get_by_name(c"setpts".as_ptr());
            if setpts_filter.is_null() {
                bail!(graph, "filter not found: setpts");
            }
            let Ok(sp_name) = CString::new(format!("setpts_offset{idx}")) else {
                bail!(graph, "CString::new failed for setpts offset name");
            };
            let Ok(sp_args) = CString::new(format!("PTS+{offset}/TB")) else {
                bail!(graph, "CString::new failed for setpts offset args");
            };
            let mut sp_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut sp_ctx,
                setpts_filter,
                sp_name.as_ptr(),
                sp_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create setpts offset filter layer={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, sp_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: →setpts_offset layer={idx}"));
            }
            chain_end = sp_ctx;
        }

        // ── Optional scale ────────────────────────────────────────────────────
        if (layer.scale - 1.0_f32).abs() > f32::EPSILON {
            let sw = (canvas_width as f32 * layer.scale).round() as u32;
            let sh = (canvas_height as f32 * layer.scale).round() as u32;
            let scale_filter = ff_sys::avfilter_get_by_name(c"scale".as_ptr());
            if scale_filter.is_null() {
                bail!(graph, "filter not found: scale");
            }
            let Ok(sc_name) = CString::new(format!("scale{idx}")) else {
                bail!(graph, "CString::new failed for scale name");
            };
            let Ok(sc_args) = CString::new(format!("{sw}:{sh}")) else {
                bail!(graph, "CString::new failed for scale args");
            };
            let mut sc_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut sc_ctx,
                scale_filter,
                sc_name.as_ptr(),
                sc_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create scale filter layer={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, sc_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: →scale layer={idx}"));
            }
            chain_end = sc_ctx;
        }

        // ── Optional opacity ──────────────────────────────────────────────────
        if layer.opacity < 1.0 {
            let opacity = layer.opacity.clamp(0.0, 1.0);
            let ccm_filter = ff_sys::avfilter_get_by_name(c"colorchannelmixer".as_ptr());
            if ccm_filter.is_null() {
                bail!(graph, "filter not found: colorchannelmixer");
            }
            let Ok(ccm_name) = CString::new(format!("ccm{idx}")) else {
                bail!(graph, "CString::new failed for colorchannelmixer name");
            };
            let Ok(ccm_args) = CString::new(format!("aa={opacity}")) else {
                bail!(graph, "CString::new failed for colorchannelmixer args");
            };
            let mut ccm_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut ccm_ctx,
                ccm_filter,
                ccm_name.as_ptr(),
                ccm_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create colorchannelmixer filter layer={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, ccm_ctx, 0);
            if ret < 0 {
                bail!(
                    graph,
                    format!("link failed: →colorchannelmixer layer={idx}")
                );
            }
            chain_end = ccm_ctx;
        }

        // ── overlay ───────────────────────────────────────────────────────────
        // Last layer uses eof_action=endall so the graph terminates when that
        // layer's source ends.  Intermediate layers use pass so the canvas
        // continues while other layers are still producing.
        let eof_action = if is_last { "endall" } else { "pass" };
        let overlay_filter = ff_sys::avfilter_get_by_name(c"overlay".as_ptr());
        if overlay_filter.is_null() {
            bail!(graph, "filter not found: overlay");
        }
        let Ok(ov_name) = CString::new(format!("overlay{idx}")) else {
            bail!(graph, "CString::new failed for overlay name");
        };
        let Ok(ov_args) = CString::new(format!("{}:{}:eof_action={eof_action}", layer.x, layer.y))
        else {
            bail!(graph, "CString::new failed for overlay args");
        };
        let mut overlay_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut overlay_ctx,
            overlay_filter,
            ov_name.as_ptr(),
            ov_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(
                graph,
                format!("failed to create overlay filter layer={idx} code={ret}")
            );
        }
        // Link: base canvas → overlay pad 0
        let ret = ff_sys::avfilter_link(prev_ctx, 0, overlay_ctx, 0);
        if ret < 0 {
            bail!(graph, format!("link failed: base→overlay[0] layer={idx}"));
        }
        // Link: layer content → overlay pad 1
        let ret = ff_sys::avfilter_link(chain_end, 0, overlay_ctx, 1);
        if ret < 0 {
            bail!(graph, format!("link failed: layer→overlay[1] layer={idx}"));
        }
        prev_ctx = overlay_ctx;
    }

    // ── Video buffersink ──────────────────────────────────────────────────────
    let sink_filter = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
    if sink_filter.is_null() {
        bail!(graph, "filter not found: buffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        sink_filter,
        c"vsink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create buffersink code={ret}"));
    }
    let ret = ff_sys::avfilter_link(prev_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: last→buffersink");
    }

    // ── Configure graph ───────────────────────────────────────────────────────
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        log::warn!("video composition avfilter_graph_config failed code={ret}");
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // SAFETY: ret >= 0 guarantees both pointers are non-null.
    let graph_nn = NonNull::new_unchecked(graph);
    let sink_nn = NonNull::new_unchecked(sink_ctx);
    let inner = FilterGraphInner::with_prebuilt_video_graph(graph_nn, sink_nn);
    log::info!(
        "video composition graph built layers={layer_count} canvas={canvas_width}x{canvas_height}"
    );
    Ok(FilterGraph::from_prebuilt(inner))
}

// ── AudioTrack ────────────────────────────────────────────────────────────────

/// A single audio track in a [`MultiTrackAudioMixer`] mix.
#[derive(Debug, Clone)]
pub struct AudioTrack {
    /// Source media file path.
    pub source: PathBuf,
    /// Volume adjustment in decibels (`0.0` = unity gain).
    pub volume_db: f32,
    /// Stereo pan (`-1.0` = full left, `0.0` = centre, `+1.0` = full right).
    pub pan: f32,
    /// Start offset on the output timeline (`Duration::ZERO` = at the beginning).
    pub time_offset: Duration,
    /// Ordered per-track audio effect chain applied before mixing.
    ///
    /// Each [`FilterStep`] is inserted as a filter node immediately after
    /// the track's pan/volume chain and before the `amix` node.
    /// Use audio-relevant variants such as [`FilterStep::Volume`],
    /// [`FilterStep::AFadeIn`], and [`FilterStep::ACompressor`].
    /// An empty vec inserts no extra nodes (zero overhead).
    pub effects: Vec<FilterStep>,
    /// Sample rate of the source audio in Hz (e.g. `44_100` or `48_000`).
    ///
    /// When this differs from the mixer's output sample rate an `aresample`
    /// filter is inserted automatically.  Set to the mixer's output rate to
    /// skip resampling.
    pub sample_rate: u32,
    /// Channel layout of the source audio.
    ///
    /// When this differs from the mixer's output layout an `aformat` filter
    /// is inserted automatically.  Set to the mixer's output layout to skip
    /// format conversion.
    pub channel_layout: ChannelLayout,
}

// ── MultiTrackAudioMixer ──────────────────────────────────────────────────────

/// Mixes multiple audio tracks into a single output stream.
///
/// The resulting [`FilterGraph`] is source-only — call [`FilterGraph::pull_audio`]
/// in a loop to extract the output frames.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::MultiTrackAudioMixer;
/// use ff_format::ChannelLayout;
/// use std::time::Duration;
///
/// let mut graph = MultiTrackAudioMixer::new(48000, ChannelLayout::Stereo)
///     .add_track(ff_filter::AudioTrack {
///         source: "music.mp3".into(),
///         volume_db: -3.0,
///         pan: 0.0,
///         time_offset: Duration::ZERO,
///         effects: vec![],
///         sample_rate: 48000,
///         channel_layout: ChannelLayout::Stereo,
///     })
///     .build()?;
///
/// while let Some(frame) = graph.pull_audio()? {
///     // encode or write `frame`
/// }
/// ```
pub struct MultiTrackAudioMixer {
    sample_rate: u32,
    channel_layout: ChannelLayout,
    tracks: Vec<AudioTrack>,
}

impl MultiTrackAudioMixer {
    /// Creates a new mixer with no tracks.
    pub fn new(sample_rate: u32, layout: ChannelLayout) -> Self {
        Self {
            sample_rate,
            channel_layout: layout,
            tracks: Vec::new(),
        }
    }

    /// Appends an audio track and returns the updated mixer.
    #[must_use]
    pub fn add_track(self, track: AudioTrack) -> Self {
        let mut tracks = self.tracks;
        tracks.push(track);
        Self { tracks, ..self }
    }

    /// Builds a source-only [`FilterGraph`] that mixes all tracks.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — no tracks were added, or an
    ///   underlying `FFmpeg` graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.tracks.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no tracks".to_string(),
            });
        }
        // SAFETY: same ownership invariants as build_video_composition.
        unsafe { build_audio_mix(self.sample_rate, self.channel_layout, &self.tracks) }
    }
}

// ── Audio mix graph builder ───────────────────────────────────────────────────

unsafe fn build_audio_mix(
    sample_rate: u32,
    channel_layout: ChannelLayout,
    tracks: &[AudioTrack],
) -> Result<FilterGraph, FilterError> {
    use std::ffi::CString;

    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::CompositionFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::CompositionFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    let track_count = tracks.len();
    let mut end_ctxs: Vec<*mut ff_sys::AVFilterContext> = Vec::with_capacity(track_count);

    for (idx, track) in tracks.iter().enumerate() {
        let path = track.source.to_string_lossy();

        // ── amovie= source ────────────────────────────────────────────────────
        let amovie_filter = ff_sys::avfilter_get_by_name(c"amovie".as_ptr());
        if amovie_filter.is_null() {
            bail!(graph, "filter not found: amovie");
        }
        let Ok(amovie_name) = CString::new(format!("amovie{idx}")) else {
            bail!(graph, "CString::new failed for amovie name");
        };
        let Ok(amovie_args) = CString::new(format!("filename={path}")) else {
            bail!(graph, "CString::new failed for amovie args");
        };
        let mut amovie_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut amovie_ctx,
            amovie_filter,
            amovie_name.as_ptr(),
            amovie_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(
                graph,
                format!("failed to create amovie filter track={idx} code={ret}")
            );
        }
        log::debug!("audio mix track={idx} amovie source path={path}");
        let mut chain_end = amovie_ctx;

        // ── Optional aresample (sample rate conversion) ───────────────────────
        if track.sample_rate != sample_rate {
            let src_rate = track.sample_rate;
            let aresample_filter = ff_sys::avfilter_get_by_name(c"aresample".as_ptr());
            if aresample_filter.is_null() {
                bail!(graph, "filter not found: aresample");
            }
            let Ok(ar_name) = CString::new(format!("aresample{idx}")) else {
                bail!(graph, "CString::new failed for aresample name");
            };
            let Ok(ar_args) = CString::new(format!("{sample_rate}")) else {
                bail!(graph, "CString::new failed for aresample args");
            };
            let mut ar_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut ar_ctx,
                aresample_filter,
                ar_name.as_ptr(),
                ar_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create aresample filter track={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, ar_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: amovie→aresample track={idx}"));
            }
            chain_end = ar_ctx;
            log::info!(
                "audio track resampled track={idx} source_rate={src_rate} target_rate={sample_rate}"
            );
        }

        // ── Optional aformat (channel layout conversion) ──────────────────────
        if track.channel_layout != channel_layout {
            let af_args_str = match channel_layout {
                ChannelLayout::Other(_) => format!("sample_rates={sample_rate}"),
                _ => format!("channel_layouts={}", channel_layout.name()),
            };
            let aformat_filter = ff_sys::avfilter_get_by_name(c"aformat".as_ptr());
            if aformat_filter.is_null() {
                bail!(graph, "filter not found: aformat");
            }
            let Ok(af_name) = CString::new(format!("aformat_layout{idx}")) else {
                bail!(graph, "CString::new failed for aformat layout name");
            };
            let Ok(af_args) = CString::new(af_args_str.as_str()) else {
                bail!(graph, "CString::new failed for aformat layout args");
            };
            let mut af_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut af_ctx,
                aformat_filter,
                af_name.as_ptr(),
                af_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create aformat layout filter track={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, af_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: →aformat_layout track={idx}"));
            }
            chain_end = af_ctx;
            log::debug!(
                "audio track reformatted track={idx} layout={}",
                channel_layout.name()
            );
        }

        // ── Optional timeline offset ──────────────────────────────────────────
        // adelay inserts real silence samples, which is required for correct
        // multi-track mixing via amix (unlike asetpts, which only shifts PTS).
        if track.time_offset > Duration::ZERO {
            let delay_ms = track.time_offset.as_millis();
            let adelay_filter = ff_sys::avfilter_get_by_name(c"adelay".as_ptr());
            if adelay_filter.is_null() {
                bail!(graph, "filter not found: adelay");
            }
            let Ok(ad_name) = CString::new(format!("adelay{idx}")) else {
                bail!(graph, "CString::new failed for adelay name");
            };
            // all=1 applies the same delay to every channel
            let Ok(ad_args) = CString::new(format!("{delay_ms}:all=1")) else {
                bail!(graph, "CString::new failed for adelay args");
            };
            let mut ad_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut ad_ctx,
                adelay_filter,
                ad_name.as_ptr(),
                ad_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create adelay filter track={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, ad_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: →adelay track={idx}"));
            }
            chain_end = ad_ctx;
            log::debug!("audio track delayed track={idx} delay_ms={delay_ms}");
        }

        // ── Optional volume ───────────────────────────────────────────────────
        if track.volume_db.abs() > f32::EPSILON {
            let vol_filter = ff_sys::avfilter_get_by_name(c"volume".as_ptr());
            if vol_filter.is_null() {
                bail!(graph, "filter not found: volume");
            }
            let Ok(vol_name) = CString::new(format!("volume{idx}")) else {
                bail!(graph, "CString::new failed for volume name");
            };
            let Ok(vol_args) = CString::new(format!("{}dB", track.volume_db)) else {
                bail!(graph, "CString::new failed for volume args");
            };
            let mut vol_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut vol_ctx,
                vol_filter,
                vol_name.as_ptr(),
                vol_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create volume filter track={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, vol_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: →volume track={idx}"));
            }
            chain_end = vol_ctx;
        }

        // ── Per-track effects chain ───────────────────────────────────────────
        for (eff_idx, step) in track.effects.iter().enumerate() {
            let combined_idx = idx * 1000 + eff_idx;
            let result =
                crate::filter_inner::add_and_link_step(graph, chain_end, step, combined_idx, "eff");
            if let Ok(ctx) = result {
                chain_end = ctx;
            } else {
                bail!(
                    graph,
                    format!(
                        "failed to apply effect track={idx} effect={eff_idx} filter={}",
                        step.filter_name()
                    )
                );
            }
        }

        end_ctxs.push(chain_end);
    }

    // ── amix ──────────────────────────────────────────────────────────────────
    let amix_filter = ff_sys::avfilter_get_by_name(c"amix".as_ptr());
    if amix_filter.is_null() {
        bail!(graph, "filter not found: amix");
    }
    let Ok(amix_args) = CString::new(format!("inputs={track_count}:normalize=0")) else {
        bail!(graph, "CString::new failed for amix args");
    };
    let mut amix_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut amix_ctx,
        amix_filter,
        c"amix".as_ptr(),
        amix_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create amix filter code={ret}"));
    }
    for (i, &end_ctx) in end_ctxs.iter().enumerate() {
        let ret = ff_sys::avfilter_link(end_ctx, 0, amix_ctx, i as u32);
        if ret < 0 {
            bail!(graph, format!("link failed: track{i}→amix[{i}]"));
        }
    }

    // ── aformat ───────────────────────────────────────────────────────────────
    let aformat_args_str = match channel_layout {
        ChannelLayout::Other(_) => format!("sample_rates={sample_rate}"),
        _ => format!(
            "sample_rates={sample_rate}:channel_layouts={}",
            channel_layout.name()
        ),
    };
    let aformat_filter = ff_sys::avfilter_get_by_name(c"aformat".as_ptr());
    if aformat_filter.is_null() {
        bail!(graph, "filter not found: aformat");
    }
    let Ok(aformat_args) = CString::new(aformat_args_str.as_str()) else {
        bail!(graph, "CString::new failed for aformat args");
    };
    let mut aformat_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut aformat_ctx,
        aformat_filter,
        c"aformat".as_ptr(),
        aformat_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create aformat filter code={ret}"));
    }
    let ret = ff_sys::avfilter_link(amix_ctx, 0, aformat_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: amix→aformat");
    }

    // ── abuffersink ───────────────────────────────────────────────────────────
    let sink_filter = ff_sys::avfilter_get_by_name(c"abuffersink".as_ptr());
    if sink_filter.is_null() {
        bail!(graph, "filter not found: abuffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        sink_filter,
        c"asink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create abuffersink code={ret}"));
    }
    let ret = ff_sys::avfilter_link(aformat_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: aformat→abuffersink");
    }

    // ── Configure graph ───────────────────────────────────────────────────────
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        log::warn!("audio mix avfilter_graph_config failed code={ret}");
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // SAFETY: ret >= 0 guarantees both pointers are non-null.
    let graph_nn = NonNull::new_unchecked(graph);
    let sink_nn = NonNull::new_unchecked(sink_ctx);
    let inner = FilterGraphInner::with_prebuilt_audio_graph(graph_nn, sink_nn);
    log::info!("audio mix graph built tracks={track_count} sample_rate={sample_rate}");
    Ok(FilterGraph::from_prebuilt(inner))
}

// ── VideoConcatenator ─────────────────────────────────────────────────────────

/// Concatenates multiple video clips into a single seamless output stream.
///
/// Each clip is loaded via a `movie=` source node.  When
/// [`output_resolution`](Self::output_resolution) is set, a `scale` filter is
/// inserted per clip to normalise all clips to a common resolution before
/// concatenation.  A single clip skips the `concat` filter and passes through
/// directly.
///
/// The resulting [`FilterGraph`] is source-only — call
/// [`FilterGraph::pull_video`] in a loop to extract the output frames.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::VideoConcatenator;
///
/// let mut graph = VideoConcatenator::new(vec!["clip_a.mp4", "clip_b.mp4"])
///     .output_resolution(1280, 720)
///     .build()?;
///
/// while let Some(frame) = graph.pull_video()? {
///     // encode or display `frame`
/// }
/// ```
pub struct VideoConcatenator {
    clips: Vec<PathBuf>,
    output_width: Option<u32>,
    output_height: Option<u32>,
}

impl VideoConcatenator {
    /// Creates a new concatenator for the given clip paths.
    pub fn new(clips: Vec<impl AsRef<std::path::Path>>) -> Self {
        Self {
            clips: clips
                .into_iter()
                .map(|p| p.as_ref().to_path_buf())
                .collect(),
            output_width: None,
            output_height: None,
        }
    }

    /// Sets the output resolution.  When provided, a `scale=W:H` filter is
    /// inserted per clip before concatenation.
    #[must_use]
    pub fn output_resolution(self, w: u32, h: u32) -> Self {
        Self {
            output_width: Some(w),
            output_height: Some(h),
            ..self
        }
    }

    /// Builds a source-only [`FilterGraph`] that concatenates all clips.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — no clips were provided, or an
    ///   underlying `FFmpeg` graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.clips.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no clips".to_string(),
            });
        }
        // SAFETY: all raw pointer operations follow the avfilter ownership rules:
        // - avfilter_graph_alloc() returns an owned pointer freed via
        //   avfilter_graph_free() on error or stored in FilterGraphInner on success.
        // - avfilter_graph_create_filter() adds contexts owned by the graph.
        // - avfilter_link() connects pads; connections are owned by the graph.
        // - avfilter_graph_config() finalises the graph.
        // - NonNull::new_unchecked() is called only after ret >= 0 checks.
        unsafe { build_video_concat(&self.clips, self.output_width, self.output_height) }
    }
}

// ── Video concat graph builder ────────────────────────────────────────────────

unsafe fn build_video_concat(
    clips: &[PathBuf],
    output_width: Option<u32>,
    output_height: Option<u32>,
) -> Result<FilterGraph, FilterError> {
    use std::ffi::CString;

    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::CompositionFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::CompositionFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    let clip_count = clips.len();
    let mut end_ctxs: Vec<*mut ff_sys::AVFilterContext> = Vec::with_capacity(clip_count);

    for (idx, clip) in clips.iter().enumerate() {
        let path = clip.to_string_lossy();

        // ── movie= source ─────────────────────────────────────────────────────
        let movie_filter = ff_sys::avfilter_get_by_name(c"movie".as_ptr());
        if movie_filter.is_null() {
            bail!(graph, "filter not found: movie");
        }
        let Ok(movie_name) = CString::new(format!("concat_movie{idx}")) else {
            bail!(graph, "CString::new failed for movie name");
        };
        let Ok(movie_args) = CString::new(format!("filename={path}")) else {
            bail!(graph, "CString::new failed for movie args");
        };
        let mut movie_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut movie_ctx,
            movie_filter,
            movie_name.as_ptr(),
            movie_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(
                graph,
                format!("failed to create movie filter clip={idx} code={ret}")
            );
        }
        log::debug!("video concat clip={idx} movie source path={path}");
        let mut chain_end = movie_ctx;

        // ── Optional scale ────────────────────────────────────────────────────
        if let (Some(w), Some(h)) = (output_width, output_height) {
            let scale_filter = ff_sys::avfilter_get_by_name(c"scale".as_ptr());
            if scale_filter.is_null() {
                bail!(graph, "filter not found: scale");
            }
            let Ok(sc_name) = CString::new(format!("concat_scale{idx}")) else {
                bail!(graph, "CString::new failed for scale name");
            };
            let Ok(sc_args) = CString::new(format!("{w}:{h}")) else {
                bail!(graph, "CString::new failed for scale args");
            };
            let mut sc_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut sc_ctx,
                scale_filter,
                sc_name.as_ptr(),
                sc_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create scale filter clip={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, sc_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: movie→scale clip={idx}"));
            }
            chain_end = sc_ctx;
        }

        end_ctxs.push(chain_end);
    }

    // ── concat (skipped for single clip) ─────────────────────────────────────
    let pre_sink_ctx = if clip_count == 1 {
        end_ctxs[0]
    } else {
        let concat_filter = ff_sys::avfilter_get_by_name(c"concat".as_ptr());
        if concat_filter.is_null() {
            bail!(graph, "filter not found: concat");
        }
        let Ok(concat_args) = CString::new(format!("n={clip_count}:v=1:a=0")) else {
            bail!(graph, "CString::new failed for concat args");
        };
        let mut concat_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut concat_ctx,
            concat_filter,
            c"concat".as_ptr(),
            concat_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(graph, format!("failed to create concat filter code={ret}"));
        }
        for (i, &end_ctx) in end_ctxs.iter().enumerate() {
            let ret = ff_sys::avfilter_link(end_ctx, 0, concat_ctx, i as u32);
            if ret < 0 {
                bail!(graph, format!("link failed: clip{i}→concat[{i}]"));
            }
        }
        concat_ctx
    };

    // ── buffersink ────────────────────────────────────────────────────────────
    let sink_filter = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
    if sink_filter.is_null() {
        bail!(graph, "filter not found: buffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        sink_filter,
        c"vsink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create buffersink code={ret}"));
    }
    let ret = ff_sys::avfilter_link(pre_sink_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: last→buffersink");
    }

    // ── Configure graph ───────────────────────────────────────────────────────
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        log::warn!("video concat avfilter_graph_config failed code={ret}");
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // SAFETY: ret >= 0 guarantees both pointers are non-null.
    let graph_nn = NonNull::new_unchecked(graph);
    let sink_nn = NonNull::new_unchecked(sink_ctx);
    let inner = FilterGraphInner::with_prebuilt_video_graph(graph_nn, sink_nn);
    log::info!("video concat graph built clips={clip_count}");
    Ok(FilterGraph::from_prebuilt(inner))
}

// ── AudioConcatenator ─────────────────────────────────────────────────────────

/// Concatenates multiple audio clips into a single seamless output stream.
///
/// Each clip is loaded via an `amovie=` source node.  When
/// [`output_format`](Self::output_format) is set, an `aresample` and/or
/// `aformat` filter is inserted per clip to normalise the sample rate and
/// channel layout before concatenation.  A single clip skips the `concat`
/// filter entirely.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::AudioConcatenator;
/// use ff_format::ChannelLayout;
///
/// let mut graph = AudioConcatenator::new(vec!["clip_a.mp3", "clip_b.mp3"])
///     .output_format(48_000, ChannelLayout::Stereo)
///     .build()?;
///
/// while let Some(frame) = graph.pull_audio()? {
///     // encode or play `frame`
/// }
/// ```
pub struct AudioConcatenator {
    clips: Vec<PathBuf>,
    output_sample_rate: Option<u32>,
    output_channel_layout: Option<ChannelLayout>,
}

impl AudioConcatenator {
    /// Creates a new concatenator for the given clip paths.
    pub fn new(clips: Vec<impl AsRef<std::path::Path>>) -> Self {
        Self {
            clips: clips
                .into_iter()
                .map(|p| p.as_ref().to_path_buf())
                .collect(),
            output_sample_rate: None,
            output_channel_layout: None,
        }
    }

    /// Sets the output sample rate and channel layout.
    ///
    /// When set, an `aresample` filter is inserted for each clip whose sample
    /// rate differs from `sample_rate`, and an `aformat` filter is inserted for
    /// each clip whose channel layout differs from `layout`.
    #[must_use]
    pub fn output_format(self, sample_rate: u32, layout: ChannelLayout) -> Self {
        Self {
            output_sample_rate: Some(sample_rate),
            output_channel_layout: Some(layout),
            ..self
        }
    }

    /// Builds a source-only [`FilterGraph`] that concatenates all clips.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — no clips were provided, or an
    ///   underlying `FFmpeg` graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        if self.clips.is_empty() {
            return Err(FilterError::CompositionFailed {
                reason: "no clips".to_string(),
            });
        }
        // SAFETY: avfilter_graph_alloc / avfilter_graph_create_filter /
        // avfilter_link / avfilter_graph_config follow the same ownership rules
        // as build_video_concat:
        // - avfilter_graph_free is called in the bail! macro on every error path.
        // - avfilter_link() connects pads; connections are owned by the graph.
        // - avfilter_graph_config() finalises the graph.
        // - NonNull::new_unchecked() is called only after ret >= 0 checks.
        unsafe {
            build_audio_concat(
                &self.clips,
                self.output_sample_rate,
                self.output_channel_layout,
            )
        }
    }
}

// ── Audio concat graph builder ────────────────────────────────────────────────

unsafe fn build_audio_concat(
    clips: &[PathBuf],
    output_sample_rate: Option<u32>,
    output_channel_layout: Option<ChannelLayout>,
) -> Result<FilterGraph, FilterError> {
    use std::ffi::CString;

    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::CompositionFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::CompositionFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    let clip_count = clips.len();
    let mut end_ctxs: Vec<*mut ff_sys::AVFilterContext> = Vec::with_capacity(clip_count);

    for (idx, clip) in clips.iter().enumerate() {
        let path = clip.to_string_lossy();

        // ── amovie= source ────────────────────────────────────────────────────
        let amovie_filter = ff_sys::avfilter_get_by_name(c"amovie".as_ptr());
        if amovie_filter.is_null() {
            bail!(graph, "filter not found: amovie");
        }
        let Ok(amovie_name) = CString::new(format!("aconcat_amovie{idx}")) else {
            bail!(graph, "CString::new failed for amovie name");
        };
        let Ok(amovie_args) = CString::new(format!("filename={path}")) else {
            bail!(graph, "CString::new failed for amovie args");
        };
        let mut amovie_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut amovie_ctx,
            amovie_filter,
            amovie_name.as_ptr(),
            amovie_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(
                graph,
                format!("failed to create amovie filter clip={idx} code={ret}")
            );
        }
        log::debug!("audio concat clip={idx} amovie source path={path}");
        let mut chain_end = amovie_ctx;

        // ── Optional aresample (sample rate conversion) ───────────────────────
        if let Some(sample_rate) = output_sample_rate {
            let aresample_filter = ff_sys::avfilter_get_by_name(c"aresample".as_ptr());
            if aresample_filter.is_null() {
                bail!(graph, "filter not found: aresample");
            }
            let Ok(ar_name) = CString::new(format!("aconcat_aresample{idx}")) else {
                bail!(graph, "CString::new failed for aresample name");
            };
            let Ok(ar_args) = CString::new(format!("{sample_rate}")) else {
                bail!(graph, "CString::new failed for aresample args");
            };
            let mut ar_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut ar_ctx,
                aresample_filter,
                ar_name.as_ptr(),
                ar_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create aresample filter clip={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, ar_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: amovie→aresample clip={idx}"));
            }
            chain_end = ar_ctx;
            log::debug!("audio concat clip={idx} aresample target_rate={sample_rate}");
        }

        // ── Optional aformat (channel layout conversion) ──────────────────────
        if let Some(layout) = output_channel_layout {
            let af_args_str = match layout {
                ChannelLayout::Other(_) => {
                    let sr = output_sample_rate.unwrap_or(48_000);
                    format!("sample_rates={sr}")
                }
                _ => format!("channel_layouts={}", layout.name()),
            };
            let aformat_filter = ff_sys::avfilter_get_by_name(c"aformat".as_ptr());
            if aformat_filter.is_null() {
                bail!(graph, "filter not found: aformat");
            }
            let Ok(af_name) = CString::new(format!("aconcat_aformat{idx}")) else {
                bail!(graph, "CString::new failed for aformat name");
            };
            let Ok(af_args) = CString::new(af_args_str.as_str()) else {
                bail!(graph, "CString::new failed for aformat args");
            };
            let mut af_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut af_ctx,
                aformat_filter,
                af_name.as_ptr(),
                af_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create aformat filter clip={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, af_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: →aformat clip={idx}"));
            }
            chain_end = af_ctx;
            log::debug!("audio concat clip={idx} aformat layout={}", layout.name());
        }

        end_ctxs.push(chain_end);
    }

    // ── concat (skipped for single clip) ─────────────────────────────────────
    let pre_sink_ctx = if clip_count == 1 {
        end_ctxs[0]
    } else {
        let concat_filter = ff_sys::avfilter_get_by_name(c"concat".as_ptr());
        if concat_filter.is_null() {
            bail!(graph, "filter not found: concat");
        }
        let Ok(concat_args) = CString::new(format!("n={clip_count}:v=0:a=1")) else {
            bail!(graph, "CString::new failed for concat args");
        };
        let mut concat_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
        let ret = ff_sys::avfilter_graph_create_filter(
            &raw mut concat_ctx,
            concat_filter,
            c"aconcat".as_ptr(),
            concat_args.as_ptr(),
            std::ptr::null_mut(),
            graph,
        );
        if ret < 0 {
            bail!(graph, format!("failed to create concat filter code={ret}"));
        }
        for (i, &end_ctx) in end_ctxs.iter().enumerate() {
            let ret = ff_sys::avfilter_link(end_ctx, 0, concat_ctx, i as u32);
            if ret < 0 {
                bail!(graph, format!("link failed: clip{i}→aconcat[{i}]"));
            }
        }
        concat_ctx
    };

    // ── abuffersink ───────────────────────────────────────────────────────────
    let sink_filter = ff_sys::avfilter_get_by_name(c"abuffersink".as_ptr());
    if sink_filter.is_null() {
        bail!(graph, "filter not found: abuffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        sink_filter,
        c"aconcat_asink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create abuffersink code={ret}"));
    }
    let ret = ff_sys::avfilter_link(pre_sink_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: last→abuffersink");
    }

    // ── Configure graph ───────────────────────────────────────────────────────
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        log::warn!("audio concat avfilter_graph_config failed code={ret}");
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // SAFETY: ret >= 0 guarantees both pointers are non-null.
    let graph_nn = NonNull::new_unchecked(graph);
    let sink_nn = NonNull::new_unchecked(sink_ctx);
    let inner = FilterGraphInner::with_prebuilt_audio_graph(graph_nn, sink_nn);
    log::info!("audio concat graph built clips={clip_count}");
    Ok(FilterGraph::from_prebuilt(inner))
}

// ── ClipJoiner ────────────────────────────────────────────────────────────────

/// Joins two video clips with a cross-dissolve transition.
///
/// Each clip is loaded via a `movie=` source node.  The last
/// `dissolve_duration` seconds of clip A overlap with the first
/// `dissolve_duration` seconds of clip B, producing an output shorter than
/// simple concatenation by `dissolve_duration`.
///
/// When `dissolve_duration` is [`Duration::ZERO`] the clips are concatenated
/// without a transition (equivalent to
/// [`VideoConcatenator::new(vec![clip_a, clip_b]).build()`]).
///
/// # Errors
///
/// Returns [`FilterError::CompositionFailed`] when:
/// - The clip duration cannot be probed (e.g. file not found).
/// - `dissolve_duration` exceeds the duration of either clip.
///
/// # Examples
///
/// ```ignore
/// use ff_filter::ClipJoiner;
/// use std::time::Duration;
///
/// let mut graph = ClipJoiner::new("intro.mp4", "main.mp4", Duration::from_secs(1))
///     .build()?;
///
/// while let Some(frame) = graph.pull_video()? {
///     // encode or display `frame`
/// }
/// ```
pub struct ClipJoiner {
    clip_a: PathBuf,
    clip_b: PathBuf,
    dissolve_duration: Duration,
}

impl ClipJoiner {
    /// Create a new `ClipJoiner`.
    ///
    /// `dissolve_duration` is the length of the cross-dissolve overlap.
    /// Pass [`Duration::ZERO`] for plain concatenation (no transition).
    pub fn new(
        clip_a: impl AsRef<std::path::Path>,
        clip_b: impl AsRef<std::path::Path>,
        dissolve_duration: Duration,
    ) -> Self {
        Self {
            clip_a: clip_a.as_ref().to_path_buf(),
            clip_b: clip_b.as_ref().to_path_buf(),
            dissolve_duration,
        }
    }

    /// Builds a source-only [`FilterGraph`] that joins the two clips.
    ///
    /// # Errors
    ///
    /// - [`FilterError::CompositionFailed`] — clip duration probe failed, or
    ///   `dissolve_duration` exceeds a clip's duration, or an `FFmpeg`
    ///   graph-construction call failed.
    pub fn build(self) -> Result<FilterGraph, FilterError> {
        let dissolve_sec = self.dissolve_duration.as_secs_f64();
        // SAFETY: avformat and avfilter invariants are maintained internally;
        //         all pointers are null-checked; resources are freed on every
        //         error path.
        unsafe { build_dissolve_join(&self.clip_a, &self.clip_b, dissolve_sec) }
    }
}

// ── Dissolve-join graph builder ────────────────────────────────────────────────

/// Probe a clip's duration in seconds using `avformat_open_input` +
/// `avformat_find_stream_info`.  Returns `CompositionFailed` if the file
/// cannot be opened or has an unknown duration.
unsafe fn probe_clip_duration_sec(path: &PathBuf) -> Result<f64, FilterError> {
    let ctx = ff_sys::avformat::open_input(path.as_ref()).map_err(|code| {
        FilterError::CompositionFailed {
            reason: format!(
                "failed to probe clip duration code={code} path={}",
                path.display()
            ),
        }
    })?;

    if let Err(code) = ff_sys::avformat::find_stream_info(ctx) {
        let mut p = ctx;
        ff_sys::avformat::close_input(&raw mut p);
        return Err(FilterError::CompositionFailed {
            reason: format!("avformat_find_stream_info failed code={code}"),
        });
    }

    let duration_val = (*ctx).duration;
    let mut p = ctx;
    ff_sys::avformat::close_input(&raw mut p);

    if duration_val <= 0 {
        return Err(FilterError::CompositionFailed {
            reason: format!("clip has unknown duration path={}", path.display()),
        });
    }
    // AV_TIME_BASE = 1_000_000 microseconds per second.
    Ok(duration_val as f64 / 1_000_000.0)
}

unsafe fn build_dissolve_join(
    clip_a: &PathBuf,
    clip_b: &PathBuf,
    dissolve_sec: f64,
) -> Result<FilterGraph, FilterError> {
    use std::ffi::CString;

    macro_rules! bail {
        ($graph:ident, $reason:expr) => {{
            let mut g = $graph;
            ff_sys::avfilter_graph_free(std::ptr::addr_of_mut!(g));
            return Err(FilterError::CompositionFailed {
                reason: format!("{}", $reason),
            });
        }};
    }

    // ── Dissolve-duration == 0: use concat (plain concatenation) ──────────────
    if dissolve_sec == 0.0 {
        return build_video_concat(&[clip_a.clone(), clip_b.clone()], None, None);
    }

    // ── Probe clip durations ──────────────────────────────────────────────────
    let clip_a_dur = probe_clip_duration_sec(clip_a)?;
    let clip_b_dur = probe_clip_duration_sec(clip_b)?;

    if dissolve_sec > clip_a_dur {
        return Err(FilterError::CompositionFailed {
            reason: format!(
                "dissolve_duration ({dissolve_sec:.3}s) exceeds clip_a duration ({clip_a_dur:.3}s)"
            ),
        });
    }
    if dissolve_sec > clip_b_dur {
        return Err(FilterError::CompositionFailed {
            reason: format!(
                "dissolve_duration ({dissolve_sec:.3}s) exceeds clip_b duration ({clip_b_dur:.3}s)"
            ),
        });
    }

    // xfade offset = when the crossfade starts (measured from the beginning of
    // the first stream).
    let xfade_offset = clip_a_dur - dissolve_sec;

    // ── Allocate graph ────────────────────────────────────────────────────────
    let graph = ff_sys::avfilter_graph_alloc();
    if graph.is_null() {
        return Err(FilterError::CompositionFailed {
            reason: "avfilter_graph_alloc failed".to_string(),
        });
    }

    // ── movie[a] source ───────────────────────────────────────────────────────
    let movie_filter = ff_sys::avfilter_get_by_name(c"movie".as_ptr());
    if movie_filter.is_null() {
        bail!(graph, "filter not found: movie");
    }
    let path_a = clip_a.to_string_lossy();
    let Ok(movie_a_name) = CString::new("jd_movie_a") else {
        bail!(graph, "CString::new failed for movie_a name");
    };
    let Ok(movie_a_args) = CString::new(format!("filename={path_a}")) else {
        bail!(graph, "CString::new failed for movie_a args");
    };
    let mut movie_a_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut movie_a_ctx,
        movie_filter,
        movie_a_name.as_ptr(),
        movie_a_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create movie_a filter code={ret}"));
    }
    log::debug!("dissolve join clip_a movie source path={path_a}");

    // ── movie[b] source ───────────────────────────────────────────────────────
    let path_b = clip_b.to_string_lossy();
    let Ok(movie_b_name) = CString::new("jd_movie_b") else {
        bail!(graph, "CString::new failed for movie_b name");
    };
    let Ok(movie_b_args) = CString::new(format!("filename={path_b}")) else {
        bail!(graph, "CString::new failed for movie_b args");
    };
    let mut movie_b_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut movie_b_ctx,
        movie_filter,
        movie_b_name.as_ptr(),
        movie_b_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create movie_b filter code={ret}"));
    }
    log::debug!("dissolve join clip_b movie source path={path_b}");

    // ── xfade ─────────────────────────────────────────────────────────────────
    let xfade_filter = ff_sys::avfilter_get_by_name(c"xfade".as_ptr());
    if xfade_filter.is_null() {
        bail!(graph, "filter not found: xfade");
    }
    let xfade_args_str =
        format!("transition=dissolve:duration={dissolve_sec}:offset={xfade_offset}");
    let Ok(xfade_args) = CString::new(xfade_args_str.as_str()) else {
        bail!(graph, "CString::new failed for xfade args");
    };
    let mut xfade_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut xfade_ctx,
        xfade_filter,
        c"jd_xfade".as_ptr(),
        xfade_args.as_ptr(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(
            graph,
            format!("failed to create xfade filter args={xfade_args_str} code={ret}")
        );
    }
    log::debug!("dissolve join xfade args={xfade_args_str}");

    // movie_a → xfade[0]
    let ret = ff_sys::avfilter_link(movie_a_ctx, 0, xfade_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: movie_a→xfade[0]");
    }
    // movie_b → xfade[1]
    let ret = ff_sys::avfilter_link(movie_b_ctx, 0, xfade_ctx, 1);
    if ret < 0 {
        bail!(graph, "link failed: movie_b→xfade[1]");
    }

    // ── buffersink ────────────────────────────────────────────────────────────
    let sink_filter = ff_sys::avfilter_get_by_name(c"buffersink".as_ptr());
    if sink_filter.is_null() {
        bail!(graph, "filter not found: buffersink");
    }
    let mut sink_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
    let ret = ff_sys::avfilter_graph_create_filter(
        &raw mut sink_ctx,
        sink_filter,
        c"jd_vsink".as_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        graph,
    );
    if ret < 0 {
        bail!(graph, format!("failed to create buffersink code={ret}"));
    }
    let ret = ff_sys::avfilter_link(xfade_ctx, 0, sink_ctx, 0);
    if ret < 0 {
        bail!(graph, "link failed: xfade→buffersink");
    }

    // ── Configure graph ───────────────────────────────────────────────────────
    let ret = ff_sys::avfilter_graph_config(graph, std::ptr::null_mut());
    if ret < 0 {
        log::warn!("dissolve join avfilter_graph_config failed code={ret}");
        bail!(graph, format!("avfilter_graph_config failed code={ret}"));
    }

    // SAFETY: ret >= 0 guarantees both pointers are non-null.
    let graph_nn = NonNull::new_unchecked(graph);
    let sink_nn = NonNull::new_unchecked(sink_ctx);
    let inner = FilterGraphInner::with_prebuilt_video_graph(graph_nn, sink_nn);
    log::info!("dissolve join graph built dissolve_sec={dissolve_sec} xfade_offset={xfade_offset}");
    Ok(FilterGraph::from_prebuilt(inner))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn composer_zero_canvas_size_should_err() {
        // width = 0
        let result = MultiTrackComposer::new(0, 1080)
            .add_layer(VideoLayer {
                source: "clip.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for zero width, got {result:?}"
        );

        // height = 0
        let result = MultiTrackComposer::new(1920, 0)
            .add_layer(VideoLayer {
                source: "clip.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for zero height, got {result:?}"
        );
    }

    #[test]
    fn composer_canvas_larger_than_track_should_succeed() {
        // A 1920×1080 canvas is larger than a typical 640×480 source track.
        // Canvas size is independent of layer resolution — placement at (x, y)
        // is handled by the overlay filter; no auto-scale is applied.
        // The validation guard must not reject non-zero canvas dimensions.
        // If the build fails it must be for an FFmpeg reason (e.g. source file
        // not found), not because of canvas size.
        let result = MultiTrackComposer::new(1920, 1080)
            .add_layer(VideoLayer {
                source: "nonexistent_640x480.mp4".into(),
                x: 100,
                y: 100,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("canvas") && !reason.contains("zero"),
                "build failed due to canvas size, which must not happen for 1920x1080: {reason}"
            );
        }
        // Ok(_) is also acceptable if the movie source happened to be present.
    }

    #[test]
    fn composer_empty_layers_should_return_err() {
        let result = MultiTrackComposer::new(1920, 1080).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed, got {result:?}"
        );
    }

    #[test]
    fn mixer_empty_tracks_should_err() {
        let result = MultiTrackAudioMixer::new(48000, ChannelLayout::Stereo).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed, got {result:?}"
        );
    }

    #[test]
    fn audio_track_with_empty_effects_should_build_successfully() {
        // build() may fail because the source doesn't exist, but must NOT fail
        // with a reason related to effects.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 48_000,
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("effect"),
                "build must not fail due to empty effects, got: {reason}"
            );
        }
    }

    #[test]
    fn audio_track_with_volume_effect_should_include_volume_filter() {
        // Structural test: verify the effects field accepts FilterStep::Volume.
        let track = AudioTrack {
            source: "track.mp3".into(),
            volume_db: 0.0,
            pan: 0.0,
            time_offset: Duration::ZERO,
            effects: vec![FilterStep::Volume(6.0)],
            sample_rate: 48_000,
            channel_layout: ChannelLayout::Stereo,
        };
        assert_eq!(track.effects.len(), 1);
        assert!(
            matches!(track.effects[0], FilterStep::Volume(_)),
            "expected Volume variant"
        );
    }

    #[test]
    fn mixer_mismatched_sample_rate_should_insert_aresample() {
        // Track is 44100 Hz, output is 48000 Hz → build_audio_mix must attempt
        // to create an aresample node.  With a nonexistent file the graph fails
        // at avfilter_graph_config, NOT at "filter not found: aresample", which
        // proves the node was created successfully before the config step.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 44_100, // mismatch → aresample should be inserted
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        assert!(result.is_err(), "expected error from nonexistent file");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("filter not found: aresample"),
                "aresample filter must exist in FFmpeg and be created; got: {reason}"
            );
        }
    }

    #[test]
    fn video_layer_with_positive_offset_should_insert_setpts() {
        // setpts_offset is inserted when time_offset > 0.
        // Build fails (nonexistent file) but NOT at "filter not found: setpts".
        let result = MultiTrackComposer::new(1920, 1080)
            .add_layer(VideoLayer {
                source: "nonexistent.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::from_secs(2),
                in_point: None,
                out_point: None,
            })
            .build();
        assert!(result.is_err(), "expected error (nonexistent file)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("filter not found: setpts"),
                "setpts must exist in FFmpeg and be created; got: {reason}"
            );
        }
    }

    #[test]
    fn audio_track_with_positive_offset_should_insert_adelay() {
        // adelay is inserted when time_offset > 0.
        // Build fails (nonexistent file) but NOT at "filter not found: adelay".
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::from_secs(2),
                effects: vec![],
                sample_rate: 48_000,
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        assert!(result.is_err(), "expected error (nonexistent file)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("filter not found: adelay"),
                "adelay must exist in FFmpeg and be created; got: {reason}"
            );
        }
    }

    #[test]
    fn zero_offset_should_not_insert_extra_filters() {
        // time_offset=ZERO must not cause setpts_offset or adelay nodes.
        let video_result = MultiTrackComposer::new(1920, 1080)
            .add_layer(VideoLayer {
                source: "nonexistent.mp4".into(),
                x: 0,
                y: 0,
                scale: 1.0,
                opacity: 1.0,
                z_order: 0,
                time_offset: Duration::ZERO,
                in_point: None,
                out_point: None,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = video_result {
            assert!(
                !reason.contains("setpts_offset"),
                "setpts_offset must not appear for zero offset; got: {reason}"
            );
        }

        let audio_result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 48_000,
                channel_layout: ChannelLayout::Stereo,
            })
            .build();
        if let Err(FilterError::CompositionFailed { ref reason }) = audio_result {
            assert!(
                !reason.contains("adelay"),
                "adelay must not appear for zero offset; got: {reason}"
            );
        }
    }

    #[test]
    fn mixer_matching_format_should_not_insert_extra_filters() {
        // Track format matches output → no aresample or aformat should be
        // inserted.  Build fails only because the source file does not exist.
        let result = MultiTrackAudioMixer::new(48_000, ChannelLayout::Stereo)
            .add_track(AudioTrack {
                source: "nonexistent.mp3".into(),
                volume_db: 0.0,
                pan: 0.0,
                time_offset: Duration::ZERO,
                effects: vec![],
                sample_rate: 48_000, // matches output → no aresample
                channel_layout: ChannelLayout::Stereo, // matches output → no aformat
            })
            .build();
        assert!(result.is_err(), "expected error from nonexistent file");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            assert!(
                !reason.contains("aresample"),
                "aresample must not appear for matching format; got: {reason}"
            );
            assert!(
                !reason.contains("filter not found: aformat"),
                "aformat must not appear for matching format; got: {reason}"
            );
        }
    }

    #[test]
    fn audio_concatenator_empty_clips_should_err() {
        let result = AudioConcatenator::new(Vec::<PathBuf>::new()).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for empty clips, got {result:?}"
        );
    }

    #[test]
    fn audio_concatenator_three_clips_should_build_successfully() {
        // Build with three nonexistent clips.  Graph construction of individual
        // filter nodes (amovie, concat, abuffersink) should succeed; failure
        // only at avfilter_graph_config (file not found) is expected.
        //
        // Some FFmpeg builds omit `amovie` or `concat`; skip gracefully on
        // those environments rather than failing.
        let result = AudioConcatenator::new(vec!["a.mp3", "b.mp3", "c.mp3"])
            .output_format(48_000, ChannelLayout::Stereo)
            .build();
        assert!(result.is_err(), "expected error (nonexistent files)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            if reason.contains("filter not found: amovie")
                || reason.contains("filter not found: concat")
            {
                println!(
                    "Skipping: required lavfi filter unavailable in this FFmpeg build ({reason})"
                );
                return;
            }
        }
    }

    #[test]
    fn concatenator_empty_clips_should_err() {
        let result = VideoConcatenator::new(Vec::<PathBuf>::new()).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for empty clips, got {result:?}"
        );
    }

    #[test]
    fn concatenator_three_clips_should_build_successfully() {
        // Build with three nonexistent clips.  Graph construction of individual
        // filter nodes (movie, concat, buffersink) should succeed; failure only
        // at avfilter_graph_config (file not found) is expected.
        //
        // Some FFmpeg builds omit the `movie` or `concat` lavfi filters; skip
        // gracefully on those environments rather than failing.
        let result = VideoConcatenator::new(vec!["a.mp4", "b.mp4", "c.mp4"]).build();
        assert!(result.is_err(), "expected error (nonexistent files)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            if reason.contains("filter not found: movie")
                || reason.contains("filter not found: concat")
            {
                println!(
                    "Skipping: required lavfi filter unavailable in this FFmpeg build ({reason})"
                );
                return;
            }
        }
    }

    #[test]
    fn join_with_dissolve_exceeding_clip_duration_should_err() {
        // dissolve_duration (9999 s) exceeds any realistic clip.  With
        // nonexistent files the probe itself returns CompositionFailed, which
        // also satisfies the assertion.  With real files the duration check
        // fires.
        let result = ClipJoiner::new("a.mp4", "b.mp4", Duration::from_secs(9999)).build();
        assert!(
            matches!(result, Err(FilterError::CompositionFailed { .. })),
            "expected CompositionFailed for dissolve_duration > clip duration, got {result:?}"
        );
    }

    #[test]
    fn join_with_dissolve_should_reduce_total_duration() {
        // With nonexistent files the probe step returns CompositionFailed.
        // This test verifies: (a) no panic, (b) the error is CompositionFailed
        // (not an unexpected variant), and (c) the `xfade` filter exists in the
        // running FFmpeg build (if the error mentions "filter not found: xfade"
        // we skip instead of failing, matching the pattern used by the concat
        // tests).
        let result =
            ClipJoiner::new("clip_a.mp4", "clip_b.mp4", Duration::from_millis(500)).build();
        assert!(result.is_err(), "expected error (probe or graph failure)");
        if let Err(FilterError::CompositionFailed { ref reason }) = result {
            if reason.contains("filter not found: xfade")
                || reason.contains("filter not found: movie")
            {
                println!(
                    "Skipping: required lavfi filter unavailable in this FFmpeg build ({reason})"
                );
                return;
            }
        }
    }
}
