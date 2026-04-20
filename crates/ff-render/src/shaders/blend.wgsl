// Blend mode compositing — 18 Photoshop-compatible modes.
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_idx: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0,  1.0), vec2<f32>( 1.0,  1.0), vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0,  1.0), vec2<f32>( 1.0, -1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_idx], 0.0, 1.0);
    out.uv = uvs[vertex_idx];
    return out;
}

// ── Bindings ──────────────────────────────────────────────────────────────────

@group(0) @binding(0) var tex_base:    texture_2d<f32>;
@group(0) @binding(1) var tex_overlay: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> u: BlendUniforms;

struct BlendUniforms {
    // Mode codes: Normal=0 Multiply=1 Screen=2 Overlay=3 SoftLight=4 HardLight=5
    //             ColorDodge=6 ColorBurn=7 Difference=8 Exclusion=9 Add=10 Subtract=11
    //             Darken=12 Lighten=13 Hue=14 Saturation=15 Color=16 Luminosity=17
    mode:    u32,
    opacity: f32,
    _pad0:   f32,
    _pad1:   f32,
}

// ── HSL helpers ───────────────────────────────────────────────────────────────

fn hue_to_rgb(p: f32, q: f32, t_in: f32) -> f32 {
    var t = t_in;
    if t < 0.0 { t += 1.0; }
    if t > 1.0 { t -= 1.0; }
    if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t; }
    if t < 0.5 { return q; }
    if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0; }
    return p;
}

fn rgb_to_hsl(rgb: vec3<f32>) -> vec3<f32> {
    let max_c = max(max(rgb.r, rgb.g), rgb.b);
    let min_c = min(min(rgb.r, rgb.g), rgb.b);
    let l = (max_c + min_c) * 0.5;
    if max_c == min_c {
        return vec3<f32>(0.0, 0.0, l);
    }
    let delta = max_c - min_c;
    let s = select(delta / (2.0 - max_c - min_c), delta / (max_c + min_c), l < 0.5);
    var h: f32;
    if max_c == rgb.r {
        h = (rgb.g - rgb.b) / delta + select(6.0, 0.0, rgb.g >= rgb.b);
    } else if max_c == rgb.g {
        h = (rgb.b - rgb.r) / delta + 2.0;
    } else {
        h = (rgb.r - rgb.g) / delta + 4.0;
    }
    return vec3<f32>(h / 6.0, s, l);
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    if hsl.y == 0.0 {
        return vec3<f32>(hsl.z);
    }
    let q = select(hsl.z + hsl.y - hsl.z * hsl.y, hsl.z * (1.0 + hsl.y), hsl.z < 0.5);
    let p = 2.0 * hsl.z - q;
    return vec3<f32>(
        hue_to_rgb(p, q, hsl.x + 1.0 / 3.0),
        hue_to_rgb(p, q, hsl.x),
        hue_to_rgb(p, q, hsl.x - 1.0 / 3.0),
    );
}

// ── Per-channel blend helpers ─────────────────────────────────────────────────

fn overlay_ch(b: f32, o: f32) -> f32 {
    return select(1.0 - 2.0 * (1.0 - b) * (1.0 - o), 2.0 * b * o, b < 0.5);
}

fn soft_light_d(b: f32) -> f32 {
    return select(sqrt(b), ((16.0 * b - 12.0) * b + 4.0) * b, b <= 0.25);
}

fn soft_light_ch(b: f32, o: f32) -> f32 {
    return select(
        b + (2.0 * o - 1.0) * (soft_light_d(b) - b),
        b - (1.0 - 2.0 * o) * b * (1.0 - b),
        o <= 0.5,
    );
}

// ── Fragment ──────────────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base    = textureSample(tex_base,    tex_sampler, in.uv);
    let overlay = textureSample(tex_overlay, tex_sampler, in.uv);

    var blend_rgb: vec3<f32>;
    switch u.mode {
        // Normal
        case 0u  { blend_rgb = overlay.rgb; }
        // Multiply
        case 1u  { blend_rgb = base.rgb * overlay.rgb; }
        // Screen
        case 2u  { blend_rgb = 1.0 - (1.0 - base.rgb) * (1.0 - overlay.rgb); }
        // Overlay
        case 3u  {
            blend_rgb = vec3<f32>(
                overlay_ch(base.r, overlay.r),
                overlay_ch(base.g, overlay.g),
                overlay_ch(base.b, overlay.b),
            );
        }
        // SoftLight
        case 4u  {
            blend_rgb = vec3<f32>(
                soft_light_ch(base.r, overlay.r),
                soft_light_ch(base.g, overlay.g),
                soft_light_ch(base.b, overlay.b),
            );
        }
        // HardLight (swap base/overlay in Overlay formula)
        case 5u  {
            blend_rgb = vec3<f32>(
                overlay_ch(overlay.r, base.r),
                overlay_ch(overlay.g, base.g),
                overlay_ch(overlay.b, base.b),
            );
        }
        // ColorDodge
        case 6u  {
            blend_rgb = clamp(base.rgb / (1.0 - overlay.rgb + 0.0001), vec3<f32>(0.0), vec3<f32>(1.0));
        }
        // ColorBurn
        case 7u  {
            blend_rgb = clamp(1.0 - (1.0 - base.rgb) / (overlay.rgb + 0.0001), vec3<f32>(0.0), vec3<f32>(1.0));
        }
        // Difference
        case 8u  { blend_rgb = abs(base.rgb - overlay.rgb); }
        // Exclusion
        case 9u  { blend_rgb = base.rgb + overlay.rgb - 2.0 * base.rgb * overlay.rgb; }
        // Add
        case 10u { blend_rgb = clamp(base.rgb + overlay.rgb, vec3<f32>(0.0), vec3<f32>(1.0)); }
        // Subtract
        case 11u { blend_rgb = clamp(base.rgb - overlay.rgb, vec3<f32>(0.0), vec3<f32>(1.0)); }
        // Darken
        case 12u { blend_rgb = min(base.rgb, overlay.rgb); }
        // Lighten
        case 13u { blend_rgb = max(base.rgb, overlay.rgb); }
        // Hue: overlay hue + base saturation + base lightness
        case 14u {
            let base_hsl    = rgb_to_hsl(base.rgb);
            let overlay_hsl = rgb_to_hsl(overlay.rgb);
            blend_rgb = hsl_to_rgb(vec3<f32>(overlay_hsl.x, base_hsl.y, base_hsl.z));
        }
        // Saturation: base hue + overlay saturation + base lightness
        case 15u {
            let base_hsl    = rgb_to_hsl(base.rgb);
            let overlay_hsl = rgb_to_hsl(overlay.rgb);
            blend_rgb = hsl_to_rgb(vec3<f32>(base_hsl.x, overlay_hsl.y, base_hsl.z));
        }
        // Color: overlay hue + overlay saturation + base lightness
        case 16u {
            let base_hsl    = rgb_to_hsl(base.rgb);
            let overlay_hsl = rgb_to_hsl(overlay.rgb);
            blend_rgb = hsl_to_rgb(vec3<f32>(overlay_hsl.x, overlay_hsl.y, base_hsl.z));
        }
        // Luminosity: base hue + base saturation + overlay lightness
        case 17u {
            let base_hsl    = rgb_to_hsl(base.rgb);
            let overlay_hsl = rgb_to_hsl(overlay.rgb);
            blend_rgb = hsl_to_rgb(vec3<f32>(base_hsl.x, base_hsl.y, overlay_hsl.z));
        }
        default  { blend_rgb = overlay.rgb; }
    }

    // Apply opacity: modulate blend result against base using overlay.a * opacity.
    let effective_alpha = overlay.a * u.opacity;
    let out_rgb = mix(base.rgb, blend_rgb, effective_alpha);
    return vec4<f32>(clamp(out_rgb, vec3<f32>(0.0), vec3<f32>(1.0)), base.a);
}
