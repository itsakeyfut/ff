# v0.7.0 — Advanced Codec Options & Professional Formats

**Goal**: Expose professional-grade encoding controls for H.264, H.265, AV1, and VP9; add ProRes and DNxHD for broadcast workflows; add HDR/10-bit support; expand container coverage; and support image sequence I/O.

**Prerequisite**: v0.6.0 complete.

**Crates in scope**: `ff-encode`, `ff-decode`, `ff-format`

---

## Requirements

### H.264 Encoding Control

- The H.264 profile can be set: Baseline, Main, High, High10.
- The H.264 level can be set (e.g., 3.1, 4.0, 4.1, 5.0, 5.1).
- The number of B-frames can be controlled (0–16).
- The GOP size (keyframe interval) can be configured.
- The number of reference frames can be set (1–16).
- The encoding preset can be selected: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow, placebo.
- The tune can be set: film, animation, grain, stillimage, fastdecode, zerolatency.
- Arbitrary x264 options can be passed through as raw key-value pairs.

### H.265 / HEVC Encoding Control

- The H.265 profile can be set: Main, Main10, Main Still Picture.
- The tier can be set: Main, High.
- The encoding preset and tune can be configured with the same range as H.264.
- 10-bit output is achievable via the Main10 profile.
- Arbitrary x265 options can be passed through as raw key-value pairs.

### AV1 Encoding Control

- `cpu-used` can be set (0–8) to trade quality for speed.
- Tile rows and columns can be configured for multi-threaded encoding.
- Usage mode can be set: VoD or RealTime.
- Both libaom-av1 and SVT-AV1 encoders are supported (selected by codec name).

### VP9 Encoding Control

- Quality/speed tradeoff can be controlled via `cpu-used` (0–9).
- Constrained quality (CQ) mode and target bitrate mode are both supported.
- Tile configuration is available for throughput.

### Professional Video Codecs

- **ProRes**: encode to ProRes 422, ProRes 422 HQ, ProRes 422 LT, ProRes 422 Proxy, ProRes 4444 — enabling Final Cut Pro and DaVinci Resolve interchange.
- **DNxHD / DNxHR**: encode to DNxHD 36/115/145/220 and DNxHR LB/SQ/HQ/HQX/444 — enabling Avid Media Composer interchange.
- ProRes and DNxHD are decoded on all platforms (no license requirement for decode).

### HDR & Wide Color Gamut

- 10-bit pixel formats are supported for encoding and decoding: `yuv420p10le`, `yuv422p10le`, `yuv444p10le`.
- HDR10 static metadata (MaxCLL, MaxFALL, mastering display color volume) can be embedded into the output container.
- HLG (Hybrid Log-Gamma) color transfer is supported.
- Color space tagging is available: BT.709, BT.2020, DCI-P3.
- Color transfer tagging is available: PQ (SMPTE ST 2084), HLG, linear.

### Audio Codec Options

- **Opus**: application mode (VoIP, audio, low-delay) and frame duration can be configured.
- **AAC**: profile (LC, HE-AAC, HE-AACv2) and VBR quality mode are selectable.
- **MP3**: VBR quality (V0–V9) and CBR bitrate are configurable.
- **FLAC**: compression level (0–12) is configurable; lossless output is guaranteed.
- **PCM**: output sample format can be selected (s16le, s24le, s32le, f32le).

### Container / Format Support

- **MKV (Matroska)**: supports video, audio, multiple subtitle streams, and file attachments (e.g., fonts).
- **WebM**: VP8/VP9/AV1 video with Vorbis/Opus audio.
- **AVI**: H.264 video with AAC or MP3 audio.
- **MOV**: H.264, H.265, and ProRes video with AAC audio.
- **OGG**: Vorbis and Opus audio.
- **FLAC**: standalone lossless audio container.
- Output format is inferred from the file extension when not explicitly specified.

### Image Sequence I/O

- JPEG, PNG, BMP, and TIFF image sequences can be decoded as a video stream (e.g., `frame%04d.png`).
- Any video can be encoded to an image sequence (all frames exported as individual image files).
- The starting frame number and step size for sequence numbering are configurable.
- OpenEXR sequences (`.exr`) can be decoded, enabling VFX and compositing workflows.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Options API | `codec_options()` is an additive builder method — no breaking changes to existing builder |
| av_opt_set | All options set as strings or integers via `av_opt_set`; return value always checked and logged on failure |
| H.265 license | GPL-encumbered encoders (libx265) require the `gpl` feature flag; documented in the README |
| AV1 encoders | `libaom-av1` (LGPL default); SVT-AV1 selected by codec name |
| ProRes encoding | Uses the built-in FFmpeg `prores_ks` encoder; no external dependency |
| HDR metadata | Passed via AVFrameSideData and container-level codec tags |

---

## Definition of Done

- `cargo test -p ff-encode` passes with codec options tests for H.264/H.265/AV1/VP9
- ProRes and DNxHD encode/decode round-trips produce valid output
- WebM output verified playable with VP9 + Opus
- HDR10 metadata survives a round-trip through MKV
- Image sequence decode and encode verified with PNG sequences
- All `av_opt_set` return values are checked and logged on failure
