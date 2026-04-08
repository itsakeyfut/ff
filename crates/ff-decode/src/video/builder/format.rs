//! Output pixel format option for [`VideoDecoderBuilder`].

use ff_format::PixelFormat;

use super::VideoDecoderBuilder;

impl VideoDecoderBuilder {
    /// Sets the output pixel format for decoded frames.
    ///
    /// If not set, frames are returned in the source format. Setting an
    /// output format enables automatic conversion during decoding.
    ///
    /// # Common Formats
    ///
    /// - [`PixelFormat::Rgba`] - Best for UI rendering, includes alpha
    /// - [`PixelFormat::Rgb24`] - RGB without alpha, smaller memory footprint
    /// - [`PixelFormat::Yuv420p`] - Source format for most H.264/H.265 videos
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_decode::VideoDecoder;
    /// use ff_format::PixelFormat;
    ///
    /// let decoder = VideoDecoder::open("video.mp4")?
    ///     .output_format(PixelFormat::Rgba)
    ///     .build()?;
    /// ```
    #[must_use]
    pub fn output_format(mut self, format: PixelFormat) -> Self {
        self.output_format = Some(format);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builder_output_format_should_set_pixel_format() {
        let builder =
            VideoDecoderBuilder::new(PathBuf::from("test.mp4")).output_format(PixelFormat::Rgba);

        assert_eq!(builder.get_output_format(), Some(PixelFormat::Rgba));
    }

    #[test]
    fn builder_output_format_last_setter_wins() {
        let builder = VideoDecoderBuilder::new(PathBuf::from("test.mp4"))
            .output_format(PixelFormat::Rgba)
            .output_format(PixelFormat::Rgb24);

        assert_eq!(builder.get_output_format(), Some(PixelFormat::Rgb24));
    }

    #[test]
    fn builder_output_format_yuv420p_should_be_accepted() {
        let builder =
            VideoDecoderBuilder::new(PathBuf::from("test.mp4")).output_format(PixelFormat::Yuv420p);

        assert_eq!(builder.get_output_format(), Some(PixelFormat::Yuv420p));
    }
}
