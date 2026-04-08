//! Output scale options for [`VideoDecoderBuilder`].

use super::{OutputScale, VideoDecoderBuilder};

impl VideoDecoderBuilder {
    /// Scales decoded frames to the given exact dimensions.
    ///
    /// The frame is scaled in the same `libswscale` pass as pixel-format
    /// conversion, so there is no extra copy. If `output_format` is not set,
    /// the source pixel format is preserved while scaling.
    ///
    /// Width and height must be greater than zero. They are rounded up to the
    /// nearest even number if necessary (required by most pixel formats).
    ///
    /// Calling this method overwrites any previous `output_width` or
    /// `output_height` call. The last setter wins.
    ///
    /// # Errors
    ///
    /// [`build()`](Self::build) returns `DecodeError::InvalidOutputDimensions`
    /// if either dimension is zero after rounding.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Decode every frame at 320×240
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_size(320, 240)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_size(mut self, width: u32, height: u32) -> Self {
        self.output_scale = Some(OutputScale::Exact { width, height });
        self
    }

    /// Scales decoded frames to the given width, preserving the aspect ratio.
    ///
    /// The height is computed from the source aspect ratio and rounded to the
    /// nearest even number. Calling this method overwrites any previous
    /// `output_size` or `output_height` call. The last setter wins.
    ///
    /// # Errors
    ///
    /// [`build()`](Self::build) returns `DecodeError::InvalidOutputDimensions`
    /// if `width` is zero.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Decode at 1280 px wide, preserving aspect ratio
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_width(1280)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_width(mut self, width: u32) -> Self {
        self.output_scale = Some(OutputScale::FitWidth(width));
        self
    }

    /// Scales decoded frames to the given height, preserving the aspect ratio.
    ///
    /// The width is computed from the source aspect ratio and rounded to the
    /// nearest even number. Calling this method overwrites any previous
    /// `output_size` or `output_width` call. The last setter wins.
    ///
    /// # Errors
    ///
    /// [`build()`](Self::build) returns `DecodeError::InvalidOutputDimensions`
    /// if `height` is zero.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    ///
    /// // Decode at 720 px tall, preserving aspect ratio
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_height(720)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_height(mut self, height: u32) -> Self {
        self.output_scale = Some(OutputScale::FitHeight(height));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DecodeError;
    use crate::video::builder::VideoDecoder;
    use std::path::PathBuf;

    #[test]
    fn builder_output_size_should_set_exact_scale() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).output_size(1280, 720);

        assert_eq!(
            builder.output_scale,
            Some(OutputScale::Exact {
                width: 1280,
                height: 720
            })
        );
    }

    #[test]
    fn builder_output_width_should_set_fit_width() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).output_width(1920);

        assert_eq!(builder.output_scale, Some(OutputScale::FitWidth(1920)));
    }

    #[test]
    fn builder_output_height_should_set_fit_height() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4")).output_height(1080);

        assert_eq!(builder.output_scale, Some(OutputScale::FitHeight(1080)));
    }

    #[test]
    fn builder_output_size_last_setter_wins() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4"))
            .output_width(1280)
            .output_size(640, 480);

        assert_eq!(
            builder.output_scale,
            Some(OutputScale::Exact {
                width: 640,
                height: 480
            })
        );
    }

    #[test]
    fn build_with_zero_width_should_return_invalid_dimensions() {
        let result = VideoDecoder::open("nonexistent.mp4")
            .output_size(0, 480)
            .build();
        assert!(matches!(
            result,
            Err(DecodeError::InvalidOutputDimensions { .. })
        ));
    }

    #[test]
    fn build_with_zero_height_should_return_invalid_dimensions() {
        let result = VideoDecoder::open("nonexistent.mp4")
            .output_size(640, 0)
            .build();
        assert!(matches!(
            result,
            Err(DecodeError::InvalidOutputDimensions { .. })
        ));
    }

    #[test]
    fn build_with_zero_output_width_should_return_invalid_dimensions() {
        let result = VideoDecoder::open("nonexistent.mp4")
            .output_width(0)
            .build();
        assert!(matches!(
            result,
            Err(DecodeError::InvalidOutputDimensions { .. })
        ));
    }

    #[test]
    fn build_with_zero_output_height_should_return_invalid_dimensions() {
        let result = VideoDecoder::open("nonexistent.mp4")
            .output_height(0)
            .build();
        assert!(matches!(
            result,
            Err(DecodeError::InvalidOutputDimensions { .. })
        ));
    }
}
