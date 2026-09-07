//! Shared GPU compositing core for the bridge (#1626 preview / #1627 export).
//!
//! Owns the `ff-render` context and a cached `Compositor`, and composites a set of
//! derived layers (any [`GpuLayerSource`]) with their decoded frames into an rgba
//! buffer. Both the preview executor ([`GpuPreviewCompositor`](crate::GpuPreviewCompositor))
//! and the export drain use it, so the mapping-to-GPU logic, the layer placement, and
//! the effect execution live in one place.
//!
//! It returns `None` on any unsupported layer ([`map_scene`] fallback, or a placement
//! with no GPU equivalent) or any GPU error, so the caller falls back to the CPU
//! compositor for that frame -- never a panic, never a partial result.
//!
//! **Placement (ADR-0016):** `layer_transform` places every layer, the base included, in
//! **canvas space**: its top-left at `(x, y)` canvas pixels, at its native size when
//! `scale == 1` and at `canvas * scale` otherwise, clipped at the canvas edge. That is the
//! CPU export composer's construction, which the realtime composer now shares, so the
//! rule is pinned by measurement against both. Nothing is fitted or stretched to the
//! canvas here: framing against the canvas is a `FitMode` effect in the layer's chain.
//! The model's units are canvas pixels / clockwise degrees while
//! `ff_render::LayerTransform` is UV-space / counter-clockwise radians, so the conversion
//! lives there. A **rotated** layer still falls back: the CPU's `rotate` fills the
//! corners it exposes with `fillcolor` while the GPU transform leaves them transparent,
//! so there is nothing to map it to (RK-020).
//!
//! **Per-frame cost (#1634):** an effected layer's `RenderGraph` is cached per layer
//! position ([`CachedEffectGraph`]) and reused while its effect list compares equal, so
//! its node pipelines are compiled once instead of every frame; a changed effect list
//! (or layer count) rebuilds. Every effect is cacheable since the mask nodes stopped
//! baking a mask buffer (#1710), and
//! [`composite_owned`](GpuCompositor::composite_owned) moves owned frames so the
//! no-effects export path avoids a `VideoFrame::clone`.
//!
//! **Stateful effects (#1653):** a [`GpuEffect::MotionBlur`] node accumulates a trail
//! across a clip's frames, so its cross-frame reuse *is* the accumulation. A caller
//! that composites a sequence of clips at one layer position must call
//! [`reset_effect_cache`](GpuCompositor::reset_effect_cache) at each clip boundary so
//! the trail does not bleed across a cut (RK-025). The export drain does this per
//! clip, and the preview runner does it at each cut and on every seek (#1705).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ff_format::VideoFrame;
use ff_render::{
    ChromaKeyNode, ColorGradeNode, ColorWheelsNode, Compositor, CurvesNode, DipToColorNode,
    DissolveTransitionNode, FadeTransitionNode, FilmGrainNode, FrameLayer, GaussianBlurNode,
    GlowNode, HslNode, LayerTransform, LumaMaskNode, LutNode, MotionBlurNode, NodeParam,
    RenderContext, RenderGraph, ScaleNode, ShapeMaskNode, SharpenNode, VignetteNode,
    WipeTransitionNode,
};

use crate::gpu::{GpuEffect, GpuLayerPlan, GpuLayerSource, GpuMapping, map_scene};
use crate::gpu_transition::GpuTransition;

/// A per-layer effect [`RenderGraph`] cached across frames so an effected layer does
/// not recompile its node pipelines every frame (#1634).
///
/// Reused when the next frame's effects match the cached ones **or differ only in a
/// parameter a live node can take** (see [`param_update`]), and the input dimensions
/// are unchanged — a node sized to the frame (e.g. `OverlayNode`) or the read-back size
/// would otherwise be stale.
///
/// `effects` is what the graph's nodes currently hold, not what they were built with:
/// a parameter pushed into a live node updates it here too, so the next comparison is
/// against reality.
struct CachedEffectGraph {
    effects: Vec<GpuEffect>,
    graph: RenderGraph,
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
}

/// How a cached graph may be reused for the next frame's effect list.
enum Reuse {
    /// The lists are identical; run the graph as it stands.
    AsIs,
    /// They differ only in parameters live nodes take; apply these first.
    WithParams(Vec<NodeParam>),
}

/// Whether `next` can run on a graph built for `cached`, and with what updates.
///
/// The comparison is *explicit* about which parameters may differ rather than loose:
/// anything a node bakes in at build time (a mask sized to the frame, a LUT, the
/// sub-frame count) must force a rebuild, or the graph would be reused with a stale
/// bake (RK-025). Only parameters with a [`NodeParam`] — which a node applies to
/// itself, keeping whatever state it carries — are allowed to differ.
fn param_update(cached: &[GpuEffect], next: &[GpuEffect]) -> Option<Reuse> {
    if cached.len() != next.len() {
        return None;
    }
    let mut params = Vec::new();
    for (was, now) in cached.iter().zip(next) {
        match (was, now) {
            (
                GpuEffect::MotionBlur {
                    shutter_angle: old,
                    sub_frames: old_sub,
                },
                GpuEffect::MotionBlur {
                    shutter_angle: new,
                    sub_frames: new_sub,
                },
            ) if old_sub == new_sub => {
                // The trail lives in the node, so the shutter travels to it rather
                // than the node being rebuilt around the new value (#1705).
                //
                // Compared bit-exactly rather than against a tolerance: the cached
                // list is meant to be what the nodes *hold*, and skipping a
                // below-tolerance change would record a value that was never applied.
                if old.to_bits() != new.to_bits() {
                    params.push(NodeParam::MotionBlurShutter(*new));
                }
            }
            (
                GpuEffect::ShapeMask {
                    x,
                    y,
                    width,
                    height,
                    invert,
                },
                GpuEffect::ShapeMask {
                    x: nx,
                    y: ny,
                    width: nw,
                    height: nh,
                    invert: ninv,
                },
            ) => {
                // The shader evaluates the rectangle, so every field of it is a
                // parameter rather than something baked into the node at build time.
                if (x, y, width, height, invert) != (nx, ny, nw, nh, ninv) {
                    params.push(NodeParam::ShapeMaskRect {
                        x: *nx,
                        y: *ny,
                        width: *nw,
                        height: *nh,
                        invert: *ninv,
                    });
                }
            }
            _ if was == now => {}
            _ => return None,
        }
    }
    if params.is_empty() {
        Some(Reuse::AsIs)
    } else {
        Some(Reuse::WithParams(params))
    }
}

/// Composites derived layers on the GPU, returning `None` (CPU fallback) on
/// unsupported content or any GPU error.
pub struct GpuCompositor {
    ctx: Arc<RenderContext>,
    /// Compositor cached for its target canvas; rebuilt when the canvas changes.
    compositor: Option<(Compositor, (u32, u32))>,
    /// Per-layer effect-graph cache, keyed by `(layer count, layer position)`.
    ///
    /// Being a map at all is what fixes #1770: this was a `Vec` resized per
    /// composite, so any change in the layer count cleared every entry — twice per
    /// output frame on the multi-track export path, which alternates a one-layer solo
    /// composite with an N-layer stack.
    ///
    /// The layer count is in the key because a position means something different
    /// under a different stack, and a *stateful* node's state would otherwise be
    /// shared between two layers that merely happen to sit at the same index with the
    /// same effects. No arrangement in the tree exhibits that today (the export
    /// path's solo composite and stack pass do not collide, and preview composites a
    /// stable stack), and mutation injection confirms no test detects its removal.
    /// It is kept because the cost is a tuple and the failure it prevents is a silent
    /// one, not because a test demands it.
    effect_cache: HashMap<(usize, usize), CachedEffectGraph>,
}

impl GpuCompositor {
    /// Initialises a GPU context (best available adapter). Returns `None` when no
    /// adapter is available, so the caller keeps the CPU path.
    #[must_use]
    pub fn new() -> Option<Self> {
        match RenderContext::init_blocking() {
            Ok(ctx) => Some(Self {
                ctx: Arc::new(ctx),
                compositor: None,
                effect_cache: HashMap::new(),
            }),
            Err(e) => {
                // Info, not debug: the GPU->CPU fallback reason must stay visible at
                // the default log level (per docs/rules/logging.md), since callers
                // only log `path=cpu` without a reason.
                log::info!("gpu context unavailable reason={e}");
                None
            }
        }
    }

    /// Composites `layers` (bottom to top, each paired with its decoded frame) at
    /// time `t` into a single rgba buffer `(rgba, width, height)`, or `None` to fall
    /// back to the CPU compositor.
    ///
    /// `None` means: `map_scene` reported an unsupported blend/composite/effect, a
    /// layer's placement has no GPU equivalent (a rotated overlay, or one hanging off
    /// the base layer's edge), or a GPU error occurred. The caller must never see a
    /// wrong-but-rendered frame.
    pub fn composite<L: GpuLayerSource>(
        &mut self,
        layers: &[(&L, &VideoFrame)],
        canvas: (u32, u32),
        t: Duration,
    ) -> Option<(Vec<u8>, u32, u32)> {
        let refs: Vec<&L> = layers.iter().map(|(l, _)| *l).collect();
        let plan = match map_scene(&refs, canvas, t) {
            GpuMapping::Gpu(plan) => plan,
            GpuMapping::Fallback(_) => return None,
        };

        // Part of the cache key: a layer position means something different under a
        // different stack, so a graph is only reused within the same layout.
        let count = plan.layers.len();
        let mut processed = Vec::with_capacity(count);
        for (idx, (lp, (_, frame))) in plan.layers.iter().zip(layers.iter()).enumerate() {
            // The preview adapter does not own its frames, so a no-effects layer must
            // clone; `composite_owned` avoids this for the export drain.
            let out = if lp.effects.is_empty() {
                (*frame).clone()
            } else {
                self.apply_effects(count, idx, lp, frame)?
            };
            processed.push((out, lp));
        }

        self.finish(assemble(processed, canvas)?, canvas)
    }

    /// Like [`composite`](Self::composite) but takes ownership of the layer frames, so a
    /// no-effects layer moves its frame into the compositor instead of cloning it (the
    /// export drain owns each freshly-decoded frame; #1634).
    pub fn composite_owned<L: GpuLayerSource>(
        &mut self,
        layers: Vec<(&L, VideoFrame)>,
        canvas: (u32, u32),
        t: Duration,
    ) -> Option<(Vec<u8>, u32, u32)> {
        let refs: Vec<&L> = layers.iter().map(|(l, _)| *l).collect();
        let plan = match map_scene(&refs, canvas, t) {
            GpuMapping::Gpu(plan) => plan,
            GpuMapping::Fallback(_) => return None,
        };

        // Part of the cache key: a layer position means something different under a
        // different stack, so a graph is only reused within the same layout.
        let count = plan.layers.len();
        let mut processed = Vec::with_capacity(count);
        for (idx, (lp, (_, frame))) in plan.layers.iter().zip(layers).enumerate() {
            // Owned: a no-effects layer moves its frame in, no clone.
            let out = if lp.effects.is_empty() {
                frame
            } else {
                self.apply_effects(count, idx, lp, &frame)?
            };
            processed.push((out, lp));
        }

        self.finish(assemble(processed, canvas)?, canvas)
    }

    /// Runs `transition` over the composited canvas frames `a` and `b` at `progress`
    /// (`0` = all `a`, `1` = all `b`), returning the blended rgba or `None` on a GPU
    /// error.
    ///
    /// Both buffers are already-composited `w` x `h` canvases, so the transition sits
    /// *after* compositing -- matching the CPU route, where `xfade` is the trailing step
    /// of the incoming layer's chain (`composition_inner.rs`).
    ///
    /// Every node here reproduces `FFmpeg`'s own formula for its kind (#1732), which is
    /// what lets the export use them at all: the export replaces `FFmpeg`'s `xfade`, so a
    /// node that merely looks similar would ship a different picture than the CPU route.
    /// `gpu_export`'s `export_maps_to_gpu` decides which kinds are allowed through.
    ///
    /// Each call builds a node, which compiles its pipeline in a per-instance `OnceLock`
    /// -- accepted for v1 since a transition is `duration x fps` frames of an offline
    /// export (#1659).
    pub(crate) fn transition(
        &mut self,
        transition: GpuTransition,
        progress: f32,
        a: &[u8],
        b: Vec<u8>,
        w: u32,
        h: u32,
    ) -> Option<Vec<u8>> {
        let graph = RenderGraph::new(Arc::clone(&self.ctx));
        let graph = match transition {
            GpuTransition::Fade => graph.push(FadeTransitionNode::new(progress, b, w, h)),
            // The mask is built here rather than in the shader: `FFmpeg`'s dissolve noise
            // outgrows `f32` well before 1080p, so a `WGSL` copy would reveal a different
            // set of pixels than the CPU reference (`ff_filter::xfade_frand`).
            GpuTransition::Dissolve => graph.push(DissolveTransitionNode::new(
                ff_filter::dissolve_mask(w, h, progress),
                b,
                w,
                h,
            )),
            // Zero softness: `FFmpeg`'s wipes have a hard edge, and that is also what
            // switches the node onto its integer-column rule.
            GpuTransition::Wipe { angle } => {
                graph.push(WipeTransitionNode::new(progress, 0.0, angle, b, w, h))
            }
            GpuTransition::Dip { color } => {
                graph.push(DipToColorNode::new(progress, color, b, w, h))
            }
        };
        graph.process_gpu(a, w, h).ok()
    }

    /// Composites the built `frame_layers` on the (canvas-cached) `Compositor` to rgba.
    fn finish(
        &mut self,
        mut frame_layers: Vec<FrameLayer>,
        canvas: (u32, u32),
    ) -> Option<(Vec<u8>, u32, u32)> {
        let rebuild = match &self.compositor {
            Some((_, cached)) => *cached != canvas,
            None => true,
        };
        if rebuild {
            self.compositor = Some((
                Compositor::new(self.ctx.clone(), canvas.0, canvas.1),
                canvas,
            ));
        }
        let (compositor, _) = self.compositor.as_mut()?;
        compositor.composite_to_rgba(&mut frame_layers).ok()
    }

    /// Drops every cached effect graph, so the next composite rebuilds each layer's
    /// graph from scratch.
    ///
    /// A stateful effect node (e.g. [`MotionBlurNode`], whose exposure trail
    /// accumulates across the frames of one clip) is embedded in the cached graph, so a
    /// caller that composites a sequence of clips at the same layer position must call
    /// this **at each clip boundary** — otherwise the previous clip's accumulated trail
    /// bleeds into the next clip's first frame (RK-025). Stateless effects are
    /// unaffected beyond a one-frame pipeline rebuild.
    pub fn reset_effect_cache(&mut self) {
        self.effect_cache.clear();
    }

    /// Applies a layer's mappable effects to its rgba frame via a `RenderGraph`, reusing
    /// the layer's cached graph when the effect list is unchanged and cacheable. `None`
    /// on a GPU error. Must not be called for an empty effect list (the caller handles
    /// that).
    fn apply_effects(
        &mut self,
        layer_count: usize,
        layer_idx: usize,
        plan: &GpuLayerPlan,
        frame: &VideoFrame,
    ) -> Option<VideoFrame> {
        let (in_w, in_h) = (frame.width(), frame.height());
        let rgba = frame.to_rgba()?;
        let key = (layer_count, layer_idx);

        // Reuse the cached graph when the input dimensions match (a node sized to the
        // frame or the read-back size would otherwise be stale) and the new effect
        // list either equals the cached one or differs only in parameters a live node
        // takes (see `param_update`).
        if let Some(cached) = self.effect_cache.get_mut(&key)
            && (cached.in_w, cached.in_h) == (in_w, in_h)
            && let Some(reuse) = param_update(&cached.effects, &plan.effects)
        {
            let applied = match reuse {
                Reuse::AsIs => true,
                Reuse::WithParams(params) => params
                    .into_iter()
                    .all(|param| cached.graph.set_param(param) > 0),
            };
            if applied {
                // The nodes now hold the new values, so record them: the next frame
                // has to be compared against what is in the graph, not against what
                // it was built with.
                cached.effects.clone_from(&plan.effects);
                let out = cached.graph.process_gpu(&rgba, in_w, in_h).ok()?;
                return VideoFrame::from_rgba(cached.out_w, cached.out_h, out).ok();
            }
            // A node declined a parameter `param_update` expected it to take. That
            // means the two are out of step, so rebuild rather than run a graph whose
            // contents are not what the comparison assumed.
        }

        let mut graph = RenderGraph::new(self.ctx.clone());
        // A `Scale` node resizes the frame; track the output dimensions so the
        // read-back buffer is wrapped at the right size.
        let (mut out_w, mut out_h) = (in_w, in_h);
        for effect in &plan.effects {
            graph = match effect {
                GpuEffect::ColorGrade {
                    brightness,
                    contrast,
                    saturation,
                    temperature,
                    tint,
                } => graph.push(ColorGradeNode::new(
                    *brightness,
                    *contrast,
                    *saturation,
                    *temperature,
                    *tint,
                )),
                GpuEffect::Scale {
                    width,
                    height,
                    algorithm,
                } => {
                    let node = ScaleNode::new(*width, *height, *algorithm);
                    // Ask the node what it will actually produce rather than
                    // assuming `width` x `height`: `target_size` passes the input
                    // size through when either dimension is `0`, matching FFmpeg's
                    // `scale=0:0`. Recording the literal `0` instead would wrap the
                    // read-back at the wrong size, `VideoFrame::from_rgba` would
                    // reject the length, and the whole frame would fall back to the
                    // CPU for no reason. `ScaleAnimated` reaches zero on a zoom that
                    // starts from nothing, so this is not hypothetical (#1630).
                    (out_w, out_h) = node.target_size(out_w, out_h);
                    graph.push(node)
                }
                // Blur preserves the frame dimensions, so out_w/out_h are unchanged.
                GpuEffect::Blur { sigma } => graph.push(GaussianBlurNode::new(*sigma)),
                // Sharpen preserves the frame dimensions too.
                GpuEffect::Sharpen { radius, strength } => {
                    graph.push(SharpenNode::new(*radius, *strength))
                }
                // Vignette preserves the frame dimensions too.
                GpuEffect::Vignette {
                    radius,
                    strength,
                    feather,
                } => graph.push(VignetteNode::new(*radius, *strength, *feather)),
                // FilmGrain preserves the frame dimensions too.
                GpuEffect::FilmGrain {
                    luma_strength,
                    chroma_strength,
                    frame_index,
                } => graph.push(FilmGrainNode::new(
                    *luma_strength,
                    *chroma_strength,
                    *frame_index,
                )),
                // Glow preserves the frame dimensions too.
                GpuEffect::Glow {
                    threshold,
                    radius,
                    intensity,
                } => graph.push(GlowNode::new(*threshold, *radius, *intensity)),
                // ColorWheels preserves the frame dimensions too.
                GpuEffect::ColorWheels {
                    shadows_lift,
                    midtones_gamma,
                    highlights_gain,
                } => graph.push(ColorWheelsNode::new(
                    *shadows_lift,
                    *midtones_gamma,
                    *highlights_gain,
                )),
                // Curves preserves the frame dimensions too.
                GpuEffect::Curves {
                    master,
                    red,
                    green,
                    blue,
                } => graph.push(CurvesNode::new(
                    master.clone(),
                    red.clone(),
                    green.clone(),
                    blue.clone(),
                )),
                // Hsl preserves the frame dimensions too.
                GpuEffect::Hsl {
                    hue_shift,
                    saturation,
                    lightness,
                } => graph.push(HslNode::new(*hue_shift, *saturation, *lightness)),
                // Lut preserves the frame dimensions too. A file the LutNode cannot
                // load (missing, malformed, or an unsupported extension) makes the
                // whole frame fall back to CPU rather than render wrong output (RK-020).
                GpuEffect::Lut { path } => graph.push(load_lut(path)?),
                // ChromaKey preserves the frame dimensions (it only rewrites alpha).
                GpuEffect::ChromaKey {
                    key_color,
                    tolerance,
                    softness,
                } => graph.push(ChromaKeyNode::new(*key_color, *tolerance, *softness)),
                // LumaMask multiplies alpha by the frame's own BT.709 luma. The node
                // samples the source frame itself (#1710), so nothing is built here
                // and nothing is uploaded per frame. When LumaMask follows another
                // effect the GPU mask is still the source luma while the CPU `geq`
                // sees the chained frame (a v1 limitation; parity uses it alone).
                GpuEffect::LumaMask { invert } => graph.push(LumaMaskNode::new(*invert)),
                // ShapeMask keeps a rectangle of the source frame. The rectangle is a
                // shader parameter rather than a baked full-frame mask (#1710).
                GpuEffect::ShapeMask {
                    x,
                    y,
                    width,
                    height,
                    invert,
                } => graph.push(ShapeMaskNode::new(*x, *y, *width, *height, *invert)),
                // MotionBlur is stateful (the trail accumulates across frames on this
                // node), so it depends on the cached graph being *reused* across a
                // clip's frames. It stays cacheable; the accumulation is reset at a
                // clip boundary via `reset_effect_cache` so a trail never bleeds across
                // a cut (RK-025). Preserves the frame dimensions.
                GpuEffect::MotionBlur {
                    shutter_angle,
                    sub_frames,
                } => graph.push(MotionBlurNode::new(*shutter_angle, *sub_frames)),
            };
        }

        // Store the built graph, then run it (`process_gpu` borrows, does not consume),
        // so the next frame reuses it.
        //
        // Every effect is cacheable. There used to be an exclusion for `LumaMask`,
        // because its node baked the source frame's own pixels into a mask and
        // reusing the graph would have applied a stale one (RK-025). The node now
        // samples the source frame per frame instead of baking anything (#1710), so
        // there is nothing left that a reused graph could hold stale. A new node that
        // *does* bake frame content would need that exclusion back.
        let cached = self
            .effect_cache
            .entry(key)
            .insert_entry(CachedEffectGraph {
                effects: plan.effects.clone(),
                graph,
                in_w,
                in_h,
                out_w,
                out_h,
            });
        let out = cached.get().graph.process_gpu(&rgba, in_w, in_h).ok()?;
        VideoFrame::from_rgba(out_w, out_h, out).ok()
    }
}

/// Wraps each processed layer frame in a [`FrameLayer`] with the transform that places
/// it, or `None` when a layer's placement has no GPU equivalent.
///
/// Every layer, the base included, is placed in canvas space (ADR-0016), the
/// construction the CPU export composer has always used and the realtime composer now
/// shares. Nothing is fitted or stretched to the canvas here.
fn assemble(
    processed: Vec<(VideoFrame, &GpuLayerPlan)>,
    canvas: (u32, u32),
) -> Option<Vec<FrameLayer>> {
    let transforms = processed
        .iter()
        .map(|(frame, lp)| layer_transform(lp, (frame.width(), frame.height()), canvas))
        .collect::<Option<Vec<_>>>()?;
    Some(
        processed
            .into_iter()
            .zip(transforms)
            .map(|((frame, lp), transform)| FrameLayer {
                frame,
                transform,
                blend_mode: lp.blend_mode,
                composite_op: lp.composite_op,
                opacity: lp.opacity,
                z_order: lp.z_order,
            })
            .collect(),
    )
}

/// The [`LayerTransform`] that places a layer whose frame is `frame` pixels on `canvas`,
/// or `None` when its placement has no GPU equivalent.
///
/// The rule is the CPU export composer's (`build_video_composition` in
/// `composition_inner.rs`), **measured** rather than read off the model:
///
/// * the layer's top-left sits at `(x, y)` canvas pixels and is clipped at the canvas
///   edge, on both paths: `overlay` writes into a canvas-sized accumulator, the GPU into
///   a canvas-sized target, so an overhang is drawn and clipped rather than refused;
/// * it keeps its native size when `scale == 1` exactly, and is `canvas * (sx, sy)`
///   otherwise (the multiplier is against the canvas, not the frame);
/// * rotation returns `None`: the CPU's `rotate` fills the corners it exposes with
///   `fillcolor` while the GPU transform leaves them transparent (RK-020).
///
/// `transform.wgsl` puts a layer's centre at `0.5 + scale * translate` and draws it
/// `scale` of the canvas wide, so a centre at `x/cw + sx/2` gives
/// `translate = (x/cw + sx/2 - 0.5) / sx`. A canvas-sized identity layer yields the
/// identity, which `ff_render`'s compositor skips entirely, so that case keeps its exact
/// pixels and its per-frame cost.
///
/// Verified against the CPU: a 64x64 layer at `(10, 4)` scaled `0.5` on a 64x64 canvas
/// lights `(10, 4)..(41, 35)` inclusive, alone and as an overlay; a 64x32 layer at
/// scale 1 on a 64x64 canvas lights `(0, 0)..(63, 31)`; the same 64x32 layer scaled
/// `0.5` at `(10, 4)` lights `(10, 4)..(41, 35)`, which is what tells the canvas-relative
/// multiplier apart from a frame-relative one.
fn layer_transform(
    lp: &GpuLayerPlan,
    frame: (u32, u32),
    canvas: (u32, u32),
) -> Option<LayerTransform> {
    if lp.rotation.abs() > 1e-6 {
        return None;
    }
    let (cw, ch) = (px(canvas.0), px(canvas.1));
    // A zero-sized canvas has no space to place into, and a non-positive scale has no
    // extent to draw; either would divide by zero below.
    if cw <= 0.0 || ch <= 0.0 || lp.scale_x <= 0.0 || lp.scale_y <= 0.0 {
        return None;
    }
    let native =
        (lp.scale_x - 1.0).abs() <= f32::EPSILON && (lp.scale_y - 1.0).abs() <= f32::EPSILON;
    let (sx, sy) = if native {
        (px(frame.0) / cw, px(frame.1) / ch)
    } else {
        (lp.scale_x, lp.scale_y)
    };
    if sx <= 0.0 || sy <= 0.0 {
        return None;
    }
    Some(LayerTransform {
        x: (lp.x / cw + sx / 2.0 - 0.5) / sx,
        y: (lp.y / ch + sy / 2.0 - 0.5) / sy,
        scale_x: sx,
        scale_y: sy,
        rotation: 0.0,
    })
}

/// A pixel count as an `f32`, for the canvas-space arithmetic in [`layer_transform`].
///
/// Distinct from [`ratio`]: `ratio(x, 1)` computes the same number but reads as a
/// proportion, which this is not.
#[allow(clippy::cast_precision_loss)] // pixel dimensions are far inside f32's exact range
fn px(v: u32) -> f32 {
    v as f32
}

/// Loads a `LutNode` from a `.cube` or `.3dl` file, or `None` when the extension is
/// unsupported or the file cannot be loaded. A `None` makes the layer fall back to
/// the CPU path (RK-020) rather than render wrong output.
fn load_lut(path: &str) -> Option<LutNode> {
    let p = std::path::Path::new(path);
    match p.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("cube") => LutNode::from_cube(p).ok(),
        Some(ext) if ext.eq_ignore_ascii_case("3dl") => LutNode::from_3dl(p).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plan layer with the given placement and nothing else.
    fn plan_layer(x: f32, y: f32, scale: (f32, f32), rotation: f32) -> GpuLayerPlan {
        GpuLayerPlan {
            z_order: 0,
            x,
            y,
            scale_x: scale.0,
            scale_y: scale.1,
            rotation,
            opacity: 1.0,
            blend_mode: ff_render::BlendMode::Normal,
            composite_op: ff_render::CompositeOp::Over,
            effects: Vec::new(),
        }
    }

    /// The canvas pixel box a [`LayerTransform`] draws into: `(x0, y0, x1, y1)`.
    ///
    /// Inverts `transform.wgsl`'s mapping — a layer's centre lands at
    /// `0.5 + scale * translate` and it covers `scale` of the canvas — so a test can be
    /// written against the pixel box the CPU was *measured* to produce rather than
    /// against the transform's own numbers, which would just restate the formula.
    fn canvas_box(t: &LayerTransform, canvas: (u32, u32)) -> (f32, f32, f32, f32) {
        let (cw, ch) = (px(canvas.0), px(canvas.1));
        let cx = 0.5 + t.scale_x * t.x;
        let cy = 0.5 + t.scale_y * t.y;
        (
            (cx - t.scale_x / 2.0) * cw,
            (cy - t.scale_y / 2.0) * ch,
            (cx + t.scale_x / 2.0) * cw,
            (cy + t.scale_y / 2.0) * ch,
        )
    }

    fn assert_box(got: (f32, f32, f32, f32), want: (f32, f32, f32, f32), what: &str) {
        for (g, w) in [
            (got.0, want.0),
            (got.1, want.1),
            (got.2, want.2),
            (got.3, want.3),
        ] {
            assert!((g - w).abs() < 0.01, "{what}: got {got:?}, want {want:?}");
        }
    }

    #[test]
    fn layer_transform_should_place_the_base_like_any_layer() {
        // Measured on every route (ADR-0016): a lone 64x64 layer at (10, 4) scaled 0.5 on
        // a 64x64 canvas lights (10, 4)..(41, 35) inclusive, the half-open box
        // (10, 4)..(42, 36). The base gets no special treatment.
        let moved = plan_layer(10.0, 4.0, (0.5, 0.5), 0.0);
        let got = layer_transform(&moved, (64, 64), (64, 64)).expect("a placed layer");
        assert_box(
            canvas_box(&got, (64, 64)),
            (10.0, 4.0, 42.0, 36.0),
            "base placement",
        );
    }
    #[test]
    fn layer_transform_should_place_an_overlay_at_the_measured_box() {
        // The same fixture as an overlay: a 64x64 overlay at (10, 4) scaled 0.5 over a
        // 64x64 base in a 64x64 canvas lit exactly (10, 4)..(41, 35) inclusive.
        let over = plan_layer(10.0, 4.0, (0.5, 0.5), 0.0);
        let got = layer_transform(&over, (64, 64), (64, 64)).expect("an overlay places");
        assert_box(
            canvas_box(&got, (64, 64)),
            (10.0, 4.0, 42.0, 36.0),
            "overlay placement",
        );
    }
    #[test]
    fn layer_transform_should_use_the_native_size_at_scale_one() {
        // The export rule: exactly 1.0 leaves the frame at its own size, from the
        // top-left, with no fit. A 64x32 frame on a 64x64 canvas is the top half.
        let plain = plan_layer(0.0, 0.0, (1.0, 1.0), 0.0);
        let got = layer_transform(&plain, (64, 32), (64, 64)).expect("places");
        assert_box(
            canvas_box(&got, (64, 64)),
            (0.0, 0.0, 64.0, 32.0),
            "native size, top-left",
        );
        // A canvas-sized identity layer must stay on the compositor's transform-free
        // path, so its pixels are exact.
        let same = layer_transform(&plain, (64, 64), (64, 64)).expect("places");
        assert!(same.is_identity(), "canvas-sized identity: {same:?}");
    }
    #[test]
    fn layer_transform_should_scale_against_the_canvas() {
        // The fixture that tells the two readings of the multiplier apart: a 64x32 frame
        // scaled 0.5 on a 64x64 canvas is 32x32 (canvas * scale, distorting the frame as
        // the CPU's `scale=canvas*s` does), not 32x16.
        let half = plan_layer(10.0, 4.0, (0.5, 0.5), 0.0);
        let got = layer_transform(&half, (64, 32), (64, 64)).expect("places");
        assert_box(
            canvas_box(&got, (64, 64)),
            (10.0, 4.0, 42.0, 36.0),
            "the multiplier is against the canvas",
        );
    }
    #[test]
    fn layer_transform_should_decline_rotation_on_every_layer() {
        // The CPU's `rotate` fills the exposed corners with `fillcolor`; the GPU leaves
        // them transparent. Nothing to map it to, so it falls back (RK-020), base or not.
        let spun = plan_layer(0.0, 0.0, (1.0, 1.0), 30.0);
        assert!(layer_transform(&spun, (64, 64), (64, 64)).is_none());
        let spun_and_scaled = plan_layer(10.0, 4.0, (0.5, 0.5), 30.0);
        assert!(layer_transform(&spun_and_scaled, (64, 64), (64, 64)).is_none());
    }
    #[test]
    fn layer_transform_should_let_a_layer_hang_off_the_canvas() {
        // The canvas is the clipping bound on both paths: `overlay` clips to its
        // canvas-sized accumulator, the GPU to its target. An overhang is drawn and
        // clipped, never refused. Off the right edge: 40 + 0.5 * 64 = 72 > 64.
        let over_right = plan_layer(40.0, 0.0, (0.5, 0.5), 0.0);
        let got = layer_transform(&over_right, (64, 64), (64, 64)).expect("places");
        assert_box(
            canvas_box(&got, (64, 64)),
            (40.0, 0.0, 72.0, 32.0),
            "an overhang keeps its geometry",
        );
        // And a negative offset, off the top-left.
        let over_left = plan_layer(-8.0, -8.0, (0.5, 0.5), 0.0);
        let got = layer_transform(&over_left, (64, 64), (64, 64)).expect("places");
        assert_box(
            canvas_box(&got, (64, 64)),
            (-8.0, -8.0, 24.0, 24.0),
            "a negative offset keeps its geometry",
        );
    }
    #[test]
    fn layer_transform_should_reject_a_degenerate_layer() {
        // A non-positive scale has no extent to draw, and would divide by zero.
        assert!(
            layer_transform(&plan_layer(0.0, 0.0, (0.0, 1.0), 0.0), (64, 64), (64, 64)).is_none()
        );
        assert!(
            layer_transform(&plan_layer(0.0, 0.0, (1.0, -1.0), 0.0), (64, 64), (64, 64)).is_none()
        );
        // A zero-sized canvas has no space to place into, and a zero-sized frame at
        // scale 1 has no extent.
        assert!(
            layer_transform(&plan_layer(0.0, 0.0, (1.0, 1.0), 0.0), (64, 64), (0, 64)).is_none()
        );
        assert!(
            layer_transform(&plan_layer(0.0, 0.0, (1.0, 1.0), 0.0), (0, 64), (64, 64)).is_none()
        );
    }
    /// A `ShapeMask` whose rectangle differs must still reuse the cached graph, or an
    /// animated rectangle would rebuild the whole effect chain every frame (#1710).
    #[test]
    fn a_moved_rectangle_should_reuse_the_graph_with_a_parameter() {
        let was = [GpuEffect::ShapeMask {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
            invert: false,
        }];
        let now = [GpuEffect::ShapeMask {
            x: 5,
            y: 2,
            width: 10,
            height: 10,
            invert: true,
        }];
        let Some(Reuse::WithParams(params)) = param_update(&was, &now) else {
            panic!("a moved rectangle must reuse the cached graph");
        };
        assert!(
            matches!(
                params.as_slice(),
                [NodeParam::ShapeMaskRect {
                    x: 5,
                    y: 2,
                    width: 10,
                    height: 10,
                    invert: true,
                }]
            ),
            "the whole rectangle must travel to the live node, got {params:?}"
        );
    }

    /// The other side of the same gate: an unchanged rectangle must not push a
    /// parameter it does not need.
    #[test]
    fn an_unchanged_rectangle_should_reuse_the_graph_as_is() {
        let effects = [GpuEffect::ShapeMask {
            x: 5,
            y: 2,
            width: 10,
            height: 10,
            invert: false,
        }];
        assert!(
            matches!(param_update(&effects, &effects), Some(Reuse::AsIs)),
            "an unchanged rectangle must run the graph as it stands"
        );
    }

    /// `LumaMask` used to be excluded from the cache because its node baked the source
    /// frame's own pixels. The shader samples the frame instead now, so it caches like
    /// any other effect (#1710).
    #[test]
    fn a_luma_mask_should_be_cacheable() {
        let effects = [GpuEffect::LumaMask { invert: false }];
        assert!(
            matches!(param_update(&effects, &effects), Some(Reuse::AsIs)),
            "a luma mask must reuse its cached graph"
        );
    }
}
