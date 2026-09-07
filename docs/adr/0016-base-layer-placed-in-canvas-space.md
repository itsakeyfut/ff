---
status: "accepted"
date: 2026-09-07
decision-makers: itsakeyfut
---

# Every layer, the base included, is placed in canvas space on every route

## Context and Problem Statement

A clip's `x` / `y` / `scale` / `rotation` are model properties with one documented meaning:
position in canvas pixels, a scale factor, clockwise degrees, and `FitMode::None` (the default)
leaving the source at its native size with those transforms applied. The four routes that render a
timeline did not agree on what they mean for the **bottom** layer. Measured at the engine level on a
single clip with an explicit 64x64 canvas:

| fixture | export CPU | export GPU | preview CPU | preview GPU |
|---|---|---|---|---|
| 64x64 at `(10, 4)`, scale `0.5` | `(10,4)-(41,35)` | full frame | full frame | full frame |
| 64x32, no transform | `(0,0)-(63,31)`, native | `(0,16)-(63,47)`, letterboxed | letterboxed | letterboxed |
| 64x32 at `(10, 4)`, scale `0.5` | `(10,4)-(41,35)` | letterboxed, ignored | letterboxed, ignored | letterboxed, ignored |

The CPU export composer overlays every layer, layer 0 included, onto a canvas-sized background at
`(x, y)` after `scale=canvas*s` and `rotate`, and fits nothing. The realtime composer used layer 0
as its accumulator, never placed it, and letterboxed the composite into an explicit canvas; the
GPU compositor reproduced the realtime rule (#1767 measured it there and only there). So the same
timeline rendered one way headless and another with an adapter, and one way in export and another
in preview, with no error and no log line. #1766 asked which behaviour is intended.

## Decision Drivers

* No silent divergence between routes in v0.18.0: an adapter must not change the picture.
* The model's docs are the contract. `FitMode::None` promises native size with transforms
  applied; `Clip::x` said "meaningful for overlay (non-base) layers". One of those had to go.
* A base layer is a common case: most timelines have one clip on one track, and its author sets a
  position or scale expecting it to render.
* ADR-0007 names the CPU as the correctness reference, but there are two CPU composers and they
  disagreed; the reference has to be one of them.

## Considered Options

* **Base defines the frame**: ignore the base's transform everywhere, letterbox it into the
  canvas, change CPU export to match, and make `Clip`'s setters and `validate` say so.
* **Keep the split and document it**: the base is placed in export and not in preview.
* **Canvas space everywhere**: the CPU export construction is the rule; the realtime composer and
  the GPU compositor adopt it.

## Decision Outcome

Chosen option: **canvas space everywhere**, because it is what the model already promises, it is
what CPU export has shipped since v0.17.0, and it removes a capability from nothing.

The rule, for every layer:

* the layer's top-left sits at `(x, y)` canvas pixels and is clipped at the canvas edge (the CPU
  `overlay` writes into a canvas-sized accumulator, the GPU into a canvas-sized target);
* it keeps its native size when `scale == 1.0` exactly and is `canvas * (sx, sy)` otherwise, the
  export construction's rule (its discontinuity at exactly 1.0 against the doc "1.0 = original
  size" is a follow-up, not changed here);
* rotation is clockwise degrees with black in the exposed corners on the CPU; the GPU declines a
  rotated layer (its transform leaves the corners transparent) and the CPU renders it, in
  preview per frame and in export by making the timeline ineligible;
* nothing is fitted or stretched to the canvas by a compositor. Framing is `FitMode`, which the
  derive emits as `FitToAspect` / `FillToAspect` / `Scale` steps for both paths.
* The canvas is always concrete in avio: `Timeline::to_scene` hands the preview the explicit or
  probed size, so an implicit canvas places exactly like an explicit one.

Concretely: `build_realtime_composition` overlays layer 0 onto a hidden canvas-sized `buffersrc`
that `RealtimeComposer` feeds a black frame per tick, through the same `add_blend_normal_step` an
overlay uses, unless the base is an identity, canvas-sized layer whose effects cannot change its
size, in which case it stays the accumulator (the overlay would be a per-frame copy, measured at
4.5 ms per 1080p frame in a release build). Its fit-to-canvas tail is gone.
`gpu_compositor::layer_transform` is one formula for every layer with the canvas as the reference.
`gpu_export::composited_base_layer` neutralises placement along with effects, opacity and blend,
because the base's own pass applies it, and the eligibility check declines a rotated clip and a
cross-fade between placed clips.

### Confirmation

* `crates/avio/tests/base_transform_tests.rs` renders each fixture on all four routes and
  asserts one canvas-sized frame and one lit box per fixture: `(10,4)-(41,35)` for a placed 64x64
  base, explicit or implicit canvas; `(0,0)-(63,31)` for a 64x32 base at scale 1; `(10,4)-(41,35)`
  for a placed 64x32 base (the multiplier is against the canvas); `(0,0)-(63,31)` in a 64x64 frame
  for a 64x32 second clip on an implicit canvas probed from a 64x64 first clip (what tells a
  concrete canvas from a preview that takes the base's own size); black corners on every route for
  a 45 degree rotation.
* `crates/avio/tests/gpu_parity_tests.rs`: `a_positioned_single_layer_gpu_should_match_cpu_within_tolerance`,
  `a_base_smaller_than_the_canvas_gpu_should_match_cpu_at_native_size`,
  `a_base_larger_than_the_canvas_gpu_should_match_cpu_clipped`, the two "over a smaller base"
  placements and `an_overlay_hanging_off_the_canvas_should_clip_like_the_cpu`.
* `crates/avio/tests/gpu_export_tests.rs`: `a_positioned_base_clip_should_export_the_same_on_both_routes`,
  `a_placed_base_under_an_overlay_should_export_the_same_on_both_routes` (the stack pass must not
  place the composited base a second time) and `a_rotated_base_clip_should_take_the_cpu_route`.
* `crates/avio/src/gpu_export.rs` unit tests: `eligible_track_should_reject_a_rotated_clip_and_accept_a_scaled_one`
  and `eligible_tracks_should_reject_a_base_track_transition_between_placed_clips` pin the
  eligibility gate itself (a placed clip stays on the GPU route, a rotated clip or a transition
  between placed base clips leaves it); removing the transition gate turns the latter red.

Measured by knocking each mechanism out in turn: the realtime base never placed, the GPU placing
every layer at the identity, the composited base keeping its placement for the stack pass, and
`to_scene` handing the preview no canvas when the timeline's is implicit each turn the named test
red. The last two needed the overlay and second-clip fixtures above: a lone 64x64 clip on an
implicit 64x64 canvas cannot tell a concrete canvas from the base's own size.
* `crates/ff-filter/src/graph/composition/realtime_composer.rs`: the base placement, scale,
  rotation and position-track tests, and `with_canvas_should_place_the_base_at_native_size_on_the_canvas`.

### Consequences

* Good, because a positioned or scaled single clip renders the same on every route, and a clip
  smaller than the canvas is no longer letterboxed by some routes and cropped by another.
* Good, because the GPU placement formula lost its base-space special case and its spill gate.
* Bad, because the preview changes visibly for a base whose aspect differs from an explicit
  canvas under the default `FitMode::None`: it used to be letterboxed, it now sits at native
  size, as the export always did. `FitMode::Fit` is the opt-in for a band; the CHANGELOG says so.
* Bad, because the CPU preview pays one `overlay` per frame for a base that carries a transform or
  a size-changing effect: 4.5 ms per 1080p frame in a release build on the development machine
  (2.98 ms for the direct chain, 7.46 ms placed). An identity, canvas-sized base whose effects keep
  its size stays on the direct chain and pays nothing.
* Deferred (#1782): the realtime composer still stretches an **overlay** to the canvas before
  `canvas * scale`, which the export does not, so an overlay at scale 1 whose size is not the
  canvas's still differs; a rotated layer on the GPU; and what `ff_preview::Scene.canvas: None`
  means for a standalone `ff-preview` caller (the base's own size, as before).
* What would reverse this: a model-level decision that the bottom track is a canvas rather than a
  layer, at which point `Clip`'s placement would have to be refused there rather than rendered.

## Pros and Cons of the Options

### Base defines the frame

* Good, because it keeps the letterboxed preview many users have seen.
* Bad, because it drops a capability CPU export has had since v0.17.0, contradicts
  `FitMode::None`, and still needs `validate` to refuse placement on the bottom track.

### Keep the split and document it

* Good, because it changes no output.
* Bad, because the output is wrong on three routes and only a doc reader would know which.

### Canvas space everywhere (chosen)

* Good, because one construction serves every layer on every route, and the model's docs are
  already its specification.
* Bad, because the preview's implicit letterbox goes, which is a visible change to announce.

## More Information

* Issues #1766 (this decision), #1782 (the deferred items), #1633 / #1767 (where the GPU
  placement was measured against the realtime composer), #1661 (the letterbox this replaces).
* [ADR-0007](./0007-gpu-compositing-bridge.md) for "the CPU is the reference"; for placement the
  reference is the CPU **export** composer.
* Code: `crates/ff-filter/src/graph/composition/composition_inner.rs` (`build_realtime_composition_unsafe`),
  `crates/ff-filter/src/graph/composition/realtime_composer.rs` (`canvas_slot`, `push_layer`),
  `crates/avio/src/gpu_compositor.rs` (`layer_transform`, `assemble`),
  `crates/avio/src/gpu_export.rs` (`eligible_one_track`, `is_placed`, `composited_base_layer`),
  `crates/avio/src/timeline.rs` (`to_scene`).
