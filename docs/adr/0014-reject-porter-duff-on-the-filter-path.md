---
status: "accepted"
date: 2026-09-06
decision-makers: itsakeyfut
---

# The filter path refuses `In`/`Out`/`Atop`/`Xor` until it can carry alpha

## Context and Problem Statement

`ff_filter::CompositeOp` has six operators. `Over` and `Under` are built with the `overlay`
filter and are real alpha compositing on both paths. `In`, `Out`, `Atop` and `Xor` were built
with `blend`'s `all_expr` on inputs normalised to `yuv420p`, a format with no alpha plane, so
what the filter path computed was the Porter-Duff formula with each colour channel standing in
for alpha: `In` was a per-channel multiply. That has been so since `CompositeOp` shipped in
v0.17.0 (#1221).

v0.18.0 made the GPU compositor implement the W3C Compositing Level 1 definitions for all six
(#1670). From then on the two paths **disagreed silently**: the same timeline rendered one way
with an adapter and another whenever the filter path ran, which is a headless machine, a
`force_cpu` run, or a frame the GPU declined for an unrelated reason. Preview and export both
composite on the GPU, so this is not a preview-versus-export difference; it is a
machine-dependent one.

The maintainer ruled that no silent wrong output ships in v0.18.0 (#1753). The filter-path
implementation that would make the four operators correct is milestone-sized, so this record
decides what the filter path does **until** that lands.

## Decision Drivers

* No silent wrong output: a user who asks for `In` must not receive a multiply with no error and
  no log line.
* The correct implementation is not a patch. Carrying alpha through the filter chain means
  replacing the `yuv420p` normalisation with `yuva420p`, an `alphaextract` / `alphamerge` pair
  around the blend, and a re-validation of every CPU parity test that rides on that
  normalisation. That is #1784.
* ADR-0007 names the CPU compositor as the correctness reference. For these four it is the GPU
  that is correct, so whatever the filter path does in the meantime must not be mistaken for the
  reference behaviour.
* Direct users of `ff-filter` (no engine, no GPU) must be covered too; the wrong maths lives in
  the primitive, not in `avio`.

## Considered Options

* **Implement the real operators now** on the filter path (#1784's scope) before releasing.
* **Document the divergence** in the type docs and the changelog and ship the arithmetic as is.
* **Refuse the four operators** on the filter path with a dedicated error, and refuse a timeline
  that needs them at the engine's open when no GPU compositor is attached.

## Decision Outcome

Chosen option: **refuse**, because it removes the silent wrong output at a cost proportional to
the gap rather than to the fix, and because the failure it introduces is loud, early and named.

What "refuse" means concretely:

* `FilterError::UnsupportedCompositeOp { op }` is returned at **build time** by every filter-path
  entry point: `FilterGraphBuilder::build()` for a `FilterStep::Composite` (recursing into the
  nested `Blend` / `Composite` / `AlphaMatte` chains), `MultiTrackComposer::build()` and
  `RealtimeComposer::new` / `with_canvas` for any layer, and `add_composite_step` itself as the
  shared construction, so no future caller can reach the per-channel arithmetic.
  `CompositeOp::is_filter_path_supported` is the single definition all four sites consult.
* The check is **pure**: it runs before the filter-name lookup and consults no registry, so it
  reports identically on a minimal `FFmpeg` build. This is the second step kind validated
  eagerly at `build()` (the first is `parse_desc`, ADR-0012) and, unlike that one, it needs no
  probe gate in tests. `docs/rules/test.md` and `docs/rules/perf.md` say so.
* `avio::TimelinePlayer::open` returns `PreviewError::NeedsGpuCompositor` when no GPU compositor
  could be attached (no adapter, or `open_forcing_cpu`) and any clip uses one of the four. This
  exists because the runner's own behaviour on a compositor build failure is to show the base
  frame with the layer missing, which would have turned "wrong blend" into "overlay vanishes",
  still silently. Export needs no such check: `Timeline::render` already propagates the
  composer's build error.
* Every layer counts, the base layer included. The filter path used to ignore the base layer's
  operator, while the GPU applies it against an empty backdrop, so a base-layer `In` was itself
  a silent divergence (blank on the GPU, the base frame on the CPU).

The one residual: with a GPU attached, a frame the GPU declines for an *unrelated* reason falls
to the filter path, which refuses it, and the runner shows the base frame. That case logs a
warning once per layer set and is documented in the bridge spec. The whole-frame fallback is not
triggered by these operators themselves, but it is by an effect with no GPU node, by an overlay
that spills outside the base, and by a rotated overlay, so a partly off-screen `Atop` layer meets
it on a machine with an adapter too. It disappears with #1784.

### Confirmation

Each gate has a test that goes red when that gate alone is removed:

* `composite_expression_operators_should_be_rejected_at_build` and
  `composite_expression_operator_nested_in_a_top_chain_should_be_rejected_at_build`
  (`crates/ff-filter/tests/push_pull_tests.rs`) for `FilterGraphBuilder::build()`, plus
  `composite_under_should_not_be_rejected_at_build` so the refusal is proven selective.
* `composite_op_in_layer_should_be_rejected_at_build` (`crates/ff-filter/tests/composition_tests.rs`)
  for `MultiTrackComposer::build()`.
* `overlay_composite_expr_ops_should_be_rejected_at_build`
  (`crates/ff-filter/src/graph/composition/realtime_composer.rs`) for `RealtimeComposer::new`.
* `crates/avio/tests/composite_op_gate.rs` for the engine: `render_forcing_cpu` on an `Atop`
  overlay is `Filter(UnsupportedCompositeOp)`, `open_forcing_cpu` is `NeedsGpuCompositor`, `open`
  with an adapter succeeds, and `Under` is still accepted.

Measured by knocking each gate out in turn: removing the builder check turns the two builder
tests red; removing the engine check turns the preview test red; removing the composer
pre-checks turns both composer tests red **on their own**, because the composers wrap any error
from the shared construction as `CompositionFailed`, so the inner guard cannot supply the right
variant there. That makes the pre-checks load-bearing for the error's *type*, not only for
failing before an allocation. The guard inside `add_composite_step` is the one gate no test
observes: with the entry-point gates in place it is unreachable, and it exists to keep the
invariant local rather than to be seen.

### Consequences

* Good, because nothing on the filter path renders `In`/`Out`/`Atop`/`Xor` wrongly any more, and
  the error names the operator and the issue that will implement it.
* Good, because the refusal is a pure check, so CI's minimal `FFmpeg` build exercises it too.
* Bad, because a timeline using these operators cannot be previewed or exported on a machine
  without a GPU adapter until #1784. That was already true in the sense that mattered: it could
  be rendered, but not correctly.
* Bad, because this is a behaviour change to a v0.17.0 feature (pre-1.0; the operators were
  never correct on that path, so nothing that relied on the output was relying on Porter-Duff).
* What would reverse this: #1784 landing. It removes `UnsupportedCompositeOp`'s reasons for
  existing, deletes the engine's open-time check, and must reconcile ADR-0007's "the CPU is the
  reference" for these four, since for them the GPU became the reference first.

## Pros and Cons of the Options

### Implement now

* Good, because it is the real fix and there is no residual.
* Bad, because it changes the pixel format of the shared composite chain, which every CPU parity
  test rides on, and needs a design decision shared with #1783 (the canvas background), so it is
  a milestone theme rather than a pre-release change.

### Document only

* Good, because it costs nothing and changes no behaviour.
* Bad, because the output is still wrong and still silent; a note in the type docs does not reach
  someone looking at a rendered frame.

### Refuse (chosen)

* Good, because the failure is loud, early, named, and testable without an adapter.
* Bad, because it withdraws a capability from CPU-only machines until #1784.

## More Information

* Issues #1753 (this decision's scope), #1784 (the filter-path implementation that lifts it),
  #1783 (the canvas background, which shares the alpha design), #1670 (the GPU operators),
  #1221 (where `CompositeOp` shipped).
* [ADR-0007](./0007-gpu-compositing-bridge.md) for the bridge and its "CPU is the reference"
  premise; [ADR-0012](./0012-chain-description-escape-hatch.md) for the first eager `build()`
  check this one sits beside.
* Code: `crates/ff-filter/src/composite.rs` (`is_filter_path_supported`, `blend_all_expr`),
  `crates/ff-filter/src/filter_inner/mod.rs` (`validate_composite_ops`),
  `crates/ff-filter/src/graph/composition/composition_inner.rs` (both eager builders),
  `crates/ff-filter/src/filter_inner/build.rs` (`add_composite_step`),
  `crates/ff-preview/src/scene/runner.rs` (the once-per-layer-set warning),
  `crates/avio/src/player.rs` (`cpu_compositor_refusal`).
* `docs/specs/gpu-compositing-bridge.md` composite row states the outcome and links here.
