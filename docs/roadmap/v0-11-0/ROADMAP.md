# v0.11.0 — Compositing, Keying & Blend Modes

**Goal**: Provide professional compositing capabilities — blend modes, chroma/luma keying, alpha operations, and masking — enabling green-screen removal, motion graphics layering, and any effect that requires per-pixel transparency or layer interaction.

**Prerequisite**: v0.10.0 complete.

**Crates in scope**: `ff-filter`

---

## Requirements

### Blend Modes

The following blend modes can be applied when compositing two video layers (base + blend):

- **Normal** — standard alpha-over compositing
- **Multiply** — darkens; multiplies base and blend pixel values
- **Screen** — lightens; inverse of multiply
- **Overlay** — combines multiply and screen depending on base luminance
- **Soft Light** — a gentler version of overlay
- **Hard Light** — a harsher version of overlay
- **Color Dodge** — brightens the base by the blend
- **Color Burn** — darkens the base by the blend
- **Darken** — retains the darker of the two pixels per channel
- **Lighten** — retains the lighter of the two pixels per channel
- **Difference** — absolute difference per channel (useful for alignment)
- **Exclusion** — similar to difference, lower contrast
- **Add** — linear add (clipped at maximum)
- **Subtract** — linear subtract (clipped at minimum)
- **Hue** — hue from blend, saturation and luminance from base
- **Saturation** — saturation from blend, hue and luminance from base
- **Color** — hue and saturation from blend, luminance from base
- **Luminosity** — luminance from blend, hue and saturation from base

Each blend mode is usable independently of keying — i.e., blending two fully opaque layers.

### Porter-Duff Alpha Compositing

The following Porter-Duff compositing operations are available for layers that carry an alpha channel:

- **Over** — blend layer rendered over base (standard)
- **Under** — blend layer rendered under base
- **In** — blend layer masked by base alpha
- **Out** — blend layer masked by inverse of base alpha
- **Atop** — blend layer placed on top, visible only where base is opaque
- **XOR** — only pixels where exactly one layer is opaque are shown

### Chroma Key (Green Screen / Blue Screen)

- A chroma key can be applied to remove a solid-color background from a video layer, outputting an alpha channel for subsequent compositing.
- Key color can be specified as an RGB hex value or sampled from a reference pixel coordinate.
- Similarity tolerance (how broadly the key color is matched) is configurable.
- Blend (softness of the key edge) is configurable, preventing hard aliased edges.
- Spill suppression is available to reduce green/blue color cast on the subject.
- The output is an RGBA video that can be composited over any background using the Porter-Duff `over` operation.

### Luma Key

- A luma key can be applied to make a video layer transparent in bright or dark regions.
- Threshold (luminance cutoff) and softness are configurable.
- Both "key out bright" and "key out dark" modes are supported.
- Common use case: white-background graphics, title cards, and lower-third overlays.

### Alpha Key

- A separate grayscale video (or image) can be used as an external alpha matte for any video layer.
- Both straight alpha and premultiplied alpha inputs are handled correctly.
- The matte can be inverted.

### Masking & Garbage Mattes

- A rectangular mask can be applied to any video layer to isolate a region of interest before keying or compositing.
- A simple polygon mask (up to 16 vertices) is supported for garbage matte use (rough isolation of the subject area before chroma keying).
- Mask edges can be feathered (soft falloff) to avoid hard borders.

### Compositing Pipeline Integration

- All of the above operations compose freely in a single `ff-filter` graph — for example: rectangular garbage matte → chroma key → blend mode composite over background.
- The compositing API is consistent with the existing filter builder pattern: each operation is a chainable step.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Blend modes | Implemented via libavfilter `blend` filter with `all_mode` option |
| Chroma key | `chromakey` filter (YCbCr) and `colorkey` filter (RGB) — both exposed |
| Luma key | `lumakey` filter |
| Alpha matte | `alphamerge` filter |
| Porter-Duff ops | `overlay` filter with `format=auto`; XOR via custom expression where needed |
| Masking | `crop` for rectangles; `drawbox` + `alphaextract` chain for polygon approximation |
| Pixel format | All compositing operations work on `yuva420p` / `rgba`; format conversion is automatic |

---

## Definition of Done

- Green screen removal test: subject keyed cleanly over a solid-color background
- All 18 blend modes produce visually correct output verified against reference images
- Porter-Duff `over` and `in` operations produce correct alpha-composite results
- Luma key test: white-background title card composited over video
- Mask + chroma key pipeline integration test passes
