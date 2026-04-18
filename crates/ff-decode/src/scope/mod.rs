//! Video scope analysis tools.
//!
//! Provides frame-level pixel analysis for video quality and colour monitoring.
//! All functions operate directly on [`ff_format::VideoFrame`] data — no `FFmpeg`
//! dependency; pure Rust pixel arithmetic.
//!
//! Currently implemented:
//! - [`ScopeAnalyzer::waveform`] — luminance waveform monitor (Y values per column)
//!

use ff_format::{PixelFormat, VideoFrame};

/// Scope analysis utilities for decoded video frames.
///
/// All methods are associated functions (no instance state).
pub struct ScopeAnalyzer;

/// Placeholder for per-channel RGB histogram data (future issue).
pub struct Histogram;

/// Placeholder for RGB parade scope data (future issue).
pub struct RgbParade;

impl ScopeAnalyzer {
    /// Compute waveform monitor data for `frame`.
    ///
    /// Returns a [`Vec`] of length `frame.width()`. Each inner [`Vec`] contains
    /// the normalised Y (luma) values `[0.0, 1.0]` of every pixel in that column,
    /// ordered top-to-bottom.
    ///
    /// Only `yuv420p`, `yuv422p`, and `yuv444p` pixel formats are supported.
    /// Returns an empty [`Vec`] for unsupported formats or if Y-plane data is
    /// unavailable.
    #[must_use]
    pub fn waveform(frame: &VideoFrame) -> Vec<Vec<f32>> {
        match frame.format() {
            PixelFormat::Yuv420p | PixelFormat::Yuv422p | PixelFormat::Yuv444p => {}
            _ => return Vec::new(),
        }

        let Some(y_data) = frame.plane(0) else {
            return Vec::new();
        };
        let Some(stride) = frame.stride(0) else {
            return Vec::new();
        };

        let w = frame.width() as usize;
        let h = frame.height() as usize;
        let mut result = vec![Vec::with_capacity(h); w];

        for row in 0..h {
            for col in 0..w {
                let luma = f32::from(y_data[row * stride + col]) / 255.0;
                result[col].push(luma);
            }
        }

        result
    }

    /// Compute vectorscope data for `frame`.
    ///
    /// Returns a [`Vec`] of `(cb, cr)` pairs, one per chroma sample, with both
    /// values normalised to `[-0.5, 0.5]`.
    ///
    /// Chroma dimensions vary by format:
    /// - `yuv420p` — `(width/2) × (height/2)` samples
    /// - `yuv422p` — `(width/2) × height` samples
    /// - `yuv444p` — `width × height` samples
    ///
    /// Returns an empty [`Vec`] for unsupported formats or if chroma plane data
    /// is unavailable.
    #[must_use]
    pub fn vectorscope(frame: &VideoFrame) -> Vec<(f32, f32)> {
        let w = frame.width() as usize;
        let h = frame.height() as usize;

        let (cb_w, cb_h) = match frame.format() {
            PixelFormat::Yuv420p => (w.div_ceil(2), h.div_ceil(2)),
            PixelFormat::Yuv422p => (w.div_ceil(2), h),
            PixelFormat::Yuv444p => (w, h),
            _ => return Vec::new(),
        };

        let Some(u_plane) = frame.plane(1) else {
            return Vec::new();
        };
        let Some(v_plane) = frame.plane(2) else {
            return Vec::new();
        };
        let Some(u_stride) = frame.stride(1) else {
            return Vec::new();
        };
        let Some(v_stride) = frame.stride(2) else {
            return Vec::new();
        };

        let mut result = Vec::with_capacity(cb_w * cb_h);
        for row in 0..cb_h {
            for col in 0..cb_w {
                let cb = f32::from(u_plane[row * u_stride + col]) / 255.0 - 0.5;
                let cr = f32::from(v_plane[row * v_stride + col]) / 255.0 - 0.5;
                result.push((cb, cr));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};

    fn make_yuv420p_frame(w: u32, h: u32, fill_y: u8) -> VideoFrame {
        let stride = w as usize;
        let uv_stride = (w as usize + 1) / 2;
        let uv_h = (h as usize + 1) / 2;
        VideoFrame::new(
            vec![
                PooledBuffer::standalone(vec![fill_y; stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; uv_stride * uv_h]),
                PooledBuffer::standalone(vec![128u8; uv_stride * uv_h]),
            ],
            vec![stride, uv_stride, uv_stride],
            w,
            h,
            PixelFormat::Yuv420p,
            Timestamp::default(),
            true,
        )
        .unwrap()
    }

    #[test]
    fn waveform_grey_frame_should_return_half_luma_values() {
        let frame = make_yuv420p_frame(4, 4, 128);
        let wf = ScopeAnalyzer::waveform(&frame);
        assert_eq!(wf.len(), 4, "result must have one inner Vec per column");
        for col in &wf {
            assert_eq!(col.len(), 4, "each column must have one value per row");
            for &v in col {
                let expected = 128.0 / 255.0;
                assert!(
                    (v - expected).abs() < 1e-6,
                    "grey Y=128 must map to {expected:.6}, got {v}"
                );
            }
        }
    }

    #[test]
    fn waveform_gradient_frame_should_have_increasing_column_means() {
        // Build a 4×4 frame where column c has Y = c * 64 (0, 64, 128, 192).
        let w = 4u32;
        let h = 4u32;
        let stride = w as usize;
        let uv_stride = (w as usize + 1) / 2;
        let uv_h = (h as usize + 1) / 2;
        let mut y_plane = vec![0u8; stride * h as usize];
        for row in 0..h as usize {
            for col in 0..w as usize {
                y_plane[row * stride + col] = (col as u8) * 64;
            }
        }
        let frame = VideoFrame::new(
            vec![
                PooledBuffer::standalone(y_plane),
                PooledBuffer::standalone(vec![128u8; uv_stride * uv_h]),
                PooledBuffer::standalone(vec![128u8; uv_stride * uv_h]),
            ],
            vec![stride, uv_stride, uv_stride],
            w,
            h,
            PixelFormat::Yuv420p,
            Timestamp::default(),
            true,
        )
        .unwrap();

        let wf = ScopeAnalyzer::waveform(&frame);
        assert_eq!(wf.len(), 4);
        let means: Vec<f32> = wf
            .iter()
            .map(|col| col.iter().sum::<f32>() / col.len() as f32)
            .collect();
        for i in 1..means.len() {
            assert!(
                means[i] > means[i - 1],
                "column means must increase left to right: {means:?}"
            );
        }
    }

    #[test]
    fn waveform_dimensions_should_match_frame_resolution() {
        let frame = make_yuv420p_frame(16, 8, 100);
        let wf = ScopeAnalyzer::waveform(&frame);
        assert_eq!(wf.len(), 16, "must have one Vec per column (width)");
        for col in &wf {
            assert_eq!(
                col.len(),
                8,
                "each column must have one value per row (height)"
            );
        }
    }

    #[test]
    fn waveform_unsupported_format_should_return_empty() {
        let frame = VideoFrame::empty(4, 4, PixelFormat::Rgba).unwrap();
        let wf = ScopeAnalyzer::waveform(&frame);
        assert!(
            wf.is_empty(),
            "unsupported pixel format must return empty Vec, got len={}",
            wf.len()
        );
    }

    #[test]
    fn waveform_yuv422p_should_be_supported() {
        let w = 4u32;
        let h = 4u32;
        let y_stride = w as usize;
        let uv_stride = (w as usize + 1) / 2;
        let frame = VideoFrame::new(
            vec![
                PooledBuffer::standalone(vec![200u8; y_stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; uv_stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; uv_stride * h as usize]),
            ],
            vec![y_stride, uv_stride, uv_stride],
            w,
            h,
            PixelFormat::Yuv422p,
            Timestamp::default(),
            true,
        )
        .unwrap();
        let wf = ScopeAnalyzer::waveform(&frame);
        assert_eq!(wf.len(), 4, "yuv422p must return result of length=width");
    }

    #[test]
    fn vectorscope_grey_frame_should_return_near_zero_pairs() {
        // U=V=128 → (128/255-0.5, 128/255-0.5) ≈ (0.00196, 0.00196)
        let frame = make_yuv420p_frame(4, 4, 128);
        let vs = ScopeAnalyzer::vectorscope(&frame);
        assert_eq!(vs.len(), 4, "yuv420p 4×4 → 2×2 chroma = 4 pairs");
        for &(cb, cr) in &vs {
            let expected = 128.0_f32 / 255.0 - 0.5;
            assert!(
                (cb - expected).abs() < 1e-6,
                "cb must be ≈{expected:.6}, got {cb}"
            );
            assert!(
                (cr - expected).abs() < 1e-6,
                "cr must be ≈{expected:.6}, got {cr}"
            );
        }
    }

    #[test]
    fn vectorscope_yuv420p_should_have_quarter_sample_count() {
        let frame = make_yuv420p_frame(8, 6, 100);
        let vs = ScopeAnalyzer::vectorscope(&frame);
        // chroma: (8+1)/2=4 × (6+1)/2=3 = 12
        assert_eq!(vs.len(), 12, "yuv420p 8×6 must produce 4×3=12 chroma pairs");
    }

    #[test]
    fn vectorscope_yuv422p_should_have_half_width_sample_count() {
        let w = 4u32;
        let h = 4u32;
        let y_stride = w as usize;
        let uv_stride = (w as usize + 1) / 2;
        let frame = VideoFrame::new(
            vec![
                PooledBuffer::standalone(vec![200u8; y_stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; uv_stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; uv_stride * h as usize]),
            ],
            vec![y_stride, uv_stride, uv_stride],
            w,
            h,
            PixelFormat::Yuv422p,
            Timestamp::default(),
            true,
        )
        .unwrap();
        let vs = ScopeAnalyzer::vectorscope(&frame);
        // chroma: 2×4 = 8 pairs
        assert_eq!(vs.len(), 8, "yuv422p 4×4 must produce 2×4=8 chroma pairs");
    }

    #[test]
    fn vectorscope_yuv444p_should_have_full_sample_count() {
        let w = 4u32;
        let h = 4u32;
        let stride = w as usize;
        let frame = VideoFrame::new(
            vec![
                PooledBuffer::standalone(vec![50u8; stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; stride * h as usize]),
            ],
            vec![stride, stride, stride],
            w,
            h,
            PixelFormat::Yuv444p,
            Timestamp::default(),
            true,
        )
        .unwrap();
        let vs = ScopeAnalyzer::vectorscope(&frame);
        assert_eq!(vs.len(), 16, "yuv444p 4×4 must produce 4×4=16 chroma pairs");
    }

    #[test]
    fn vectorscope_unsupported_format_should_return_empty() {
        let frame = VideoFrame::empty(4, 4, PixelFormat::Rgba).unwrap();
        let vs = ScopeAnalyzer::vectorscope(&frame);
        assert!(
            vs.is_empty(),
            "unsupported pixel format must return empty Vec, got len={}",
            vs.len()
        );
    }

    #[test]
    fn waveform_yuv444p_should_be_supported() {
        let w = 4u32;
        let h = 4u32;
        let stride = w as usize;
        let frame = VideoFrame::new(
            vec![
                PooledBuffer::standalone(vec![50u8; stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; stride * h as usize]),
                PooledBuffer::standalone(vec![128u8; stride * h as usize]),
            ],
            vec![stride, stride, stride],
            w,
            h,
            PixelFormat::Yuv444p,
            Timestamp::default(),
            true,
        )
        .unwrap();
        let wf = ScopeAnalyzer::waveform(&frame);
        assert_eq!(wf.len(), 4, "yuv444p must return result of length=width");
    }
}
