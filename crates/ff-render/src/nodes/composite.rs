use super::RenderNodeCpu;

// ── BlendMode ─────────────────────────────────────────────────────────────────

/// Photoshop-compatible blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum BlendMode {
    /// Overlay replaces base.
    #[default]
    Normal = 0,
    /// base × overlay.
    Multiply = 1,
    /// 1 − (1−base)(1−overlay).
    Screen = 2,
    /// Multiply below 50% grey, Screen above.
    Overlay = 3,
    /// Soft light — W3C formula.
    SoftLight = 4,
    /// Hard light — Overlay with base/overlay swapped.
    HardLight = 5,
    /// base / (1 − overlay).
    ColorDodge = 6,
    /// 1 − (1−base) / overlay.
    ColorBurn = 7,
    /// |base − overlay|.
    Difference = 8,
    /// base + overlay − 2·base·overlay.
    Exclusion = 9,
    /// clamp(base + overlay, 0, 1).
    Add = 10,
    /// clamp(base − overlay, 0, 1).
    Subtract = 11,
    /// min(base, overlay).
    Darken = 12,
    /// max(base, overlay).
    Lighten = 13,
    /// Overlay hue + base saturation + base lightness.
    Hue = 14,
    /// Base hue + overlay saturation + base lightness.
    Saturation = 15,
    /// Overlay hue + overlay saturation + base lightness.
    Color = 16,
    /// Base hue + base saturation + overlay lightness.
    Luminosity = 17,
}

// ── BlendModeNode ─────────────────────────────────────────────────────────────

#[cfg(feature = "wgpu")]
struct BlendPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
}

/// Apply a Photoshop-compatible blend mode to two input textures.
///
/// `input_count() = 2` — `inputs[0]` is the base layer, `inputs[1]` is the
/// overlay.  The `opacity` field attenuates the overlay's contribution.
///
/// For the CPU path the overlay data must be stored in `overlay_rgba`.
pub struct BlendModeNode {
    /// Blend algorithm.
    pub mode: BlendMode,
    /// Overlay opacity (0.0 = invisible, 1.0 = fully applied).
    pub opacity: f32,
    /// Overlay frame as RGBA bytes (required for CPU path).
    pub overlay_rgba: Vec<u8>,
    /// Width of `overlay_rgba`.
    pub overlay_width: u32,
    /// Height of `overlay_rgba`.
    pub overlay_height: u32,
    #[cfg(feature = "wgpu")]
    pipeline: std::sync::OnceLock<BlendPipeline>,
}

impl BlendModeNode {
    #[must_use]
    pub fn new(
        mode: BlendMode,
        opacity: f32,
        overlay_rgba: Vec<u8>,
        overlay_width: u32,
        overlay_height: u32,
    ) -> Self {
        Self {
            mode,
            opacity,
            overlay_rgba,
            overlay_width,
            overlay_height,
            #[cfg(feature = "wgpu")]
            pipeline: std::sync::OnceLock::new(),
        }
    }
}

// ── CPU helpers ───────────────────────────────────────────────────────────────

#[allow(clippy::many_single_char_names, clippy::float_cmp)]
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> [f32; 3] {
    let max_c = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let l = (max_c + min_c) * 0.5;
    if (max_c - min_c).abs() < 1e-6 {
        return [0.0, 0.0, l];
    }
    let delta = max_c - min_c;
    let s = if l < 0.5 {
        delta / (max_c + min_c)
    } else {
        delta / (2.0 - max_c - min_c)
    };
    let h = if max_c == r {
        let raw = (g - b) / delta;
        if g >= b { raw } else { raw + 6.0 }
    } else if max_c == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;
    [h, s, l]
}

fn hue_to_rgb_cpu(p: f32, q: f32, t_in: f32) -> f32 {
    let t = t_in.rem_euclid(1.0);
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

#[allow(clippy::many_single_char_names)]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s.abs() < 1e-6 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    [
        hue_to_rgb_cpu(p, q, h + 1.0 / 3.0),
        hue_to_rgb_cpu(p, q, h),
        hue_to_rgb_cpu(p, q, h - 1.0 / 3.0),
    ]
}

fn overlay_ch(b: f32, o: f32) -> f32 {
    if b < 0.5 {
        2.0 * b * o
    } else {
        1.0 - 2.0 * (1.0 - b) * (1.0 - o)
    }
}

fn soft_light_d(b: f32) -> f32 {
    if b <= 0.25 {
        ((16.0 * b - 12.0) * b + 4.0) * b
    } else {
        b.sqrt()
    }
}

fn soft_light_ch(b: f32, o: f32) -> f32 {
    if o <= 0.5 {
        b - (1.0 - 2.0 * o) * b * (1.0 - b)
    } else {
        b + (2.0 * o - 1.0) * (soft_light_d(b) - b)
    }
}

#[allow(clippy::many_single_char_names)]
fn blend_rgb(mode: BlendMode, base: [f32; 3], ov: [f32; 3]) -> [f32; 3] {
    let [br, bg, bb] = base;
    let [or, og, ob] = ov;
    match mode {
        BlendMode::Normal => ov,
        BlendMode::Multiply => [br * or, bg * og, bb * ob],
        BlendMode::Screen => [
            1.0 - (1.0 - br) * (1.0 - or),
            1.0 - (1.0 - bg) * (1.0 - og),
            1.0 - (1.0 - bb) * (1.0 - ob),
        ],
        BlendMode::Overlay => [overlay_ch(br, or), overlay_ch(bg, og), overlay_ch(bb, ob)],
        BlendMode::SoftLight => [
            soft_light_ch(br, or),
            soft_light_ch(bg, og),
            soft_light_ch(bb, ob),
        ],
        BlendMode::HardLight => [overlay_ch(or, br), overlay_ch(og, bg), overlay_ch(ob, bb)],
        BlendMode::ColorDodge => [
            (br / (1.0 - or + 1e-4)).clamp(0.0, 1.0),
            (bg / (1.0 - og + 1e-4)).clamp(0.0, 1.0),
            (bb / (1.0 - ob + 1e-4)).clamp(0.0, 1.0),
        ],
        BlendMode::ColorBurn => [
            (1.0 - (1.0 - br) / (or + 1e-4)).clamp(0.0, 1.0),
            (1.0 - (1.0 - bg) / (og + 1e-4)).clamp(0.0, 1.0),
            (1.0 - (1.0 - bb) / (ob + 1e-4)).clamp(0.0, 1.0),
        ],
        BlendMode::Difference => [(br - or).abs(), (bg - og).abs(), (bb - ob).abs()],
        BlendMode::Exclusion => [
            br + or - 2.0 * br * or,
            bg + og - 2.0 * bg * og,
            bb + ob - 2.0 * bb * ob,
        ],
        BlendMode::Add => [
            (br + or).clamp(0.0, 1.0),
            (bg + og).clamp(0.0, 1.0),
            (bb + ob).clamp(0.0, 1.0),
        ],
        BlendMode::Subtract => [
            (br - or).clamp(0.0, 1.0),
            (bg - og).clamp(0.0, 1.0),
            (bb - ob).clamp(0.0, 1.0),
        ],
        BlendMode::Darken => [br.min(or), bg.min(og), bb.min(ob)],
        BlendMode::Lighten => [br.max(or), bg.max(og), bb.max(ob)],
        BlendMode::Hue => {
            let [_bh, bs, bl] = rgb_to_hsl(br, bg, bb);
            let [oh, _, _] = rgb_to_hsl(or, og, ob);
            hsl_to_rgb(oh, bs, bl)
        }
        BlendMode::Saturation => {
            let [bh, bs, bl] = rgb_to_hsl(br, bg, bb);
            let [_, os, _] = rgb_to_hsl(or, og, ob);
            let _ = bs;
            hsl_to_rgb(bh, os, bl)
        }
        BlendMode::Color => {
            let [_, _, bl] = rgb_to_hsl(br, bg, bb);
            let [oh, os, _] = rgb_to_hsl(or, og, ob);
            hsl_to_rgb(oh, os, bl)
        }
        BlendMode::Luminosity => {
            let [bh, bs, _] = rgb_to_hsl(br, bg, bb);
            let [_, _, ol] = rgb_to_hsl(or, og, ob);
            hsl_to_rgb(bh, bs, ol)
        }
    }
}

impl RenderNodeCpu for BlendModeNode {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn process_cpu(&self, rgba: &mut [u8], _w: u32, _h: u32) {
        if self.overlay_rgba.len() != rgba.len() {
            log::warn!(
                "BlendModeNode::process_cpu skipped: size mismatch base={} overlay={}",
                rgba.len(),
                self.overlay_rgba.len()
            );
            return;
        }
        for (base, ov) in rgba
            .chunks_exact_mut(4)
            .zip(self.overlay_rgba.chunks_exact(4))
        {
            let br = f32::from(base[0]) / 255.0;
            let bg = f32::from(base[1]) / 255.0;
            let bb = f32::from(base[2]) / 255.0;
            let or = f32::from(ov[0]) / 255.0;
            let og = f32::from(ov[1]) / 255.0;
            let ob = f32::from(ov[2]) / 255.0;
            let oa = f32::from(ov[3]) / 255.0;

            let [rr, rg, rb] = blend_rgb(self.mode, [br, bg, bb], [or, og, ob]);
            let eff_alpha = oa * self.opacity;
            let out_r = (br + (rr - br) * eff_alpha).clamp(0.0, 1.0);
            let out_g = (bg + (rg - bg) * eff_alpha).clamp(0.0, 1.0);
            let out_b = (bb + (rb - bb) * eff_alpha).clamp(0.0, 1.0);
            base[0] = (out_r * 255.0 + 0.5) as u8;
            base[1] = (out_g * 255.0 + 0.5) as u8;
            base[2] = (out_b * 255.0 + 0.5) as u8;
        }
    }
}

// ── GPU: BlendModeNode ────────────────────────────────────────────────────────

#[cfg(feature = "wgpu")]
impl BlendModeNode {
    #[allow(clippy::too_many_lines)]
    fn get_or_create_pipeline(&self, ctx: &crate::context::RenderContext) -> &BlendPipeline {
        self.pipeline.get_or_init(|| {
            let device = &ctx.device;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Blend shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/blend.wgsl").into()),
            });
            let bgl = two_tex_sampler_uniform_bgl(device, "Blend");
            let render_pipeline = fullscreen_pipeline(device, &shader, "Blend", &bgl);
            let sampler = linear_sampler(device, "Blend");
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Blend uniforms"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            BlendPipeline {
                render_pipeline,
                bind_group_layout: bgl,
                sampler,
                uniform_buf,
            }
        })
    }
}

#[cfg(feature = "wgpu")]
impl super::RenderNode for BlendModeNode {
    fn input_count(&self) -> usize {
        2
    }

    fn process(
        &self,
        inputs: &[&wgpu::Texture],
        outputs: &[&wgpu::Texture],
        ctx: &crate::context::RenderContext,
    ) {
        let Some(tex_base) = inputs.first() else {
            log::warn!("BlendModeNode::process called with no inputs");
            return;
        };
        let Some(output) = outputs.first() else {
            log::warn!("BlendModeNode::process called with no outputs");
            return;
        };
        let pd = self.get_or_create_pipeline(ctx);

        // Upload overlay frame.
        let ov_tex = upload_rgba_texture(
            ctx,
            &self.overlay_rgba,
            self.overlay_width,
            self.overlay_height,
            "Blend overlay",
        );

        // Write uniforms: [mode_u32, opacity_f32, pad, pad] = 16 bytes.
        let mode_bytes = (self.mode as u32).to_le_bytes();
        let opac_bytes = self.opacity.to_le_bytes();
        let uniforms: [u8; 16] = [
            mode_bytes[0],
            mode_bytes[1],
            mode_bytes[2],
            mode_bytes[3],
            opac_bytes[0],
            opac_bytes[1],
            opac_bytes[2],
            opac_bytes[3],
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        ctx.queue.write_buffer(&pd.uniform_buf, 0, &uniforms);

        let base_view = tex_base.create_view(&wgpu::TextureViewDescriptor::default());
        let ov_view = ov_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let out_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blend BG"),
            layout: &pd.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&base_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&ov_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&pd.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pd.uniform_buf.as_entire_binding(),
                },
            ],
        });

        submit_render_pass(ctx, &pd.render_pipeline, &bind_group, &out_view, "Blend");
    }
}

// ── TransformNode ─────────────────────────────────────────────────────────────

#[cfg(feature = "wgpu")]
struct TransformPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
}

/// Apply a 2D affine transform (translate, rotate, scale) to a texture.
///
/// Pixels that fall outside the [0, 1] UV range after the inverse transform
/// are rendered as fully transparent.
///
/// The CPU path is a no-op (passthrough); use the GPU path for actual
/// transformation.
pub struct TransformNode {
    /// UV-space translation (positive = shift right/down).
    pub translate: [f32; 2],
    /// Counter-clockwise rotation in radians.
    pub rotate: f32,
    /// Scale factors (1.0 = no change; > 1.0 = zoom in).
    pub scale: [f32; 2],
    #[cfg(feature = "wgpu")]
    pipeline: std::sync::OnceLock<TransformPipeline>,
}

impl TransformNode {
    #[must_use]
    pub fn new(translate: [f32; 2], rotate: f32, scale: [f32; 2]) -> Self {
        Self {
            translate,
            rotate,
            scale,
            #[cfg(feature = "wgpu")]
            pipeline: std::sync::OnceLock::new(),
        }
    }
}

impl Default for TransformNode {
    fn default() -> Self {
        Self::new([0.0, 0.0], 0.0, [1.0, 1.0])
    }
}

impl RenderNodeCpu for TransformNode {
    fn process_cpu(&self, _rgba: &mut [u8], _w: u32, _h: u32) {
        // Affine transform is not implemented in the CPU fallback path.
    }
}

#[cfg(feature = "wgpu")]
impl TransformNode {
    fn get_or_create_pipeline(&self, ctx: &crate::context::RenderContext) -> &TransformPipeline {
        self.pipeline.get_or_init(|| {
            let device = &ctx.device;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Transform shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/transform.wgsl").into()),
            });
            let bgl = one_tex_sampler_uniform_bgl(device, "Transform");
            let render_pipeline = fullscreen_pipeline(device, &shader, "Transform", &bgl);
            let sampler = linear_sampler(device, "Transform");
            // Uniform: translate[2], rotate, _pad, scale[2], _pad, _pad = 32 bytes.
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Transform uniforms"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            TransformPipeline {
                render_pipeline,
                bind_group_layout: bgl,
                sampler,
                uniform_buf,
            }
        })
    }
}

#[cfg(feature = "wgpu")]
impl super::RenderNode for TransformNode {
    fn process(
        &self,
        inputs: &[&wgpu::Texture],
        outputs: &[&wgpu::Texture],
        ctx: &crate::context::RenderContext,
    ) {
        let Some(input) = inputs.first() else {
            log::warn!("TransformNode::process called with no inputs");
            return;
        };
        let Some(output) = outputs.first() else {
            log::warn!("TransformNode::process called with no outputs");
            return;
        };
        let pd = self.get_or_create_pipeline(ctx);

        // Pack uniforms: translate(2), rotate(1), pad(1), scale(2), pad(2) → 8×f32 = 32 bytes.
        let uniforms = pack_f32(&[
            self.translate[0],
            self.translate[1],
            self.rotate,
            0.0,
            self.scale[0],
            self.scale[1],
            0.0,
            0.0,
        ]);
        ctx.queue.write_buffer(&pd.uniform_buf, 0, &uniforms);

        let in_view = input.create_view(&wgpu::TextureViewDescriptor::default());
        let out_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Transform BG"),
            layout: &pd.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&in_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pd.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pd.uniform_buf.as_entire_binding(),
                },
            ],
        });
        submit_render_pass(
            ctx,
            &pd.render_pipeline,
            &bind_group,
            &out_view,
            "Transform",
        );
    }
}

// ── ChromaKeyNode ─────────────────────────────────────────────────────────────

#[cfg(feature = "wgpu")]
struct ChromaKeyPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
}

/// Remove a solid colour from a texture by chroma distance, producing alpha.
///
/// The algorithm computes the Euclidean distance between the pixel's chroma
/// vector (RGB − luma) and the key colour's chroma vector, then applies a soft
/// threshold to set the alpha channel.  Pixels that match `key_color` within
/// `tolerance` become fully transparent; pixels further than `tolerance +
/// softness` stay fully opaque.
pub struct ChromaKeyNode {
    /// Key colour in linear RGB [0.0, 1.0].
    pub key_color: [f32; 3],
    /// Chroma distance threshold (0.0–1.0).
    pub tolerance: f32,
    /// Edge feather width (0.0–1.0).
    pub softness: f32,
    #[cfg(feature = "wgpu")]
    pipeline: std::sync::OnceLock<ChromaKeyPipeline>,
}

impl ChromaKeyNode {
    #[must_use]
    pub fn new(key_color: [f32; 3], tolerance: f32, softness: f32) -> Self {
        Self {
            key_color,
            tolerance,
            softness,
            #[cfg(feature = "wgpu")]
            pipeline: std::sync::OnceLock::new(),
        }
    }
}

// ── CPU helpers ───────────────────────────────────────────────────────────────

fn bt709_luma(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn chroma_dist_cpu(pixel: [f32; 3], key: [f32; 3]) -> f32 {
    let pl = bt709_luma(pixel[0], pixel[1], pixel[2]);
    let kl = bt709_luma(key[0], key[1], key[2]);
    let dp = [pixel[0] - pl, pixel[1] - pl, pixel[2] - pl];
    let dk = [key[0] - kl, key[1] - kl, key[2] - kl];
    let d = [dp[0] - dk[0], dp[1] - dk[1], dp[2] - dk[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl RenderNodeCpu for ChromaKeyNode {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn process_cpu(&self, rgba: &mut [u8], _w: u32, _h: u32) {
        for pixel in rgba.chunks_exact_mut(4) {
            let r = f32::from(pixel[0]) / 255.0;
            let g = f32::from(pixel[1]) / 255.0;
            let b = f32::from(pixel[2]) / 255.0;
            let a = f32::from(pixel[3]) / 255.0;
            let dist = chroma_dist_cpu([r, g, b], self.key_color);
            let alpha_factor = smoothstep(
                self.tolerance - self.softness,
                self.tolerance + self.softness,
                dist,
            );
            pixel[3] = ((a * alpha_factor).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
}

#[cfg(feature = "wgpu")]
impl ChromaKeyNode {
    fn get_or_create_pipeline(&self, ctx: &crate::context::RenderContext) -> &ChromaKeyPipeline {
        self.pipeline.get_or_init(|| {
            let device = &ctx.device;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("ChromaKey shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/chroma_key.wgsl").into()),
            });
            let bgl = one_tex_sampler_uniform_bgl(device, "ChromaKey");
            let render_pipeline = fullscreen_pipeline(device, &shader, "ChromaKey", &bgl);
            let sampler = linear_sampler(device, "ChromaKey");
            // Uniform: key_color(3) + tolerance(1) + softness(1) + pad(3) = 8×f32 = 32 bytes.
            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ChromaKey uniforms"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            ChromaKeyPipeline {
                render_pipeline,
                bind_group_layout: bgl,
                sampler,
                uniform_buf,
            }
        })
    }
}

#[cfg(feature = "wgpu")]
impl super::RenderNode for ChromaKeyNode {
    fn process(
        &self,
        inputs: &[&wgpu::Texture],
        outputs: &[&wgpu::Texture],
        ctx: &crate::context::RenderContext,
    ) {
        let Some(input) = inputs.first() else {
            log::warn!("ChromaKeyNode::process called with no inputs");
            return;
        };
        let Some(output) = outputs.first() else {
            log::warn!("ChromaKeyNode::process called with no outputs");
            return;
        };
        let pd = self.get_or_create_pipeline(ctx);

        let uniforms = pack_f32(&[
            self.key_color[0],
            self.key_color[1],
            self.key_color[2],
            self.tolerance,
            self.softness,
            0.0,
            0.0,
            0.0,
        ]);
        ctx.queue.write_buffer(&pd.uniform_buf, 0, &uniforms);

        let in_view = input.create_view(&wgpu::TextureViewDescriptor::default());
        let out_view = output.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ChromaKey BG"),
            layout: &pd.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&in_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pd.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pd.uniform_buf.as_entire_binding(),
                },
            ],
        });
        submit_render_pass(
            ctx,
            &pd.render_pipeline,
            &bind_group,
            &out_view,
            "ChromaKey",
        );
    }
}

// ── Shared mask pipeline ──────────────────────────────────────────────────────

#[cfg(feature = "wgpu")]
struct MaskPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
}

#[cfg(feature = "wgpu")]
fn create_mask_pipeline(ctx: &crate::context::RenderContext) -> MaskPipeline {
    let device = &ctx.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Mask shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/mask.wgsl").into()),
    });
    let bgl = two_tex_sampler_uniform_bgl(device, "Mask");
    let render_pipeline = fullscreen_pipeline(device, &shader, "Mask", &bgl);
    let sampler = linear_sampler(device, "Mask");
    let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Mask uniforms"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    MaskPipeline {
        render_pipeline,
        bind_group_layout: bgl,
        sampler,
        uniform_buf,
    }
}

#[cfg(feature = "wgpu")]
fn submit_mask_pass(
    ctx: &crate::context::RenderContext,
    pd: &MaskPipeline,
    base_tex: &wgpu::Texture,
    mask_tex: &wgpu::Texture,
    output_tex: &wgpu::Texture,
    mode: u32,
    label: &str,
) {
    let mode_bytes = mode.to_le_bytes();
    let uniforms: [u8; 16] = [
        mode_bytes[0],
        mode_bytes[1],
        mode_bytes[2],
        mode_bytes[3],
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    ctx.queue.write_buffer(&pd.uniform_buf, 0, &uniforms);

    let base_view = base_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mask_view = mask_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let out_view = output_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &pd.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&base_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&mask_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&pd.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: pd.uniform_buf.as_entire_binding(),
            },
        ],
    });
    submit_render_pass(ctx, &pd.render_pipeline, &bind_group, &out_view, label);
}

// ── ShapeMaskNode ─────────────────────────────────────────────────────────────

/// Mask `inputs[0]` using the alpha channel of `inputs[1]` (or `mask_rgba`).
///
/// Pixels where the mask alpha is > 0 are kept opaque; all others are made
/// fully transparent (hard threshold at ~1/255).
pub struct ShapeMaskNode {
    /// Mask frame RGBA bytes (required for the CPU path).
    pub mask_rgba: Vec<u8>,
    /// Width of `mask_rgba`.
    pub mask_width: u32,
    /// Height of `mask_rgba`.
    pub mask_height: u32,
    #[cfg(feature = "wgpu")]
    pipeline: std::sync::OnceLock<MaskPipeline>,
}

impl ShapeMaskNode {
    #[must_use]
    pub fn new(mask_rgba: Vec<u8>, mask_width: u32, mask_height: u32) -> Self {
        Self {
            mask_rgba,
            mask_width,
            mask_height,
            #[cfg(feature = "wgpu")]
            pipeline: std::sync::OnceLock::new(),
        }
    }
}

impl RenderNodeCpu for ShapeMaskNode {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn process_cpu(&self, rgba: &mut [u8], _w: u32, _h: u32) {
        if self.mask_rgba.len() != rgba.len() {
            return;
        }
        for (base, mask) in rgba.chunks_exact_mut(4).zip(self.mask_rgba.chunks_exact(4)) {
            let keep = if mask[3] > 1 { 1.0_f32 } else { 0.0_f32 };
            let a = f32::from(base[3]) / 255.0;
            base[3] = ((a * keep).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
}

#[cfg(feature = "wgpu")]
impl ShapeMaskNode {
    fn get_or_create_pipeline(&self, ctx: &crate::context::RenderContext) -> &MaskPipeline {
        self.pipeline.get_or_init(|| create_mask_pipeline(ctx))
    }
}

#[cfg(feature = "wgpu")]
impl super::RenderNode for ShapeMaskNode {
    fn input_count(&self) -> usize {
        2
    }

    fn process(
        &self,
        inputs: &[&wgpu::Texture],
        outputs: &[&wgpu::Texture],
        ctx: &crate::context::RenderContext,
    ) {
        let Some(base_tex) = inputs.first() else {
            log::warn!("ShapeMaskNode::process called with no inputs");
            return;
        };
        let Some(output) = outputs.first() else {
            log::warn!("ShapeMaskNode::process called with no outputs");
            return;
        };
        let pd = self.get_or_create_pipeline(ctx);
        let mask_tex = upload_rgba_texture(
            ctx,
            &self.mask_rgba,
            self.mask_width,
            self.mask_height,
            "ShapeMask mask",
        );
        submit_mask_pass(ctx, pd, base_tex, &mask_tex, output, 0, "ShapeMask BG");
    }
}

// ── LumaMaskNode ──────────────────────────────────────────────────────────────

/// Mask `inputs[0]` using the BT.709 luma of `inputs[1]` (or `mask_rgba`).
///
/// The mask luma (0.0–1.0) is multiplied into the base alpha channel.
pub struct LumaMaskNode {
    /// Mask frame RGBA bytes (required for the CPU path).
    pub mask_rgba: Vec<u8>,
    /// Width of `mask_rgba`.
    pub mask_width: u32,
    /// Height of `mask_rgba`.
    pub mask_height: u32,
    #[cfg(feature = "wgpu")]
    pipeline: std::sync::OnceLock<MaskPipeline>,
}

impl LumaMaskNode {
    #[must_use]
    pub fn new(mask_rgba: Vec<u8>, mask_width: u32, mask_height: u32) -> Self {
        Self {
            mask_rgba,
            mask_width,
            mask_height,
            #[cfg(feature = "wgpu")]
            pipeline: std::sync::OnceLock::new(),
        }
    }
}

impl RenderNodeCpu for LumaMaskNode {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn process_cpu(&self, rgba: &mut [u8], _w: u32, _h: u32) {
        if self.mask_rgba.len() != rgba.len() {
            return;
        }
        for (base, mask) in rgba.chunks_exact_mut(4).zip(self.mask_rgba.chunks_exact(4)) {
            let mr = f32::from(mask[0]) / 255.0;
            let mg = f32::from(mask[1]) / 255.0;
            let mb = f32::from(mask[2]) / 255.0;
            let luma = bt709_luma(mr, mg, mb);
            let ba = f32::from(base[3]) / 255.0;
            base[3] = ((ba * luma).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
}

#[cfg(feature = "wgpu")]
impl LumaMaskNode {
    fn get_or_create_pipeline(&self, ctx: &crate::context::RenderContext) -> &MaskPipeline {
        self.pipeline.get_or_init(|| create_mask_pipeline(ctx))
    }
}

#[cfg(feature = "wgpu")]
impl super::RenderNode for LumaMaskNode {
    fn input_count(&self) -> usize {
        2
    }

    fn process(
        &self,
        inputs: &[&wgpu::Texture],
        outputs: &[&wgpu::Texture],
        ctx: &crate::context::RenderContext,
    ) {
        let Some(base_tex) = inputs.first() else {
            log::warn!("LumaMaskNode::process called with no inputs");
            return;
        };
        let Some(output) = outputs.first() else {
            log::warn!("LumaMaskNode::process called with no outputs");
            return;
        };
        let pd = self.get_or_create_pipeline(ctx);
        let mask_tex = upload_rgba_texture(
            ctx,
            &self.mask_rgba,
            self.mask_width,
            self.mask_height,
            "LumaMask mask",
        );
        submit_mask_pass(ctx, pd, base_tex, &mask_tex, output, 1, "LumaMask BG");
    }
}

// ── AlphaMatteNode ────────────────────────────────────────────────────────────

/// Porter-Duff src-over: composite `inputs[0]` (foreground) over `inputs[1]`
/// (background) using the foreground's own alpha channel.
///
/// For the CPU path the background data must be stored in `background_rgba`.
pub struct AlphaMatteNode {
    /// Background frame RGBA bytes (required for the CPU path).
    pub background_rgba: Vec<u8>,
    /// Width of `background_rgba`.
    pub background_width: u32,
    /// Height of `background_rgba`.
    pub background_height: u32,
    #[cfg(feature = "wgpu")]
    pipeline: std::sync::OnceLock<MaskPipeline>,
}

impl AlphaMatteNode {
    #[must_use]
    pub fn new(background_rgba: Vec<u8>, background_width: u32, background_height: u32) -> Self {
        Self {
            background_rgba,
            background_width,
            background_height,
            #[cfg(feature = "wgpu")]
            pipeline: std::sync::OnceLock::new(),
        }
    }
}

impl RenderNodeCpu for AlphaMatteNode {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn process_cpu(&self, rgba: &mut [u8], _w: u32, _h: u32) {
        if self.background_rgba.len() != rgba.len() {
            return;
        }
        for (fg, bg) in rgba
            .chunks_exact_mut(4)
            .zip(self.background_rgba.chunks_exact(4))
        {
            let fa = f32::from(fg[3]) / 255.0;
            let ba = f32::from(bg[3]) / 255.0;
            for ch in 0..3 {
                let fc = f32::from(fg[ch]) / 255.0;
                let bc = f32::from(bg[ch]) / 255.0;
                fg[ch] = ((fc * fa + bc * (1.0 - fa)).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            fg[3] = ((fa + ba * (1.0 - fa)).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
}

#[cfg(feature = "wgpu")]
impl AlphaMatteNode {
    fn get_or_create_pipeline(&self, ctx: &crate::context::RenderContext) -> &MaskPipeline {
        self.pipeline.get_or_init(|| create_mask_pipeline(ctx))
    }
}

#[cfg(feature = "wgpu")]
impl super::RenderNode for AlphaMatteNode {
    fn input_count(&self) -> usize {
        2
    }

    fn process(
        &self,
        inputs: &[&wgpu::Texture],
        outputs: &[&wgpu::Texture],
        ctx: &crate::context::RenderContext,
    ) {
        let Some(fg_tex) = inputs.first() else {
            log::warn!("AlphaMatteNode::process called with no inputs");
            return;
        };
        let Some(output) = outputs.first() else {
            log::warn!("AlphaMatteNode::process called with no outputs");
            return;
        };
        let pd = self.get_or_create_pipeline(ctx);
        let bg_tex = upload_rgba_texture(
            ctx,
            &self.background_rgba,
            self.background_width,
            self.background_height,
            "AlphaMatte bg",
        );
        submit_mask_pass(ctx, pd, fg_tex, &bg_tex, output, 2, "AlphaMatte BG");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── BlendModeNode ─────────────────────────────────────────────────────────

    #[test]
    fn blend_mode_multiply_should_produce_product_of_base_and_overlay() {
        // 50% grey × 50% grey = 25% grey (pixel-exact per acceptance criteria).
        let grey50 = vec![128u8, 128, 128, 255];
        let node = BlendModeNode::new(BlendMode::Multiply, 1.0, grey50.clone(), 1, 1);
        let mut rgba = grey50;
        node.process_cpu(&mut rgba, 1, 1);
        // 128/255 * 128/255 * 255 ≈ 64.25 → 64 or 65.
        let expected = (128.0_f32 / 255.0 * 128.0 / 255.0 * 255.0 + 0.5) as u8;
        let diff = (rgba[0] as i32 - expected as i32).abs();
        assert!(
            diff <= 1,
            "Multiply 50%×50% grey: expected ~{expected}, got {}",
            rgba[0]
        );
    }

    #[test]
    fn blend_mode_screen_should_be_brighter_than_either_input() {
        let base = vec![100u8, 100, 100, 255];
        let overlay = vec![150u8, 150, 150, 255];
        let node = BlendModeNode::new(BlendMode::Screen, 1.0, overlay, 1, 1);
        let mut rgba = base;
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            rgba[0] > 150,
            "Screen must be brighter than max input; got {}",
            rgba[0]
        );
    }

    #[test]
    fn blend_mode_normal_at_full_opacity_should_replace_base_with_overlay() {
        let base = vec![50u8, 50, 50, 255];
        let overlay = vec![200u8, 100, 50, 255];
        let node = BlendModeNode::new(BlendMode::Normal, 1.0, overlay, 1, 1);
        let mut rgba = base;
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            (rgba[0] as i32 - 200).abs() <= 1,
            "R should match overlay; got {}",
            rgba[0]
        );
        assert!(
            (rgba[1] as i32 - 100).abs() <= 1,
            "G should match overlay; got {}",
            rgba[1]
        );
    }

    #[test]
    fn blend_mode_normal_at_zero_opacity_should_leave_base_unchanged() {
        let base = vec![50u8, 80, 120, 255];
        let overlay = vec![200u8, 200, 200, 255];
        let node = BlendModeNode::new(BlendMode::Normal, 0.0, overlay, 1, 1);
        let mut rgba = base.clone();
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            (rgba[0] as i32 - 50).abs() <= 1,
            "R should match base; got {}",
            rgba[0]
        );
    }

    #[test]
    fn blend_mode_difference_of_equal_pixels_should_be_black() {
        let grey = vec![128u8, 128, 128, 255];
        let node = BlendModeNode::new(BlendMode::Difference, 1.0, grey.clone(), 1, 1);
        let mut rgba = grey;
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            rgba[0] <= 1,
            "Difference of same pixel must be ~black; got {}",
            rgba[0]
        );
    }

    #[test]
    fn blend_mode_add_should_clamp_at_white() {
        let bright = vec![200u8, 200, 200, 255];
        let node = BlendModeNode::new(BlendMode::Add, 1.0, bright.clone(), 1, 1);
        let mut rgba = bright;
        node.process_cpu(&mut rgba, 1, 1);
        assert_eq!(rgba[0], 255, "Add of two bright values must clamp to 255");
    }

    #[test]
    fn blend_mode_darken_should_return_minimum_channel() {
        let base = vec![100u8, 200, 50, 255];
        let overlay = vec![150u8, 50, 100, 255];
        let node = BlendModeNode::new(BlendMode::Darken, 1.0, overlay, 1, 1);
        let mut rgba = base;
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            (rgba[0] as i32 - 100).abs() <= 1,
            "Darken R: min(100,150)=100; got {}",
            rgba[0]
        );
        assert!(
            (rgba[1] as i32 - 50).abs() <= 1,
            "Darken G: min(200,50)=50; got {}",
            rgba[1]
        );
        assert!(
            (rgba[2] as i32 - 50).abs() <= 1,
            "Darken B: min(50,100)=50; got {}",
            rgba[2]
        );
    }

    #[test]
    fn blend_mode_size_mismatch_should_be_noop() {
        let overlay = vec![200u8; 8];
        let node = BlendModeNode::new(BlendMode::Normal, 1.0, overlay, 2, 1);
        let original = vec![50u8, 80, 120, 255];
        let mut rgba = original.clone();
        node.process_cpu(&mut rgba, 1, 1);
        assert_eq!(rgba, original, "size mismatch must leave base unchanged");
    }

    // ── TransformNode ─────────────────────────────────────────────────────────

    #[test]
    fn transform_node_cpu_path_should_be_passthrough() {
        let node = TransformNode::new([0.1, 0.0], 0.0, [2.0, 2.0]);
        let original = vec![10u8, 20, 30, 255];
        let mut rgba = original.clone();
        node.process_cpu(&mut rgba, 1, 1);
        assert_eq!(rgba, original, "TransformNode CPU must be a no-op");
    }

    #[test]
    fn transform_node_default_should_be_identity() {
        let node = TransformNode::default();
        assert_eq!(node.translate, [0.0, 0.0]);
        assert_eq!(node.rotate, 0.0);
        assert_eq!(node.scale, [1.0, 1.0]);
    }

    // ── ChromaKeyNode ─────────────────────────────────────────────────────────

    #[test]
    fn chroma_key_node_pure_green_should_become_transparent() {
        let mut rgba = vec![0u8, 255, 0, 255]; // pure green
        let node = ChromaKeyNode::new([0.0, 1.0, 0.0], 0.1, 0.05);
        node.process_cpu(&mut rgba, 1, 1);
        assert_eq!(
            rgba[3], 0,
            "pure green key must produce fully transparent alpha"
        );
    }

    #[test]
    fn chroma_key_node_non_key_colour_should_stay_opaque() {
        let mut rgba = vec![255u8, 0, 0, 255]; // pure red
        let node = ChromaKeyNode::new([0.0, 1.0, 0.0], 0.1, 0.05);
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            rgba[3] > 200,
            "non-key colour must stay opaque; got alpha={}",
            rgba[3]
        );
    }

    #[test]
    fn chroma_key_node_tolerances_should_control_threshold() {
        // A dark green should be keyed with a generous tolerance but not with a tight one.
        let mut rgba_tight = vec![0u8, 100, 0, 255]; // dark green
        let mut rgba_loose = rgba_tight.clone();
        let node_tight = ChromaKeyNode::new([0.0, 1.0, 0.0], 0.05, 0.01);
        let node_loose = ChromaKeyNode::new([0.0, 1.0, 0.0], 0.8, 0.1);
        node_tight.process_cpu(&mut rgba_tight, 1, 1);
        node_loose.process_cpu(&mut rgba_loose, 1, 1);
        assert!(
            rgba_loose[3] < rgba_tight[3],
            "loose tolerance must key more aggressively than tight"
        );
    }

    // ── ShapeMaskNode ─────────────────────────────────────────────────────────

    #[test]
    fn shape_mask_node_opaque_mask_should_keep_base_alpha() {
        let mask = vec![0u8, 0, 0, 255]; // fully opaque mask
        let node = ShapeMaskNode::new(mask, 1, 1);
        let mut rgba = vec![128u8, 128, 128, 200];
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            (rgba[3] as i32 - 200).abs() <= 1,
            "opaque mask must preserve base alpha"
        );
    }

    #[test]
    fn shape_mask_node_transparent_mask_should_zero_alpha() {
        let mask = vec![255u8, 255, 255, 0]; // fully transparent mask
        let node = ShapeMaskNode::new(mask, 1, 1);
        let mut rgba = vec![128u8, 128, 128, 255];
        node.process_cpu(&mut rgba, 1, 1);
        assert_eq!(rgba[3], 0, "transparent mask must produce zero alpha");
    }

    // ── LumaMaskNode ─────────────────────────────────────────────────────────

    #[test]
    fn luma_mask_node_white_mask_should_preserve_alpha() {
        let mask = vec![255u8, 255, 255, 255]; // white → luma = 1.0
        let node = LumaMaskNode::new(mask, 1, 1);
        let mut rgba = vec![100u8, 100, 100, 200];
        node.process_cpu(&mut rgba, 1, 1);
        assert!(
            (rgba[3] as i32 - 200).abs() <= 2,
            "white mask preserves alpha"
        );
    }

    #[test]
    fn luma_mask_node_black_mask_should_zero_alpha() {
        let mask = vec![0u8, 0, 0, 255]; // black → luma = 0.0
        let node = LumaMaskNode::new(mask, 1, 1);
        let mut rgba = vec![100u8, 100, 100, 255];
        node.process_cpu(&mut rgba, 1, 1);
        assert_eq!(rgba[3], 0, "black mask must zero out alpha");
    }

    // ── AlphaMatteNode ───────────────────────────────────────────────────────

    #[test]
    fn alpha_matte_node_opaque_fg_should_replace_background() {
        let bg = vec![50u8, 50, 50, 255];
        let node = AlphaMatteNode::new(bg, 1, 1);
        let mut fg = vec![200u8, 100, 50, 255]; // fully opaque fg
        node.process_cpu(&mut fg, 1, 1);
        assert!(
            (fg[0] as i32 - 200).abs() <= 1,
            "opaque fg must dominate; got {}",
            fg[0]
        );
    }

    #[test]
    fn alpha_matte_node_transparent_fg_should_show_background() {
        let bg = vec![50u8, 80, 120, 255];
        let node = AlphaMatteNode::new(bg, 1, 1);
        let mut fg = vec![200u8, 200, 200, 0]; // fully transparent fg
        node.process_cpu(&mut fg, 1, 1);
        assert!(
            (fg[0] as i32 - 50).abs() <= 1,
            "transparent fg must show bg; got {}",
            fg[0]
        );
    }

    // ── Type-check ────────────────────────────────────────────────────────────

    #[test]
    fn all_blend_mode_variants_should_compile() {
        let modes = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::SoftLight,
            BlendMode::HardLight,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::Difference,
            BlendMode::Exclusion,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ];
        assert_eq!(modes.len(), 18);
    }
}

// ── GPU helpers (shared) ──────────────────────────────────────────────────────

#[cfg(feature = "wgpu")]
pub(crate) fn linear_sampler(device: &wgpu::Device, label: &str) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(&format!("{label} sampler")),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

/// Build a BGL with one texture + one sampler + one uniform buffer.
#[cfg(feature = "wgpu")]
pub(crate) fn one_tex_sampler_uniform_bgl(
    device: &wgpu::Device,
    label: &str,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&format!("{label} BGL")),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

/// Build a BGL with two textures + one sampler + one uniform buffer.
#[cfg(feature = "wgpu")]
pub(crate) fn two_tex_sampler_uniform_bgl(
    device: &wgpu::Device,
    label: &str,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&format!("{label} BGL")),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

#[cfg(feature = "wgpu")]
pub(crate) fn fullscreen_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    label: &str,
    bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{label} layout")),
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&format!("{label} pipeline")),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(feature = "wgpu")]
pub(crate) fn upload_rgba_texture(
    ctx: &crate::context::RenderContext,
    data: &[u8],
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    let tex = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    ctx.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    tex
}

#[cfg(feature = "wgpu")]
pub(crate) fn submit_render_pass(
    ctx: &crate::context::RenderContext,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    out_view: &wgpu::TextureView,
    label: &str,
) {
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(&format!("{label} encoder")),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(&format!("{label} pass")),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
    ctx.queue.submit(std::iter::once(encoder.finish()));
}

#[cfg(feature = "wgpu")]
fn pack_f32(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}
