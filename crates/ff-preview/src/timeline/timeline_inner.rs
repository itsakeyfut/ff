//! Unsafe-free helpers for the timeline presentation loop.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

/// Alpha-composite `overlay` (RGBA) over `base` (RGBA) in place.
///
/// Uses straight-alpha blending: for each pixel `b[i] = (1 − a) · base[i] + a · overlay[i]`
/// where `a = overlay_alpha / 255`. If the buffers differ in length (different frame
/// resolutions), `base` is returned unchanged — the caller should ensure both frames have
/// been scaled to the same canvas size.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn composite_over(base: &mut [u8], overlay: &[u8]) {
    if base.len() != overlay.len() {
        return;
    }
    for (b, o) in base.chunks_exact_mut(4).zip(overlay.chunks_exact(4)) {
        let a = f32::from(o[3]) / 255.0;
        if a > 0.0 {
            let ia = 1.0_f32 - a;
            b[0] = (f32::from(b[0]) * ia + f32::from(o[0]) * a) as u8;
            b[1] = (f32::from(b[1]) * ia + f32::from(o[1]) * a) as u8;
            b[2] = (f32::from(b[2]) * ia + f32::from(o[2]) * a) as u8;
            b[3] = 255;
        }
    }
}

/// Blend two packed-RGBA buffers: `dst[i] = (1 − alpha) · a[i] + alpha · b[i]`.
///
/// If `a` and `b` have different lengths, `dst` is set to a copy of `a`.
/// The alpha channel (byte index 3, 7, 11, …) is blended identically to the
/// colour channels so that transparency transitions work correctly.
pub(super) fn blend_rgba(a: &[u8], b: &[u8], alpha: f32, dst: &mut Vec<u8>) {
    if a.len() != b.len() {
        dst.resize(a.len(), 0);
        dst.copy_from_slice(a);
        return;
    }
    dst.resize(a.len(), 0);
    let inv = 1.0_f32 - alpha;
    for ((d, av), bv) in dst.iter_mut().zip(a.iter()).zip(b.iter()) {
        *d = (f32::from(*av) * inv + f32::from(*bv) * alpha) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_rgba_at_zero_alpha_should_return_a() {
        let a = vec![200u8, 100, 50, 255];
        let b = vec![0u8, 0, 0, 255];
        let mut dst = Vec::new();
        blend_rgba(&a, &b, 0.0, &mut dst);
        assert_eq!(dst, a);
    }

    #[test]
    fn blend_rgba_at_full_alpha_should_return_b() {
        let a = vec![0u8, 0, 0, 255];
        let b = vec![200u8, 100, 50, 255];
        let mut dst = Vec::new();
        blend_rgba(&a, &b, 1.0, &mut dst);
        assert_eq!(dst, b);
    }

    #[test]
    fn blend_rgba_at_half_alpha_should_average() {
        let a = vec![100u8, 200, 0, 255];
        let b = vec![200u8, 0, 100, 255];
        let mut dst = Vec::new();
        blend_rgba(&a, &b, 0.5, &mut dst);
        // (100 * 0.5 + 200 * 0.5) as u8 = 150
        assert_eq!(dst[0], 150);
        // (200 * 0.5 + 0 * 0.5) as u8 = 100
        assert_eq!(dst[1], 100);
    }

    #[test]
    fn blend_rgba_mismatched_lengths_should_copy_a() {
        let a = vec![1u8, 2, 3, 4];
        let b = vec![5u8, 6];
        let mut dst = Vec::new();
        blend_rgba(&a, &b, 0.5, &mut dst);
        assert_eq!(dst, a);
    }
}
