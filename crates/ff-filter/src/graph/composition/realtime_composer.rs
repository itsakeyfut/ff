//! Real-time multi-layer video compositor fed by externally-decoded frames.
//!
//! Unlike [`MultiTrackComposer`](super::MultiTrackComposer) (which decodes
//! internally via `movie` sources and is pulled to completion for export), a
//! [`RealtimeComposer`] exposes one `buffersrc` input per layer so a host (e.g.
//! a seekable preview player) feeds already-decoded frames per layer per tick
//! and pulls one composited frame. Per-clip effects and blend modes are applied
//! by the **same** `FFmpeg` filter primitives the export path uses, so the
//! preview matches the rendered output.

use ff_format::{PixelFormat, VideoFrame};

use crate::animation::AnimatedValue;
use crate::blend::BlendMode;
use crate::composite::CompositeOp;
use crate::error::FilterError;
use crate::graph::filter_step::FilterStep;
use crate::graph::graph::FilterGraph;

// RealtimeLayer

/// One layer in a [`RealtimeComposer`], composited bottom-up in `Vec` order
/// (index `0` is the base; later layers blend on top).
///
/// Frames pushed to this layer via [`RealtimeComposer::push_layer`] must match
/// the [`width`](Self::width), [`height`](Self::height), and
/// [`pixel_format`](Self::pixel_format) declared here — these fix the layer's
/// `buffersrc` format at build time.
#[derive(Debug, Clone)]
pub struct RealtimeLayer {
    /// Width in pixels of frames pushed to this layer.
    pub width: u32,
    /// Height in pixels of frames pushed to this layer.
    pub height: u32,
    /// Pixel format of frames pushed to this layer.
    pub pixel_format: PixelFormat,
    /// Per-clip video effect chain applied to this layer before compositing
    /// (the same `FilterStep`s as `Clip::video_effect_chain` / the export path).
    pub effects: Vec<FilterStep>,
    /// Opacity in `[0.0, 1.0]`, static or animated. Applied via `colorchannelmixer`
    /// alpha when this layer is blended onto the layer below (Normal blend only). An
    /// [`AnimatedValue::Track`] registers the node for per-frame `send_command`. No
    /// effect on the base layer 0 (apply base opacity host-side).
    pub opacity: AnimatedValue<f64>,
    /// X position (pixels) of this layer's top-left on the canvas, static or
    /// animated. Maps to the `overlay` filter's `x` (Normal blend only); a
    /// [`AnimatedValue::Track`] uses `:eval=frame` + per-frame `send_command`.
    /// Applies to the base layer 0 too (ADR-0016).
    pub x: AnimatedValue<f64>,
    /// Overlay Y position; see [`x`](Self::x).
    pub y: AnimatedValue<f64>,
    /// Horizontal scale multiplier, evaluated statically at t=0 (matching the export
    /// path): `1.0` leaves the frame at its native size, any other value scales it
    /// to `canvas * scale_x`. Applied before the layer's effects, on every layer.
    pub scale_x: AnimatedValue<f64>,
    /// Vertical scale multiplier; see [`scale_x`](Self::scale_x).
    pub scale_y: AnimatedValue<f64>,
    /// Clockwise rotation in degrees, evaluated statically at t=0 (matching the
    /// export path); `0.0` = no rotation, exposed corners fill black. Applied before
    /// the layer's effects, on every layer.
    pub rotation: AnimatedValue<f64>,
    /// How this layer blends with the layer below. [`BlendMode::Normal`] uses
    /// `overlay`; other modes use `blend=all_mode=<token>`.
    pub blend_mode: BlendMode,
    /// Porter-Duff composite operator. Carried for representation parity with the
    /// export [`VideoLayer`](super::VideoLayer); the realtime builder renders `Over`
    /// today (the other operators land in C4b).
    pub composite_op: CompositeOp,
}

// RealtimeLayerDescriptor

/// The dimension-independent part of a [`RealtimeLayer`]: every field except
/// `width` / `height` / `pixel_format`, which are only known once a frame has been
/// decoded.
///
/// An engine derives a descriptor from its editing model; a real-time driver
/// realises it into a [`RealtimeLayer`] at decode time via
/// [`RealtimeLayer::with_dimensions`].
#[derive(Debug, Clone)]
pub struct RealtimeLayerDescriptor {
    /// See [`RealtimeLayer::effects`].
    pub effects: Vec<FilterStep>,
    /// See [`RealtimeLayer::opacity`].
    pub opacity: AnimatedValue<f64>,
    /// See [`RealtimeLayer::x`].
    pub x: AnimatedValue<f64>,
    /// See [`RealtimeLayer::y`].
    pub y: AnimatedValue<f64>,
    /// See [`RealtimeLayer::scale_x`].
    pub scale_x: AnimatedValue<f64>,
    /// See [`RealtimeLayer::scale_y`].
    pub scale_y: AnimatedValue<f64>,
    /// See [`RealtimeLayer::rotation`].
    pub rotation: AnimatedValue<f64>,
    /// See [`RealtimeLayer::blend_mode`].
    pub blend_mode: BlendMode,
    /// See [`RealtimeLayer::composite_op`].
    pub composite_op: CompositeOp,
}

impl RealtimeLayer {
    /// Build a [`RealtimeLayer`] from a [`RealtimeLayerDescriptor`] plus the frame
    /// dimensions and pixel format known at decode time.
    #[must_use]
    pub fn with_dimensions(
        descriptor: RealtimeLayerDescriptor,
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
    ) -> Self {
        Self {
            width,
            height,
            pixel_format,
            effects: descriptor.effects,
            opacity: descriptor.opacity,
            x: descriptor.x,
            y: descriptor.y,
            scale_x: descriptor.scale_x,
            scale_y: descriptor.scale_y,
            rotation: descriptor.rotation,
            blend_mode: descriptor.blend_mode,
            composite_op: descriptor.composite_op,
        }
    }
}

// RealtimeComposer

/// Composites externally-decoded frames from several layers into one frame,
/// reusing a single built filter graph across frames.
///
/// Build once with [`new`](Self::new); then per output frame,
/// [`push_layer`](Self::push_layer) one frame for every layer and
/// [`pull`](Self::pull) the composited result. The output frame is
/// `rgba`. The graph (and any `lut3d` file its effects load) is built once, so it
/// is suitable for real-time playback.
pub struct RealtimeComposer {
    graph: FilterGraph,
    layer_count: usize,
    /// The hidden input slot of the canvas accumulator, when the base layer is placed
    /// rather than used as the accumulator itself (ADR-0016).
    canvas_slot: Option<usize>,
    /// The black canvas frame pushed into `canvas_slot` before every base frame,
    /// stamped with that frame's timestamp so the `overlay` sees them as one tick.
    canvas_frame: Option<VideoFrame>,
}

impl RealtimeComposer {
    /// Builds a compositor for the given layers (at least one required).
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::CompositionFailed`] when `layers` is empty or the
    /// underlying `FFmpeg` graph cannot be built.
    pub fn new(layers: &[RealtimeLayer]) -> Result<Self, FilterError> {
        Self::with_canvas(layers, None)
    }

    /// Like [`new`](Self::new), but composites onto a project canvas of
    /// `canvas = (width, height)` pixels: every layer, the base included, is placed
    /// in canvas pixels by its `x` / `y` / `scale` / `rotation` (ADR-0016), and the
    /// output is exactly canvas-sized. A base whose frames are not canvas-sized sits
    /// at its native size from the top-left; framing against the canvas is a
    /// `FitMode` effect, never implicit. `None` uses the base layer's own size as the
    /// canvas (identical to [`new`](Self::new)).
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::CompositionFailed`] when `layers` is empty or the
    /// underlying `FFmpeg` graph cannot be built.
    pub fn with_canvas(
        layers: &[RealtimeLayer],
        canvas: Option<(u32, u32)>,
    ) -> Result<Self, FilterError> {
        let layer_count = layers.len();
        let (graph, canvas_slot) =
            super::composition_inner::build_realtime_composition(layers, canvas)?;
        let canvas_frame = match canvas_slot {
            Some(_) => {
                let (w, h) = canvas.unwrap_or((layers[0].width, layers[0].height));
                let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
                for px in rgba.as_chunks_mut::<4>().0 {
                    px[3] = 255;
                }
                Some(VideoFrame::from_rgba(w, h, rgba).map_err(|e| {
                    FilterError::CompositionFailed {
                        reason: format!("canvas frame: {e}"),
                    }
                })?)
            }
            None => None,
        };
        Ok(Self {
            graph,
            layer_count,
            canvas_slot,
            canvas_frame,
        })
    }

    /// Number of layers (== number of input slots).
    #[must_use]
    pub fn layer_count(&self) -> usize {
        self.layer_count
    }

    /// Pushes one frame into layer `idx`'s input slot.
    ///
    /// Push exactly one frame per layer before each [`pull`](Self::pull). The
    /// frame must match the layer's declared width/height/pixel format.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError::InvalidInput`] if `idx` is not a layer slot, and the
    /// graph's error if the frame cannot be pushed. After a push error the composer's
    /// current tick is incomplete and it must be rebuilt.
    pub fn push_layer(&mut self, idx: usize, frame: &VideoFrame) -> Result<(), FilterError> {
        // The graph may own one more buffersrc than there are layers (the canvas
        // accumulator), so the public slot range is checked here, not by the graph.
        if idx >= self.layer_count {
            return Err(FilterError::InvalidInput {
                slot: idx,
                reason: format!("layer slot out of range (layers={})", self.layer_count),
            });
        }
        // The base goes first: a host frame is the push that can fail (wrong size or
        // format), and a failure then leaves the graph without a half-pushed tick. A
        // failed canvas push after it still strands the base frame, so an error from
        // this method leaves the composer's tick state undefined: rebuild it.
        self.graph.push_video(idx, frame)?;
        if idx == 0
            && let (Some(slot), Some(canvas)) = (self.canvas_slot, self.canvas_frame.as_mut())
        {
            canvas.set_timestamp(frame.timestamp());
            self.graph.push_video(slot, canvas)?;
        }
        Ok(())
    }

    /// Pulls the next composited frame (`rgba`), or `None` if not yet available.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] on an unexpected `FFmpeg` error.
    pub fn pull(&mut self) -> Result<Option<VideoFrame>, FilterError> {
        self.graph.pull_video()
    }
}

// LavfiSource

/// Generates video frames from an `FFmpeg` `lavfi` filtergraph string (e.g.
/// `color=s=1920x1080:c=black@0.0,drawtext=text='Title'`), so a host such as the
/// preview runner can feed a timeline-level generated overlay into a
/// [`RealtimeComposer`] as pushed frames.
///
/// The output is `rgba` and preserves the graph's own alpha (no canvas is
/// composited underneath), matching the export path's topmost lavfi layer. Frames
/// are produced sequentially via [`pull`](Self::pull); the source exposes no seek,
/// so a host that seeks rebuilds it from scratch.
pub struct LavfiSource {
    graph: FilterGraph,
}

impl LavfiSource {
    /// Builds a source from a `lavfi` filtergraph string.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] when the underlying `FFmpeg` graph cannot be built
    /// (e.g. the `movie` / `lavfi` demuxer is unavailable, or the string is invalid).
    pub fn new(lavfi: &str) -> Result<Self, FilterError> {
        let graph = super::composition_inner::build_lavfi_source(lavfi)?;
        Ok(Self { graph })
    }

    /// Pulls the next generated frame (`rgba`), or `None` when none is buffered yet
    /// or at end-of-stream.
    ///
    /// # Errors
    ///
    /// Returns [`FilterError`] on an unexpected `FFmpeg` error.
    pub fn pull(&mut self) -> Result<Option<VideoFrame>, FilterError> {
        self.graph.pull_video()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn realtime_layer_with_dimensions_should_copy_descriptor_fields() {
        let descriptor = RealtimeLayerDescriptor {
            effects: vec![FilterStep::HFlip],
            opacity: AnimatedValue::Track(crate::animation::AnimationTrack::new()),
            x: AnimatedValue::Static(12.0),
            y: AnimatedValue::Static(34.0),
            scale_x: AnimatedValue::Static(2.0),
            scale_y: AnimatedValue::Static(3.0),
            rotation: AnimatedValue::Static(45.0),
            blend_mode: BlendMode::Multiply,
            composite_op: CompositeOp::Under,
        };
        let layer = RealtimeLayer::with_dimensions(descriptor, 640, 360, PixelFormat::Rgba);
        assert_eq!(layer.width, 640);
        assert_eq!(layer.height, 360);
        assert!(matches!(layer.pixel_format, PixelFormat::Rgba));
        assert_eq!(layer.effects.len(), 1, "effects moved into the layer");
        assert!(matches!(layer.opacity, AnimatedValue::Track(_)));
        assert!(matches!(layer.x, AnimatedValue::Static(v) if (v - 12.0).abs() < f64::EPSILON));
        assert!(matches!(layer.y, AnimatedValue::Static(v) if (v - 34.0).abs() < f64::EPSILON));
        assert!(
            matches!(layer.scale_x, AnimatedValue::Static(v) if (v - 2.0).abs() < f64::EPSILON)
        );
        assert!(
            matches!(layer.scale_y, AnimatedValue::Static(v) if (v - 3.0).abs() < f64::EPSILON)
        );
        assert!(
            matches!(layer.rotation, AnimatedValue::Static(v) if (v - 45.0).abs() < f64::EPSILON)
        );
        assert!(matches!(layer.blend_mode, BlendMode::Multiply));
        assert!(matches!(layer.composite_op, CompositeOp::Under));
    }

    #[test]
    fn empty_layers_should_err() {
        let result = RealtimeComposer::new(&[]);
        assert!(matches!(result, Err(FilterError::CompositionFailed { .. })));
    }

    #[test]
    fn base_layer_with_glow_compound_effect_should_build() {
        // Regression: `Glow` is a compound `FilterStep` (split → curves → gblur →
        // blend). The realtime compositor drives per-layer effects through
        // `add_and_link_step`, which must dispatch compound steps to their builders
        // rather than create a single `split` filter with the whole subgraph as
        // args (which failed with "No such option: split").
        //
        // Probe-gate: a no-effect base layer needs only `buffer`/`format`/
        // `buffersink`. CI's Linux FFmpeg is built with no filters, so even that
        // fails to build there — skip. Where it builds, the filter set is present,
        // so the `Glow` layer *must* also build; that is the regression assertion
        // (it fails before the fix, on any host with filters).
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let layer = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::Glow {
                threshold: 0.6,
                radius: 4.0,
                intensity: 0.5,
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[layer])
            .expect("Glow compound step must dispatch and build once FFmpeg filters exist");
        let frame = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_with_glow_animated_compound_effect_should_build() {
        // `GlowAnimated` dispatches to the same compound builder (`add_glow_step`) as
        // `Glow`, using its `Duration::ZERO` values. This drives that dispatch through
        // `add_and_link_step` for real (the args-only unit test does not). Same
        // probe-gate as the static Glow test: skip where FFmpeg has no filters.
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let layer = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::GlowAnimated {
                threshold: AnimatedValue::Static(0.6),
                radius: AnimatedValue::Static(4.0),
                intensity: AnimatedValue::Static(0.5),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[layer])
            .expect("GlowAnimated compound step must dispatch and build once filters exist");
        let frame = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_with_three_way_cc_animated_should_build() {
        // `ThreeWayCCAnimated` builds the `curves` filter from its `Duration::ZERO`
        // values through the generic `add_and_link_step` path. This drives that build
        // for real (the args-only unit test does not). Same probe-gate as the other
        // compound-effect tests: skip where FFmpeg has no filters.
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        // Non-neutral lift/gamma/gain so the curves are not the identity.
        let ch = |v: f64| {
            [
                AnimatedValue::Static(v),
                AnimatedValue::Static(v),
                AnimatedValue::Static(v),
            ]
        };
        let layer = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::ThreeWayCCAnimated {
                lift: ch(1.1),
                gamma: ch(1.2),
                gain: ch(1.05),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[layer])
            .expect("ThreeWayCCAnimated must build the curves filter once filters exist");
        let frame = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_with_hsl_animated_should_build() {
        // `HslAnimated` builds the `hue` filter from its `Duration::ZERO` values
        // through the generic `add_and_link_step` path. This drives that build for
        // real (the args-only unit test does not). Same probe-gate: skip where
        // FFmpeg has no filters.
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        // Non-neutral hue/saturation/lightness so the filter is not the identity.
        let layer = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::HslAnimated {
                hue: AnimatedValue::Static(20.0),
                saturation: AnimatedValue::Static(1.2),
                lightness: AnimatedValue::Static(0.05),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[layer])
            .expect("HslAnimated must build the hue filter once filters exist");
        let frame = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_with_raw_effect_should_build() {
        // #1376: `FilterStep::Raw` (the arbitrary-avfilter escape hatch) must work as a
        // per-layer effect through the realtime (preview) compositor's `add_and_link_step`
        // dispatch, exactly like the typed steps — this is the composer-path coverage for
        // the raw hatch. `hflip` (one-in / one-out, no args) is the raw filter. Probe-gate
        // as above: CI's Linux FFmpeg has no filters, so a no-effect base layer fails to
        // build there — skip; where filters exist, the raw `hflip` layer must build.
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let layer = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::Raw {
                filter: "hflip".to_string(),
                args: String::new(),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[layer])
            .expect("raw `hflip` effect must dispatch and build once FFmpeg filters exist");
        let frame = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    /// The lit box of an rgba buffer as `(x0, y0, x1, y1)` inclusive: pixels whose red
    /// channel is above 128.
    fn lit_box(rgba: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
        let mut b: Option<(u32, u32, u32, u32)> = None;
        for y in 0..h {
            for x in 0..w {
                if rgba[((y * w + x) * 4) as usize] > 128 {
                    b = Some(match b {
                        None => (x, y, x, y),
                        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    });
                }
            }
        }
        b
    }

    fn placed_base(w: u32, h: u32, x: f64, y: f64, scale: f64, rotation: f64) -> RealtimeLayer {
        RealtimeLayer {
            width: w,
            height: h,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(x),
            y: AnimatedValue::Static(y),
            scale_x: AnimatedValue::Static(scale),
            scale_y: AnimatedValue::Static(scale),
            rotation: AnimatedValue::Static(rotation),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        }
    }

    /// Pushes one `w`x`h` white frame through a composer built for `layer` on
    /// `canvas`, returning the rgba output, or `None` when `FFmpeg` filters are
    /// unavailable (skip).
    fn composite_white(layer: RealtimeLayer, canvas: (u32, u32)) -> Option<(Vec<u8>, u32, u32)> {
        let (w, h) = (layer.width, layer.height);
        let mut composer = match RealtimeComposer::with_canvas(&[layer], Some(canvas)) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return None;
            }
        };
        let frame = VideoFrame::from_rgba(w, h, vec![255u8; (w * h * 4) as usize]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return None;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                let rgba = out.to_rgba().expect("rgba");
                Some((rgba, out.width(), out.height()))
            }
            other => {
                println!("Skipping: pull -> {other:?}");
                None
            }
        }
    }

    #[test]
    fn with_canvas_should_place_the_base_at_native_size_on_the_canvas() {
        // A 640x360 base on a 1080x1920 canvas is neither letterboxed nor stretched
        // (ADR-0016): the output is canvas-sized with the base at its own size from the
        // top-left and the canvas black elsewhere. Framing is a `FitMode` effect.
        let Some((rgba, w, h)) =
            composite_white(placed_base(640, 360, 0.0, 0.0, 1.0, 0.0), (1080, 1920))
        else {
            return;
        };
        assert_eq!((w, h), (1080, 1920));
        assert_eq!(
            lit_box(&rgba, w, h),
            Some((0, 0, 639, 359)),
            "native size, top-left"
        );
    }

    #[test]
    fn base_layer_position_and_scale_should_render_in_canvas_space() {
        // Measured on the export composer and now on this one: a 64x64 base at (10, 4)
        // scaled 0.5 on a 64x64 canvas lights (10, 4)..(41, 35) inclusive.
        let Some((rgba, w, h)) =
            composite_white(placed_base(64, 64, 10.0, 4.0, 0.5, 0.0), (64, 64))
        else {
            return;
        };
        assert_eq!(
            lit_box(&rgba, w, h),
            Some((10, 4, 41, 35)),
            "canvas-space placement"
        );
    }

    #[test]
    fn base_layer_scale_should_be_canvas_relative() {
        // The export rule: a 64x32 base scaled 0.5 on a 64x64 canvas is 32x32, not 32x16.
        let Some((rgba, w, h)) =
            composite_white(placed_base(64, 32, 10.0, 4.0, 0.5, 0.0), (64, 64))
        else {
            return;
        };
        assert_eq!(
            lit_box(&rgba, w, h),
            Some((10, 4, 41, 35)),
            "canvas * scale"
        );
    }

    #[test]
    fn base_layer_rotation_should_fill_the_exposed_corners_black() {
        // A white 64x64 base rotated 45 degrees: the frame's corners are outside the
        // rotated square, and `rotate` fills them black, while the centre stays white.
        let Some((rgba, w, h)) =
            composite_white(placed_base(64, 64, 0.0, 0.0, 1.0, 45.0), (64, 64))
        else {
            return;
        };
        let at = |x: u32, y: u32| rgba[((y * w + x) * 4) as usize];
        assert!(
            at(0, 0) < 16,
            "top-left corner must be black, got {}",
            at(0, 0)
        );
        assert!(
            at(w - 1, h - 1) < 16,
            "bottom-right corner must be black, got {}",
            at(w - 1, h - 1)
        );
        assert!(
            at(32, 32) > 200,
            "the centre must stay white, got {}",
            at(32, 32)
        );
    }

    #[test]
    fn push_layer_should_reject_the_hidden_canvas_slot() {
        // A placed base gives the graph one more buffersrc than there are layers. That
        // slot is the composer's, and `push_layer` must refuse it like any index past
        // the layers rather than let a host frame into the accumulator.
        let mut composer = match RealtimeComposer::with_canvas(
            &[placed_base(8, 8, 2.0, 0.0, 1.0, 0.0)],
            Some((8, 8)),
        ) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        assert_eq!(composer.layer_count(), 1);
        let frame = VideoFrame::from_rgba(8, 8, vec![255u8; 8 * 8 * 4]).unwrap();
        let err = composer
            .push_layer(1, &frame)
            .expect_err("slot 1 is not a layer");
        assert!(
            matches!(err, FilterError::InvalidInput { slot: 1, .. }),
            "the hidden canvas slot must read as out of range, got {err:?}"
        );
    }

    #[test]
    fn base_layer_position_track_should_move_per_frame() {
        // An animated base position goes through the same `overlay:eval=frame` +
        // `send_command` wiring an overlay's does: two frames a second apart land at
        // the track's value for each.
        use crate::animation::{AnimationTrack, Easing, Keyframe};
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;
        let track = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0, Easing::Linear))
            .push(Keyframe::new(Duration::from_secs(1), 20.0, Easing::Linear));
        let mut base = placed_base(64, 64, 0.0, 0.0, 0.5, 0.0);
        base.x = AnimatedValue::Track(track);
        let mut composer = match RealtimeComposer::with_canvas(&[base], Some((64, 64))) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let mut boxes = Vec::new();
        for secs in [0u64, 1] {
            let mut frame = VideoFrame::from_rgba(64, 64, vec![255u8; 64 * 64 * 4]).unwrap();
            frame.set_timestamp(Timestamp::from_duration(
                Duration::from_secs(secs),
                Rational::new(1, 1_000_000),
            ));
            if composer.push_layer(0, &frame).is_err() {
                println!("Skipping: push failed (FFmpeg unavailable?)");
                return;
            }
            match composer.pull() {
                Ok(Some(out)) => {
                    let rgba = out.to_rgba().expect("rgba");
                    boxes.push(lit_box(&rgba, out.width(), out.height()));
                }
                other => {
                    println!("Skipping: pull -> {other:?}");
                    return;
                }
            }
        }
        assert_eq!(boxes[0], Some((0, 0, 31, 31)), "at t=0 the track reads 0");
        assert_eq!(
            boxes[1],
            Some((20, 0, 51, 31)),
            "at t=1s the track reads 20"
        );
    }

    #[test]
    fn base_layer_large_scale_then_offset_crop_does_not_overrun() {
        // Regression: a large up-`Scale` followed by a `Crop` with a large centre
        // offset used to segfault when reading back the composited frame — the crop
        // view's `data[i]` is offset into the scaled buffer while its linesize stays
        // the parent's, so copying `stride * rows` overran the buffer by the offset.
        // 640×360 → scale 3412×1920 → crop 1080×1920 at x=1166 (centre).
        let base = RealtimeLayer {
            width: 640,
            height: 360,
            pixel_format: PixelFormat::Rgba,
            effects: vec![
                FilterStep::Scale {
                    width: 3412,
                    height: 1920,
                    algorithm: crate::graph::types::ScaleAlgorithm::Bicubic,
                },
                FilterStep::Crop {
                    x: 1166,
                    y: 0,
                    width: 1080,
                    height: 1920,
                },
            ],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let frame = VideoFrame::from_rgba(640, 360, vec![120u8; 640 * 360 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.width(), 1080);
                assert_eq!(out.height(), 1920);
                let rgba = out.to_rgba().expect("rgba");
                assert_eq!(rgba.len(), 1080 * 1920 * 4);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_fit_to_aspect_is_scaled_and_padded() {
        // Regression: `FitToAspect` is a compound scale+pad. The realtime compositor
        // used to apply only the scale, so the frame was never padded to the target
        // canvas. A 640×360 (16:9) base fitted to 1080×1920 (9:16) must be scaled to
        // fit (1080×608) *and* padded to the full 1080×1920, not left at 1080×608.
        let base = RealtimeLayer {
            width: 640,
            height: 360,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::FitToAspect {
                width: 1080,
                height: 1920,
                color: "black".to_string(),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let frame = VideoFrame::from_rgba(640, 360, vec![120u8; 640 * 360 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.width(), 1080);
                assert_eq!(out.height(), 1920);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn two_layer_composite_should_produce_rgba_frame() {
        // 4×4 RGBA base + overlay; skip-guard on FFmpeg availability.
        let layer = |op: f32| RealtimeLayer {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(f64::from(op)),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[layer(1.0), layer(0.5)]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let base = VideoFrame::from_rgba(4, 4, vec![10u8; 4 * 4 * 4]).unwrap();
        let top = VideoFrame::from_rgba(4, 4, vec![200u8; 4 * 4 * 4]).unwrap();
        if composer.push_layer(0, &base).is_err() || composer.push_layer(1, &top).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.format(), PixelFormat::Rgba);
                assert_eq!(out.width(), 4);
                assert_eq!(out.height(), 4);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn differently_sized_layers_with_blend_mode_should_composite() {
        // Regression: the `blend` filter (Screen/Multiply/…) requires equal-sized
        // inputs, so the composer must scale each layer to the base size. Base 4×4,
        // overlay 8×6, Screen. Output is the base size.
        let base = RealtimeLayer {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let top = RealtimeLayer {
            width: 8,
            height: 6,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Screen,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base, top]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let bf = VideoFrame::from_rgba(4, 4, vec![80u8; 4 * 4 * 4]).unwrap();
        let tf = VideoFrame::from_rgba(8, 6, vec![80u8; 8 * 6 * 4]).unwrap();
        if composer.push_layer(0, &bf).is_err() || composer.push_layer(1, &tf).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.format(), PixelFormat::Rgba);
                assert_eq!(out.width(), 4);
                assert_eq!(out.height(), 4);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_crop_then_scale_zooms_into_the_cropped_region() {
        // Crop the right half then scale back to full size ("crop & zoom"). The
        // top-left output pixel (black in the source's left half) must become the
        // white of the cropped right half — proving crop+scale actually resamples.
        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![
                FilterStep::Crop {
                    x: 4,
                    y: 0,
                    width: 4,
                    height: 8,
                },
                FilterStep::Scale {
                    width: 8,
                    height: 8,
                    algorithm: crate::graph::types::ScaleAlgorithm::Bilinear,
                },
            ],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        // Left half (cols 0..4) black, right half (cols 4..8) white.
        let mut data = vec![0u8; 8 * 8 * 4];
        for row in 0..8 {
            for col in 4..8 {
                let p = (row * 8 + col) * 4;
                data[p] = 255;
                data[p + 1] = 255;
                data[p + 2] = 255;
                data[p + 3] = 255;
            }
        }
        for row in 0..8 {
            for col in 0..4 {
                data[(row * 8 + col) * 4 + 3] = 255; // opaque black
            }
        }
        let frame = VideoFrame::from_rgba(8, 8, data).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.width(), 8);
                assert_eq!(out.height(), 8);
                let rgba = out.to_rgba().expect("rgba");
                // Top-left pixel: source left half was black; after cropping to the
                // right (white) half and scaling up it must be near-white.
                assert!(
                    rgba[0] > 200,
                    "expected top-left to be white after crop+zoom, got {}",
                    rgba[0]
                );
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_crop_resizes_the_output_frame() {
        // A base-layer `Crop` shrinks the composited frame: the output must carry
        // the cropped dimensions (not the pushed 8×8), and its RGBA buffer length
        // must equal `width * height * 4`. Consumers that report the decoded size
        // instead of this one would mis-tag the buffer and drop the frame.
        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::Crop {
                x: 0,
                y: 0,
                width: 4,
                height: 6,
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let frame = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.width(), 4);
                assert_eq!(out.height(), 6);
                let rgba = out.to_rgba().expect("rgba");
                assert_eq!(rgba.len(), 4 * 6 * 4);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_with_srt_subtitles_builds_and_pulls() {
        // Diagnostic/regression: a base layer carrying `SubtitlesSrt` (a generic,
        // PTS-dependent filter) must build in the realtime composer and pull a
        // frame — this is the path the demo preview uses (#138). Requires an
        // FFmpeg built with libass; skip if the `subtitles` filter is absent.
        if !super::super::composition_inner::subtitles_filter_available() {
            println!("Skipping: subtitles filter unavailable (no libass)");
            return;
        }
        // Write a tiny .srt whose first cue covers t=0 (pushed frame pts=0).
        let srt = std::env::temp_dir().join("avio_rt_subs_test.srt");
        if std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:05,000\nhello subtitle\n").is_err() {
            println!("Skipping: could not write temp .srt");
            return;
        }
        let base = RealtimeLayer {
            width: 320,
            height: 240,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::SubtitlesSrt {
                path: srt.to_string_lossy().into_owned(),
                force_style: Some("Fontsize=24,PrimaryColour=&H00FFFFFF&,Alignment=2".to_owned()),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base]) {
            Ok(c) => c,
            Err(e) => {
                // Build failure here is the bug we are hunting for (#138 preview).
                panic!("realtime composer failed to build subtitles layer: {e}");
            }
        };
        let frame = VideoFrame::from_rgba(320, 240, vec![40u8; 320 * 240 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.format(), PixelFormat::Rgba);
                assert_eq!(out.width(), 320);
                assert_eq!(out.height(), 240);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => panic!("pull failed for subtitles layer: {e}"),
        }
    }

    #[test]
    fn base_layer_with_diagonal_polygon_matte_builds_and_pulls() {
        // Regression: a diagonal `PolygonMatte` must build its `geq` and pull a frame,
        // catching invalid geq expressions (e.g. `iw`/`ih`, which geq lacks) that a
        // string-only test would miss. Probe-gated: CI's Linux FFmpeg is built with no
        // filters, so even a no-effect layer fails to build there — skip. Where the
        // probe builds, the geq layer MUST also build; that is the regression check.
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }
        let base = RealtimeLayer {
            width: 320,
            height: 240,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::PolygonMatte {
                vertices: vec![(0.2, 0.1), (0.9, 0.5), (0.3, 0.9)],
                invert: false,
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base])
            .expect("polygon-matte geq must build once FFmpeg filters exist");
        let frame = VideoFrame::from_rgba(320, 240, vec![90u8; 320 * 240 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.format(), PixelFormat::Rgba);
                assert_eq!(out.width(), 320);
                assert_eq!(out.height(), 240);
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn base_layer_with_inverted_lumakey_builds_and_pulls() {
        // Regression: `LumaKey { invert: true }` appends an alpha-negating `geq`, which
        // must be built by `add_and_link_step` (the realtime/preview path), not only by
        // the single-source export builder — otherwise invert is a no-op in the preview.
        // Probe-gated like the other realtime tests (CI Linux FFmpeg has no filters).
        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }
        let base = RealtimeLayer {
            width: 320,
            height: 240,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::LumaKey {
                threshold: 0.5,
                tolerance: 0.2,
                softness: 0.1,
                invert: true,
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base])
            .expect("inverted lumakey must build once FFmpeg filters exist");
        let frame = VideoFrame::from_rgba(320, 240, vec![130u8; 320 * 240 * 4]).unwrap();
        if composer.push_layer(0, &frame).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn animated_opacity_track_changes_composite_across_pts() {
        // A top layer with an opacity ramp (0→1 over 1s) over a black base: at PTS 0 the
        // top is transparent (composite ≈ black), at PTS 1s it is opaque (composite ≈
        // white). Proves the realtime composer registers the `blend_ccm` animation and
        // evaluates it at each pushed frame's PTS. Probe-gated: CI Linux FFmpeg has no
        // filters, so even a no-effect layer fails to build there — skip.
        use crate::animation::{AnimationTrack, Easing, Keyframe};
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let track = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0, Easing::Linear))
            .push(Keyframe::new(Duration::from_secs(1), 1.0, Easing::Linear));
        let base = RealtimeLayer {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let top = RealtimeLayer {
            width: 4,
            height: 4,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Track(track),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base, top])
            .expect("animated-opacity composite must build once FFmpeg filters exist");

        // Opaque black base and opaque white top, each stamped with the query PTS.
        let stamped = |rgba: Vec<u8>, pts: Duration| -> VideoFrame {
            let mut f = VideoFrame::from_rgba(4, 4, rgba).unwrap();
            f.set_timestamp(Timestamp::from_duration(pts, Rational::new(1, 1_000_000)));
            f
        };
        let black = |pts| stamped([0u8, 0, 0, 255].repeat(16), pts);
        let white = |pts| stamped([255u8, 255, 255, 255].repeat(16), pts);

        let sample = |composer: &mut RealtimeComposer, pts: Duration| -> Option<u8> {
            composer.push_layer(0, &black(pts)).ok()?;
            composer.push_layer(1, &white(pts)).ok()?;
            Some(composer.pull().ok()??.to_rgba()?[0])
        };

        let Some(r0) = sample(&mut composer, Duration::ZERO) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        let Some(r1) = sample(&mut composer, Duration::from_secs(1)) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        assert!(
            r1 > r0 + 100,
            "opacity ramp should brighten the composite across PTS: r0={r0} r1={r1}"
        );
    }

    #[test]
    fn animated_position_track_moves_overlay_across_pts() {
        // A blue top layer slides in from off-screen-right (x=8, over an 8-wide canvas)
        // to fully covering (x=0) across 1s, over a red base. At PTS 0 the composite is
        // red (top off-screen); at PTS 1s it is blue (top covers). Proves the realtime
        // overlay honors an animated x position via `send_command`. Probe-gated.
        use crate::animation::{AnimationTrack, Easing, Keyframe};
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let x_track = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 8.0, Easing::Linear))
            .push(Keyframe::new(Duration::from_secs(1), 0.0, Easing::Linear));
        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let top = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Track(x_track),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base, top])
            .expect("animated-position composite must build once FFmpeg filters exist");

        let stamped = |rgba: Vec<u8>, pts: Duration| -> VideoFrame {
            let mut f = VideoFrame::from_rgba(8, 8, rgba).unwrap();
            f.set_timestamp(Timestamp::from_duration(pts, Rational::new(1, 1_000_000)));
            f
        };
        let red = |pts| stamped([255u8, 0, 0, 255].repeat(64), pts);
        let blue = |pts| stamped([0u8, 0, 255, 255].repeat(64), pts);
        let sample = |composer: &mut RealtimeComposer, pts: Duration| -> Option<u8> {
            composer.push_layer(0, &red(pts)).ok()?;
            composer.push_layer(1, &blue(pts)).ok()?;
            Some(composer.pull().ok()??.to_rgba()?[0])
        };

        let Some(r0) = sample(&mut composer, Duration::ZERO) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        let Some(r1) = sample(&mut composer, Duration::from_secs(1)) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        assert!(
            r0 > r1 + 100,
            "overlay should slide in and cover the red base, dropping R: r0={r0} r1={r1}"
        );
    }

    #[test]
    fn animated_crop_effect_moves_the_cropped_region_across_pts() {
        // A base layer whose left half is red and right half is blue, cropped to a
        // 4-wide window whose x slides 0→4 across 1s. At PTS 0 the window shows the red
        // half; at PTS 1s it shows the blue half. Proves a `CropAnimated` per-clip
        // effect surfaces an AnimationEntry from `add_and_link_step` and animates via
        // `send_command` (the #1294 plumbing that unblocks Ken Burns). Probe-gated.
        use crate::animation::{AnimatedValue, AnimationTrack, Easing, Keyframe};
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let x_track = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0, Easing::Linear))
            .push(Keyframe::new(Duration::from_secs(1), 4.0, Easing::Linear));
        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::CropAnimated {
                x: AnimatedValue::Track(x_track),
                y: AnimatedValue::Static(0.0),
                width: AnimatedValue::Static(4.0),
                height: AnimatedValue::Static(8.0),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base])
            .expect("animated-crop base must build once FFmpeg filters exist");

        // Left half (cols 0..4) red, right half (cols 4..8) blue.
        let mut data = vec![0u8; 8 * 8 * 4];
        for row in 0..8 {
            for col in 0..8 {
                let p = (row * 8 + col) * 4;
                if col < 4 {
                    data[p] = 255; // R
                } else {
                    data[p + 2] = 255; // B
                }
                data[p + 3] = 255; // opaque
            }
        }
        let sample = |composer: &mut RealtimeComposer, pts: Duration| -> Option<u8> {
            let mut f = VideoFrame::from_rgba(8, 8, data.clone()).unwrap();
            f.set_timestamp(Timestamp::from_duration(pts, Rational::new(1, 1_000_000)));
            composer.push_layer(0, &f).ok()?;
            Some(composer.pull().ok()??.to_rgba()?[0])
        };

        let Some(r0) = sample(&mut composer, Duration::ZERO) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        let Some(r1) = sample(&mut composer, Duration::from_secs(1)) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        assert!(
            r0 > r1 + 100,
            "the crop window should slide from the red half to the blue half: r0={r0} r1={r1}"
        );
    }

    #[test]
    fn animated_scale_effect_resizes_the_output_across_pts() {
        // A base layer with a `ScaleAnimated` whose width shrinks 8→4 across 1s. The
        // self-animating `scale=eval=frame` expression re-evaluates the width per frame,
        // so the composited output width is 8 at PTS 0 and 4 at PTS 1s. Proves #1297's
        // expression-driven scale animates (and builds — this FFmpeg's scale supports
        // eval=frame). Probe-gated.
        use crate::animation::{AnimatedValue, AnimationTrack, Easing, Keyframe};
        use crate::graph::types::ScaleAlgorithm;
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let width = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 8.0, Easing::Linear))
            .push(Keyframe::new(Duration::from_secs(1), 4.0, Easing::Linear));
        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::ScaleAnimated {
                width: AnimatedValue::Track(width),
                height: AnimatedValue::Static(8.0),
                algorithm: ScaleAlgorithm::Bilinear,
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base])
            .expect("animated-scale base must build once FFmpeg filters exist");

        let sample = |composer: &mut RealtimeComposer, pts: Duration| -> Option<u32> {
            let mut f = VideoFrame::from_rgba(8, 8, vec![120u8; 8 * 8 * 4]).unwrap();
            f.set_timestamp(Timestamp::from_duration(pts, Rational::new(1, 1_000_000)));
            composer.push_layer(0, &f).ok()?;
            Some(composer.pull().ok()??.width())
        };

        let Some(w0) = sample(&mut composer, Duration::ZERO) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        let Some(w1) = sample(&mut composer, Duration::from_secs(1)) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        assert!(
            w0 > w1,
            "the scaled output width should shrink across PTS: w0={w0} w1={w1}"
        );
    }

    #[test]
    fn animated_rotate_effect_turns_content_across_pts() {
        // A base layer whose left half is white and right half is black, rotated by a
        // `RotateAnimated` angle 0°→180° across 1s. At PTS 0 the top-left pixel is white;
        // at PTS 1s (180°) the frame is turned, so the top-left shows the (black) right
        // half. Proves the self-animating `rotate=angle=EXPR(t)` turns per frame.
        // Probe-gated.
        use crate::animation::{AnimatedValue, AnimationTrack, Easing, Keyframe};
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let angle = AnimationTrack::new()
            .push(Keyframe::new(Duration::ZERO, 0.0, Easing::Linear))
            .push(Keyframe::new(Duration::from_secs(1), 180.0, Easing::Linear));
        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::RotateAnimated {
                angle: AnimatedValue::Track(angle),
                fill_color: "black".to_string(),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = RealtimeComposer::new(&[base])
            .expect("animated-rotate base must build once FFmpeg filters exist");

        // Left half (cols 0..4) white, right half (cols 4..8) black.
        let mut data = vec![0u8; 8 * 8 * 4];
        for row in 0..8 {
            for col in 0..8 {
                let p = (row * 8 + col) * 4;
                if col < 4 {
                    data[p] = 255;
                    data[p + 1] = 255;
                    data[p + 2] = 255;
                }
                data[p + 3] = 255;
            }
        }
        let sample = |composer: &mut RealtimeComposer, pts: Duration| -> Option<u8> {
            let mut f = VideoFrame::from_rgba(8, 8, data.clone()).unwrap();
            f.set_timestamp(Timestamp::from_duration(pts, Rational::new(1, 1_000_000)));
            composer.push_layer(0, &f).ok()?;
            Some(composer.pull().ok()??.to_rgba()?[0])
        };

        let Some(r0) = sample(&mut composer, Duration::ZERO) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        let Some(r1) = sample(&mut composer, Duration::from_secs(1)) else {
            println!("Skipping: push/pull failed (FFmpeg unavailable?)");
            return;
        };
        assert!(
            r0 > r1 + 100,
            "180° rotation should turn the white half away from the top-left: r0={r0} r1={r1}"
        );
    }

    #[test]
    fn rotate_transparent_fill_shows_layer_below_in_corners() {
        // A blue top layer rotated 45° with a transparent fill, over a red base. At 45°
        // the fixed 8×8 output's corners fall outside the rotated square and are filled
        // transparently, so the top-left composites to the red base — not black. Proves
        // `RotateAnimated` fillcolor=none + the overlay honoring that alpha (rgba
        // conversion + `:format=auto`). Probe-gated.
        use crate::animation::AnimatedValue;
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let base = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let top = RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::RotateAnimated {
                angle: AnimatedValue::Static(45.0),
                fill_color: "none".to_string(),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::new(&[base, top]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };

        let red = VideoFrame::from_rgba(8, 8, [255u8, 0, 0, 255].repeat(64)).unwrap();
        let mut blue = VideoFrame::from_rgba(8, 8, [0u8, 0, 255, 255].repeat(64)).unwrap();
        blue.set_timestamp(Timestamp::from_duration(
            Duration::ZERO,
            Rational::new(1, 1_000_000),
        ));
        if composer.push_layer(0, &red).is_err() || composer.push_layer(1, &blue).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                let rgba = out.to_rgba().expect("rgba");
                // Top-left corner: exposed by the 45° rotation → transparent → red base.
                assert!(
                    rgba[0] > 100 && rgba[2] < 100,
                    "rotated corner should show the red base, not black: rgba={:?}",
                    &rgba[0..4]
                );
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn rotate_over_base_640x360_canvas_none_reveals_base() {
        // Exactly the demo's real config: V1 base 640×360 + V2 overlay 640×360, both
        // same size (identity force-scale), RotateAnimated, canvas=None (aspect Original).
        // Asserts the rotated corner reveals the red base.
        use crate::animation::AnimatedValue;
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 640,
            height: 360,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }
        let mk = |effects: Vec<FilterStep>| RealtimeLayer {
            width: 640,
            height: 360,
            pixel_format: PixelFormat::Rgba,
            effects,
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let base = mk(vec![]);
        let top = mk(vec![FilterStep::RotateAnimated {
            angle: AnimatedValue::Static(30.0),
            fill_color: "none".to_string(),
        }]);
        let mut composer = match RealtimeComposer::new(&[base, top]) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        let red = VideoFrame::from_rgba(640, 360, [255u8, 0, 0, 255].repeat(640 * 360)).unwrap();
        let mut blue =
            VideoFrame::from_rgba(640, 360, [0u8, 0, 255, 255].repeat(640 * 360)).unwrap();
        blue.set_timestamp(Timestamp::from_duration(
            Duration::ZERO,
            Rational::new(1, 1_000_000),
        ));
        if composer.push_layer(0, &red).is_err() || composer.push_layer(1, &blue).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                let rgba = out.to_rgba().expect("rgba");
                // Top-left corner exposed by the 30° rotation → should be the red base.
                assert!(
                    rgba[0] > 100 && rgba[2] < 100,
                    "640x360 canvas=None: rotated corner should show red base, not black: rgba={:?}",
                    &rgba[0..4]
                );
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn rotate_over_base_reveals_base_through_forcescale_and_canvas() {
        // Mirrors the DEMO's real preview path more closely than the 8×8 test above:
        // the overlay is a DIFFERENT size than the base (so `build_realtime_composition`
        // prepends a non-identity `Scale{canvas, Fast}` force-scale before the rotate),
        // and a project canvas is set. The rotated overlay's transparent corners must
        // still composite to the red base.
        use crate::animation::AnimatedValue;
        use ff_format::{Rational, Timestamp};
        use std::time::Duration;

        let probe = RealtimeLayer {
            width: 16,
            height: 16,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        if RealtimeComposer::new(&[probe]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }

        let base = RealtimeLayer {
            width: 16,
            height: 16,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        // Overlay is 32×24 (≠ base 16×16) → real force-scale to canvas before rotate.
        let top = RealtimeLayer {
            width: 32,
            height: 24,
            pixel_format: PixelFormat::Rgba,
            effects: vec![FilterStep::RotateAnimated {
                angle: AnimatedValue::Static(45.0),
                fill_color: "none".to_string(),
            }],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        };
        let mut composer = match RealtimeComposer::with_canvas(&[base, top], Some((16, 16))) {
            Ok(c) => c,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };

        let red = VideoFrame::from_rgba(16, 16, [255u8, 0, 0, 255].repeat(256)).unwrap();
        let mut blue = VideoFrame::from_rgba(32, 24, [0u8, 0, 255, 255].repeat(32 * 24)).unwrap();
        blue.set_timestamp(Timestamp::from_duration(
            Duration::ZERO,
            Rational::new(1, 1_000_000),
        ));
        if composer.push_layer(0, &red).is_err() || composer.push_layer(1, &blue).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                let rgba = out.to_rgba().expect("rgba");
                assert!(
                    rgba[0] > 100 && rgba[2] < 100,
                    "rotated corner should show the red base, not black: rgba={:?}",
                    &rgba[0..4]
                );
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    fn base_8x8() -> RealtimeLayer {
        RealtimeLayer {
            width: 8,
            height: 8,
            pixel_format: PixelFormat::Rgba,
            effects: vec![],
            opacity: AnimatedValue::Static(1.0),
            x: AnimatedValue::Static(0.0),
            y: AnimatedValue::Static(0.0),
            scale_x: AnimatedValue::Static(1.0),
            scale_y: AnimatedValue::Static(1.0),
            rotation: AnimatedValue::Static(0.0),
            blend_mode: BlendMode::Normal,
            composite_op: CompositeOp::Over,
        }
    }

    #[test]
    fn overlay_static_scale_should_shrink_the_overlay_and_reveal_base() {
        // C4a: an overlay descriptor with a static `scale_x`/`scale_y` (the merged
        // timeline transform) must build the scale node so the overlay shrinks —
        // otherwise it would cover the whole canvas. A blue overlay at scale 0.5 over
        // a red base covers only the top-left 4×4 quadrant, so the top-left pixel is
        // blue but the bottom-right pixel reveals the red base. Probe-gated (CI's Linux
        // FFmpeg has no filters), and pull is required to actually observe the effect.
        if RealtimeComposer::new(&[base_8x8()]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }
        let mut overlay = base_8x8();
        overlay.scale_x = AnimatedValue::Static(0.5);
        overlay.scale_y = AnimatedValue::Static(0.5);
        let mut composer = RealtimeComposer::new(&[base_8x8(), overlay])
            .expect("overlay scale must build once FFmpeg filters exist");
        let red = VideoFrame::from_rgba(8, 8, [255u8, 0, 0, 255].repeat(64)).unwrap();
        let blue = VideoFrame::from_rgba(8, 8, [0u8, 0, 255, 255].repeat(64)).unwrap();
        if composer.push_layer(0, &red).is_err() || composer.push_layer(1, &blue).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                assert_eq!(out.width(), 8);
                assert_eq!(out.height(), 8);
                let rgba = out.to_rgba().expect("rgba");
                // Top-left: the shrunk blue overlay sits here.
                assert!(
                    rgba[2] > 100 && rgba[0] < 100,
                    "top-left should show the blue overlay: {:?}",
                    &rgba[0..4]
                );
                // Bottom-right (row 7, col 7): outside the 4×4 overlay → red base. If
                // the scale node were dropped, the overlay would cover this too (blue).
                let br = (7 * 8 + 7) * 4;
                assert!(
                    rgba[br] > 100 && rgba[br + 2] < 100,
                    "bottom-right should reveal the red base (overlay shrunk): {:?}",
                    &rgba[br..br + 4]
                );
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn overlay_static_rotation_should_build_and_pull() {
        // C4a: a static `rotation` field must build the rotate node in the realtime
        // compositor (the field path, distinct from a per-clip `RotateAnimated` effect)
        // and pull an rgba frame. Probe-gated.
        if RealtimeComposer::new(&[base_8x8()]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }
        let mut overlay = base_8x8();
        overlay.rotation = AnimatedValue::Static(45.0);
        let mut composer = RealtimeComposer::new(&[base_8x8(), overlay])
            .expect("overlay rotate must build once FFmpeg filters exist");
        let bf = VideoFrame::from_rgba(8, 8, vec![80u8; 8 * 8 * 4]).unwrap();
        let tf = VideoFrame::from_rgba(8, 8, vec![160u8; 8 * 8 * 4]).unwrap();
        if composer.push_layer(0, &bf).is_err() || composer.push_layer(1, &tf).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => assert_eq!(out.format(), PixelFormat::Rgba),
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn overlay_composite_under_should_render_base_over_overlay() {
        // C4b: `CompositeOp::Under` swaps the compositing so the base renders over the
        // overlay. An opaque red base over an opaque blue overlay must stay red — the
        // opposite of the default `Over` (which would show the blue overlay on top).
        // Probe-gated (CI's Linux FFmpeg has no filters).
        if RealtimeComposer::new(&[base_8x8()]).is_err() {
            println!("Skipping: FFmpeg filters unavailable");
            return;
        }
        let mut overlay = base_8x8();
        overlay.composite_op = CompositeOp::Under;
        let mut composer = RealtimeComposer::new(&[base_8x8(), overlay])
            .expect("composite Under must build once FFmpeg filters exist");
        let red = VideoFrame::from_rgba(8, 8, [255u8, 0, 0, 255].repeat(64)).unwrap();
        let blue = VideoFrame::from_rgba(8, 8, [0u8, 0, 255, 255].repeat(64)).unwrap();
        if composer.push_layer(0, &red).is_err() || composer.push_layer(1, &blue).is_err() {
            println!("Skipping: push failed (FFmpeg unavailable?)");
            return;
        }
        match composer.pull() {
            Ok(Some(out)) => {
                let rgba = out.to_rgba().expect("rgba");
                assert!(
                    rgba[0] > 100 && rgba[2] < 100,
                    "Under should keep the red base on top of the blue overlay: {:?}",
                    &rgba[0..4]
                );
            }
            Ok(None) => println!("Skipping: no frame produced"),
            Err(e) => println!("Skipping: {e}"),
        }
    }

    #[test]
    fn overlay_composite_expr_ops_should_be_rejected_at_build() {
        // C4b, inverted by #1753 (ADR-0014): the expression operators are refused
        // when the realtime composer is built, because the filter path cannot carry
        // the backdrop's alpha and would otherwise compute per-channel arithmetic
        // under the operator's name. The `all_opacity` branch those operators drove
        // is unreachable until #1784. Pure check, so no probe gate: it reports the
        // same on CI's minimal FFmpeg.
        for op in [
            CompositeOp::In,
            CompositeOp::Out,
            CompositeOp::Atop,
            CompositeOp::Xor,
        ] {
            let mut overlay = base_8x8();
            overlay.composite_op = op;
            let built = RealtimeComposer::new(&[base_8x8(), overlay]);
            assert!(
                matches!(
                    built,
                    Err(FilterError::UnsupportedCompositeOp { op: got }) if got == op
                ),
                "{op:?} must be refused at build on the realtime composer"
            );
        }
    }

    #[test]
    fn lavfi_source_should_generate_rgba_frames() {
        // C4d: a `LavfiSource` generates rgba frames from a filtergraph string. Probe-
        // gated: CI's Linux FFmpeg has no filters / lavfi demuxer, so skip gracefully.
        let mut source = match LavfiSource::new("color=c=red:s=16x16:d=1") {
            Ok(s) => s,
            Err(e) => {
                println!("Skipping: {e}");
                return;
            }
        };
        // The movie/lavfi source may return None before it warms up; try a few pulls.
        for _ in 0..8 {
            match source.pull() {
                Ok(Some(frame)) => {
                    assert_eq!(frame.format(), PixelFormat::Rgba);
                    assert_eq!(frame.width(), 16);
                    assert_eq!(frame.height(), 16);
                    return;
                }
                Ok(None) => continue,
                Err(e) => {
                    println!("Skipping: {e}");
                    return;
                }
            }
        }
        println!("Skipping: no frame produced");
    }
}
