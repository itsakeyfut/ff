//! Plane allocation helpers for [`VideoFrame`].

use super::VideoFrame;
use crate::error::FrameError;
use crate::{PixelFormat, PooledBuffer};

impl VideoFrame {
    /// Allocates planes and strides for the given dimensions and format.
    #[allow(clippy::too_many_lines)]
    pub(super) fn allocate_planes(
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<(Vec<PooledBuffer>, Vec<usize>), FrameError> {
        match format {
            // Packed RGB formats - single plane
            PixelFormat::Rgb24 | PixelFormat::Bgr24 => {
                let stride = width as usize * 3;
                let data = vec![0u8; stride * height as usize];
                Ok((vec![PooledBuffer::standalone(data)], vec![stride]))
            }
            // RGBA/BGRA - 4 bytes per pixel
            PixelFormat::Rgba | PixelFormat::Bgra => {
                let stride = width as usize * 4;
                let data = vec![0u8; stride * height as usize];
                Ok((vec![PooledBuffer::standalone(data)], vec![stride]))
            }
            PixelFormat::Gray8 => {
                let stride = width as usize;
                let data = vec![0u8; stride * height as usize];
                Ok((vec![PooledBuffer::standalone(data)], vec![stride]))
            }

            // Planar YUV 4:2:0 - Y full, U/V quarter size
            PixelFormat::Yuv420p => {
                let y_stride = width as usize;
                let uv_stride = (width as usize).div_ceil(2);
                let uv_height = (height as usize).div_ceil(2);

                let y_data = vec![0u8; y_stride * height as usize];
                let u_data = vec![0u8; uv_stride * uv_height];
                let v_data = vec![0u8; uv_stride * uv_height];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(u_data),
                        PooledBuffer::standalone(v_data),
                    ],
                    vec![y_stride, uv_stride, uv_stride],
                ))
            }

            // Planar YUV 4:2:2 - Y full, U/V half width, full height
            PixelFormat::Yuv422p => {
                let y_stride = width as usize;
                let uv_stride = (width as usize).div_ceil(2);

                let y_data = vec![0u8; y_stride * height as usize];
                let u_data = vec![0u8; uv_stride * height as usize];
                let v_data = vec![0u8; uv_stride * height as usize];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(u_data),
                        PooledBuffer::standalone(v_data),
                    ],
                    vec![y_stride, uv_stride, uv_stride],
                ))
            }

            // Planar YUV 4:4:4 - all planes full size
            PixelFormat::Yuv444p => {
                let stride = width as usize;
                let size = stride * height as usize;

                Ok((
                    vec![
                        PooledBuffer::standalone(vec![0u8; size]),
                        PooledBuffer::standalone(vec![0u8; size]),
                        PooledBuffer::standalone(vec![0u8; size]),
                    ],
                    vec![stride, stride, stride],
                ))
            }

            // Semi-planar NV12/NV21 - Y full, UV interleaved half height
            PixelFormat::Nv12 | PixelFormat::Nv21 => {
                let y_stride = width as usize;
                let uv_stride = width as usize; // UV interleaved, so same width
                let uv_height = (height as usize).div_ceil(2);

                let y_data = vec![0u8; y_stride * height as usize];
                let uv_data = vec![0u8; uv_stride * uv_height];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(uv_data),
                    ],
                    vec![y_stride, uv_stride],
                ))
            }

            // 10-bit planar YUV 4:2:0 - 2 bytes per sample
            PixelFormat::Yuv420p10le => {
                let y_stride = width as usize * 2;
                let uv_stride = (width as usize).div_ceil(2) * 2;
                let uv_height = (height as usize).div_ceil(2);

                let y_data = vec![0u8; y_stride * height as usize];
                let u_data = vec![0u8; uv_stride * uv_height];
                let v_data = vec![0u8; uv_stride * uv_height];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(u_data),
                        PooledBuffer::standalone(v_data),
                    ],
                    vec![y_stride, uv_stride, uv_stride],
                ))
            }

            // 10-bit planar YUV 4:2:2 - 2 bytes per sample
            PixelFormat::Yuv422p10le => {
                let y_stride = width as usize * 2;
                let uv_stride = (width as usize).div_ceil(2) * 2;

                let y_data = vec![0u8; y_stride * height as usize];
                let u_data = vec![0u8; uv_stride * height as usize];
                let v_data = vec![0u8; uv_stride * height as usize];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(u_data),
                        PooledBuffer::standalone(v_data),
                    ],
                    vec![y_stride, uv_stride, uv_stride],
                ))
            }

            // 10-bit planar YUV 4:4:4 - 2 bytes per sample
            PixelFormat::Yuv444p10le => {
                let stride = width as usize * 2;

                let y_data = vec![0u8; stride * height as usize];
                let u_data = vec![0u8; stride * height as usize];
                let v_data = vec![0u8; stride * height as usize];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(u_data),
                        PooledBuffer::standalone(v_data),
                    ],
                    vec![stride, stride, stride],
                ))
            }

            // 10-bit planar YUVA 4:4:4 with alpha - 2 bytes per sample
            PixelFormat::Yuva444p10le => {
                let stride = width as usize * 2;

                let y_data = vec![0u8; stride * height as usize];
                let u_data = vec![0u8; stride * height as usize];
                let v_data = vec![0u8; stride * height as usize];
                let a_data = vec![0u8; stride * height as usize];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(u_data),
                        PooledBuffer::standalone(v_data),
                        PooledBuffer::standalone(a_data),
                    ],
                    vec![stride, stride, stride, stride],
                ))
            }

            // 10-bit semi-planar P010
            PixelFormat::P010le => {
                let y_stride = width as usize * 2;
                let uv_stride = width as usize * 2;
                let uv_height = (height as usize).div_ceil(2);

                let y_data = vec![0u8; y_stride * height as usize];
                let uv_data = vec![0u8; uv_stride * uv_height];

                Ok((
                    vec![
                        PooledBuffer::standalone(y_data),
                        PooledBuffer::standalone(uv_data),
                    ],
                    vec![y_stride, uv_stride],
                ))
            }

            // Planar GBR float (gbrpf32le) - three planes, 4 bytes per sample (f32)
            PixelFormat::Gbrpf32le => {
                let stride = width as usize * 4; // 4 bytes per f32 sample
                let size = stride * height as usize;
                Ok((
                    vec![
                        PooledBuffer::standalone(vec![0u8; size]),
                        PooledBuffer::standalone(vec![0u8; size]),
                        PooledBuffer::standalone(vec![0u8; size]),
                    ],
                    vec![stride, stride, stride],
                ))
            }

            // Unknown format - cannot determine memory layout
            PixelFormat::Other(_) => Err(FrameError::UnsupportedPixelFormat(format)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_rgba() {
        let frame = VideoFrame::empty(1920, 1080, PixelFormat::Rgba).unwrap();
        assert_eq!(frame.width(), 1920);
        assert_eq!(frame.height(), 1080);
        assert_eq!(frame.format(), PixelFormat::Rgba);
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.stride(0), Some(1920 * 4));
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(1920 * 1080 * 4));
    }

    #[test]
    fn test_empty_yuv420p() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Yuv420p).unwrap();
        assert_eq!(frame.num_planes(), 3);
        assert_eq!(frame.stride(0), Some(640));
        assert_eq!(frame.stride(1), Some(320));
        assert_eq!(frame.stride(2), Some(320));

        // Y plane: 640 * 480
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(640 * 480));
        // U/V planes: 320 * 240
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(320 * 240));
        assert_eq!(frame.plane(2).map(|p| p.len()), Some(320 * 240));
    }

    #[test]
    fn test_empty_yuv422p() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Yuv422p).unwrap();
        assert_eq!(frame.num_planes(), 3);
        assert_eq!(frame.stride(0), Some(640));
        assert_eq!(frame.stride(1), Some(320));

        // Y: full resolution, U/V: half width, full height
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(640 * 480));
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(320 * 480));
    }

    #[test]
    fn test_empty_yuv444p() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Yuv444p).unwrap();
        assert_eq!(frame.num_planes(), 3);

        // All planes: full resolution
        let expected_size = 640 * 480;
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(expected_size));
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(expected_size));
        assert_eq!(frame.plane(2).map(|p| p.len()), Some(expected_size));
    }

    #[test]
    fn test_empty_nv12() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Nv12).unwrap();
        assert_eq!(frame.num_planes(), 2);
        assert_eq!(frame.stride(0), Some(640));
        assert_eq!(frame.stride(1), Some(640)); // UV interleaved

        // Y plane: full resolution
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(640 * 480));
        // UV plane: full width, half height
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(640 * 240));
    }

    #[test]
    fn test_empty_gray8() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Gray8).unwrap();
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.stride(0), Some(640));
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(640 * 480));
    }

    #[test]
    fn test_default() {
        let frame = VideoFrame::default();
        assert_eq!(frame.width(), 1);
        assert_eq!(frame.height(), 1);
        assert_eq!(frame.format(), PixelFormat::default());
        assert!(!frame.is_key_frame());
    }

    #[test]
    fn test_odd_dimensions_yuv420p() {
        // Odd dimensions require proper rounding for chroma planes
        let frame = VideoFrame::empty(641, 481, PixelFormat::Yuv420p).unwrap();

        // Y plane should be full size
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(641 * 481));

        // U/V planes should be (641+1)/2 * (481+1)/2 = 321 * 241
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(321 * 241));
        assert_eq!(frame.plane(2).map(|p| p.len()), Some(321 * 241));
    }

    #[test]
    fn test_small_frame() {
        let frame = VideoFrame::empty(1, 1, PixelFormat::Rgba).unwrap();
        assert_eq!(frame.total_size(), 4);
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(4));
    }

    #[test]
    fn test_10bit_yuv420p() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Yuv420p10le).unwrap();
        assert_eq!(frame.num_planes(), 3);

        // 10-bit uses 2 bytes per sample
        assert_eq!(frame.stride(0), Some(640 * 2));
        assert_eq!(frame.stride(1), Some(320 * 2));
        assert_eq!(frame.plane(0).map(|p| p.len()), Some(640 * 480 * 2));
        assert_eq!(frame.plane(1).map(|p| p.len()), Some(320 * 240 * 2));
    }

    #[test]
    fn test_p010le() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::P010le).unwrap();
        assert_eq!(frame.num_planes(), 2);

        // 10-bit semi-planar: Y and UV interleaved, both 2 bytes per sample
        assert_eq!(frame.stride(0), Some(640 * 2));
        assert_eq!(frame.stride(1), Some(640 * 2));
    }

    #[test]
    fn test_rgb24() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Rgb24).unwrap();
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.stride(0), Some(640 * 3));
        assert_eq!(frame.total_size(), 640 * 480 * 3);
    }

    #[test]
    fn test_bgr24() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Bgr24).unwrap();
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.stride(0), Some(640 * 3));
    }

    #[test]
    fn test_bgra() {
        let frame = VideoFrame::empty(640, 480, PixelFormat::Bgra).unwrap();
        assert_eq!(frame.num_planes(), 1);
        assert_eq!(frame.stride(0), Some(640 * 4));
    }

    #[test]
    fn test_other_format_returns_error() {
        // Unknown formats cannot be allocated - memory layout is unknown
        let result = VideoFrame::empty(640, 480, PixelFormat::Other(999));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            FrameError::UnsupportedPixelFormat(PixelFormat::Other(999))
        );
    }
}
