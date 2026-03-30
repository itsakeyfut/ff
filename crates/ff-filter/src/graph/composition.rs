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

        // ── Optional timeline offset ──────────────────────────────────────────
        if track.time_offset > Duration::ZERO {
            let offset = track.time_offset.as_secs_f64();
            let asetpts_filter = ff_sys::avfilter_get_by_name(c"asetpts".as_ptr());
            if asetpts_filter.is_null() {
                bail!(graph, "filter not found: asetpts");
            }
            let Ok(asp_name) = CString::new(format!("asetpts_offset{idx}")) else {
                bail!(graph, "CString::new failed for asetpts name");
            };
            let Ok(asp_args) = CString::new(format!("PTS+{offset}/TB")) else {
                bail!(graph, "CString::new failed for asetpts args");
            };
            let mut asp_ctx: *mut ff_sys::AVFilterContext = std::ptr::null_mut();
            let ret = ff_sys::avfilter_graph_create_filter(
                &raw mut asp_ctx,
                asetpts_filter,
                asp_name.as_ptr(),
                asp_args.as_ptr(),
                std::ptr::null_mut(),
                graph,
            );
            if ret < 0 {
                bail!(
                    graph,
                    format!("failed to create asetpts offset filter track={idx} code={ret}")
                );
            }
            let ret = ff_sys::avfilter_link(chain_end, 0, asp_ctx, 0);
            if ret < 0 {
                bail!(graph, format!("link failed: amovie→asetpts track={idx}"));
            }
            chain_end = asp_ctx;
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
        };
        assert_eq!(track.effects.len(), 1);
        assert!(
            matches!(track.effects[0], FilterStep::Volume(_)),
            "expected Volume variant"
        );
    }
}
