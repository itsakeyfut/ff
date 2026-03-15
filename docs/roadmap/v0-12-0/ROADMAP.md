# v0.12.0 — Keyframe Animation

**Goal**: Provide a time-varying property system so that any filter parameter, overlay position, volume level, or color correction value can be animated over time — enabling fades, zooms, motion graphics, dynamic color grading, and audio automation.

**Prerequisite**: v0.11.0 complete.

**Crates in scope**: `ff-filter`, `ff-encode`

---

## Requirements

### Property Tracks

- Any numeric filter parameter can be driven by a property track: a list of (timestamp, value) keyframe pairs that define how the property changes over time.
- The following property types are supported as animatable values:
  - `f64` — for scalar values (opacity, volume, scale, rotation angle, CRF offset, blur radius, etc.)
  - `(f64, f64)` — for 2D position or size (x/y coordinates, width/height)
  - `(f64, f64, f64)` — for 3D color values (RGB lift/gamma/gain, white balance)
- Keyframes can be set at any timestamp with millisecond precision.
- Property tracks are sparse: only keyframes that differ from the previous value need to be specified.

### Easing / Interpolation Functions

The following interpolation modes are available between any two adjacent keyframes:

- **Hold** — value is constant until the next keyframe (step function)
- **Linear** — linear interpolation between keyframe values
- **Ease In** — slow start, fast end (cubic)
- **Ease Out** — fast start, slow end (cubic)
- **Ease In-Out** — slow start and end, fast middle (cubic)
- **Bezier** — full cubic Bézier with two user-defined control points (matching the model used by After Effects, Premiere Pro, and Final Cut Pro)

Each keyframe independently specifies the easing for the segment that follows it.

### Animatable Video Properties

The following video properties can be animated via property tracks:

- Layer **opacity** (0.0–1.0)
- Layer **position** (x, y) in pixels or as a fraction of output canvas
- Layer **scale** (x, y) independently
- Layer **rotation** (degrees)
- **Crop** rectangle (x, y, width, height)
- **Blur** radius
- Color correction **brightness**, **contrast**, **saturation**
- Color correction **lift**, **gamma**, **gain** (per channel RGB)
- **Volume** (audio gain in dB) — see audio section below
- Any scalar `av_opt_set` parameter exposed by a filter

### Animatable Audio Properties

- Per-track **volume** automation (dB over time)
- Per-track **pan** automation (stereo position −1.0 to +1.0)
- **Fade in** and **fade out** are expressible as a volume track with two keyframes (common shorthand API)

### Animation Data Model

- An `AnimationTrack<T>` type holds a sorted list of `Keyframe<T>` values and provides `value_at(timestamp: Duration) -> T` interpolation.
- `AnimationTrack<T>` is serializable to/from a plain Rust data structure (no external format dependency at this stage), enabling applications to save and restore animation data.
- Multiple animation tracks can be attached to a single filter node, each controlling a different property.
- A `Timeline` helper type bundles a list of clips and their associated animation tracks, providing a higher-level interface for construction — without prescribing a full NLE data model.

### Integration with Filter Graph

- Animated property values are applied frame-by-frame when the filter graph is evaluated.
- The animation engine evaluates all active tracks for each frame's presentation timestamp and passes the interpolated values to libavfilter via `av_opt_set` or expression strings (`sendcmd` / `modify_graph`).
- Animation has no measurable per-frame overhead beyond the interpolation evaluation itself.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Interpolation engine | Implemented entirely in Rust — no FFmpeg dependency for the math |
| Filter graph injection | Animated values passed to libavfilter via `avfilter_graph_send_command` per frame |
| Bezier control points | Stored as normalized (0–1) in/out handles matching the After Effects convention |
| Serialization | `AnimationTrack` derives `serde::Serialize/Deserialize` behind a `serde` feature flag |
| Timeline type | Convenience wrapper only — library does not enforce a particular composition data model |

---

## Definition of Done

- Opacity fade from 1.0 to 0.0 over 2 seconds produces correct per-frame alpha values
- Bezier eased position animation matches reference output from an After Effects export
- Volume automation test: gain ramps from −∞ dB to 0 dB over the first second of audio
- `value_at()` unit tests cover all six easing modes
- Frame-accurate evaluation verified: keyframe at t=1.000s applies correctly on the frame at that timestamp
