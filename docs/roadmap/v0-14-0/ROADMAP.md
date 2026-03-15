# v0.14.0 — Advanced Effects & Audio Processing

**Goal**: Add the remaining professional-grade video effects (stabilization, motion blur, film grain, lens correction) and audio processing capabilities (reverb, pitch shift, time stretch, noise reduction, ducking) that are found in commercial NLE tools, alongside color analysis scopes.

**Prerequisite**: v0.13.0 complete.

**Crates in scope**: `ff-filter`, `ff-decode`

---

## Requirements

### Video Stabilization

- Shaky video can be stabilized using a two-pass analysis and correction pipeline:
  - **Pass 1 (analyze)**: motion vectors are extracted and written to an intermediate file.
  - **Pass 2 (stabilize)**: the correction is applied using the motion data, with configurable smoothing strength.
- Stabilization smoothing strength is configurable (0 = no smoothing, 100 = maximum).
- The zoom factor applied to hide border artifacts after stabilization is configurable or can be set to "auto" (minimum required zoom).
- Stabilization is implemented via libvidstab; FFmpeg must be built with `--enable-libvidstab`.

### Motion Blur

- Synthetic motion blur can be added to a video by blending a configurable number of sub-frames (shutter angle simulation).
- Shutter angle (180°, 270°, 360°) and samples per frame (2–16) are configurable.
- Motion blur gives slow-motion footage a cinematic look when the source was captured at high frame rate.

### Film Grain

- Synthetic film grain can be added to video with configurable strength and size.
- Grain is generated per-frame with a different random seed, producing temporally incoherent grain as in real film.
- Chroma grain (color noise) can be enabled or disabled independently of luma grain.

### Lens Distortion Correction

- Barrel and pincushion distortion can be corrected with configurable radial distortion coefficients.
- A lens correction profile (distortion coefficients + crop factor) can be applied to automatically correct for known camera/lens combinations.
- The corrected output can optionally be cropped to remove the black border introduced by the correction.

### Chromatic Aberration

- Lateral chromatic aberration (color fringing at the edges of the frame) can be reduced by independently scaling the R and B color channels relative to G.
- The correction can be applied globally or limited to the outer region of the frame via a radial mask.

### Glow / Bloom

- A glow effect can be applied to bright areas of the frame, simulating lens bloom.
- Glow threshold, radius, and intensity are configurable.
- The effect is composited additively over the original frame.

### Advanced Audio: Reverb

- Convolution reverb can be applied to an audio track using an impulse response (IR) file in WAV or FLAC format.
- Wet/dry mix is configurable (0.0 = dry only, 1.0 = wet only).
- Pre-delay (time before the reverb tail begins) is configurable in milliseconds.
- Built-in algorithmic reverb (using FFmpeg's `aecho` filter) is available as a lightweight alternative when no IR file is provided.

### Advanced Audio: Pitch Shift

- Audio pitch can be shifted by a configurable number of semitones (−12 to +12) without changing the playback speed.
- Pitch shift is time-domain and does not introduce significant latency suitable for monitoring.

### Advanced Audio: Time Stretch

- Audio playback speed can be changed (0.5× to 2×) without altering the pitch, using a phase vocoder / WSOLA algorithm.
- Time-stretched audio retains natural timbre without the "chipmunk" or "slow-motion" artifacts of naive resampling.
- Combined speed + pitch change (both at the same ratio) is also available as a high-quality alternative to `atempo`.

### Advanced Audio: Spectral Noise Reduction

- Background noise can be attenuated from an audio track using a two-step process:
  - **Noise profile capture**: a section of audio containing only noise (e.g., room tone) is analyzed to build a noise spectral profile.
  - **Reduction**: the profile is applied to the full track, attenuating frequencies that match the noise signature.
- Noise reduction strength (0–100%) and frequency resolution are configurable.
- Implemented via FFmpeg's `afftdn` (FFT-based denoiser) filter.

### Audio Ducking (Sidechain Compression)

- A "foreground" audio track (e.g., narration) can automatically reduce the volume of a "background" track (e.g., music) when the foreground is active.
- Threshold, ratio, attack, and release are configurable.
- The duck amount (maximum dB reduction) is configurable.
- Common use case: voice-over automatically ducks background music.

### Color Analysis Scopes

- A **waveform monitor** can be computed from any video frame: returns luminance values as a 2D array (x position × luminance level), suitable for rendering a waveform display.
- A **vectorscope** can be computed: returns Cb/Cr (U/V) component data as a 2D scatter plot suitable for rendering a vectorscope display.
- A **RGB parade** can be computed: separate waveform data for R, G, and B channels.
- A **histogram** can be computed: distribution of luminance or per-channel values as a `Vec<u32>` bin array.
- All scope outputs are returned as Rust data structures; rendering is left to the caller.

---

## Design Decisions

| Topic | Decision |
|---|---|
| Stabilization | libvidstab via FFmpeg `vidstabdetect` + `vidstabtransform` filters; requires `gpl` or `lgpl` build with libvidstab |
| Motion blur | `minterpolate` or `tblend` filter depending on the mode |
| Film grain | `noise` filter with `alls` parameter; or `filmgrain` filter if available |
| Pitch shift | `asetrate` + `atempo` chain for simple pitch shift; `rubberband` filter if available for quality |
| Time stretch | `atempo` (chained for ratios outside 0.5–2.0) by default; `rubberband` for higher quality |
| Noise reduction | `afftdn` filter |
| Ducking | `sidechaincompress` filter |
| Scopes | Computed from raw `VideoFrame` / `AudioFrame` data in Rust; no libavfilter dependency for scope math |

---

## Definition of Done

- Stabilization two-pass pipeline produces visibly smoother output on a shaky test clip
- Pitch shift by +2 semitones produces correct frequency shift (verified with `ffprobe` spectral analysis)
- Time stretch at 0.75× produces correct duration without pitch change
- Ducking test: background music at −20 dBFS reduces by ≥ 12 dB when foreground speech is present
- Waveform monitor data verified against a known frame with a horizontal gradient
