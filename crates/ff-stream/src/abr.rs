//! Adaptive bitrate (ABR) ladder for multi-rendition HLS / DASH output.
//!
//! This module provides [`AbrLadder`] and [`Rendition`]. An `AbrLadder` holds
//! an ordered list of [`Rendition`]s (resolution + bitrate pairs) and produces
//! multi-variant HLS or multi-representation DASH output from a single input
//! file in one call.

use std::fmt::Write as _;
use std::time::Duration;

use crate::error::StreamError;

/// A single resolution/bitrate rendition in an ABR ladder.
///
/// Each `Rendition` describes one quality level that the player can switch
/// between based on available bandwidth.
///
/// # Examples
///
/// ```
/// use ff_stream::Rendition;
///
/// let r = Rendition::new(1280, 720, 3_000_000);
/// assert_eq!(r.width, 1280);
/// assert_eq!(r.bitrate, 3_000_000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rendition {
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Target bitrate in bits per second.
    pub bitrate: u64,
}

impl Rendition {
    /// Create a rendition with the given width, height, and bitrate.
    ///
    /// # Examples
    ///
    /// ```
    /// use ff_stream::Rendition;
    ///
    /// let hd = Rendition::new(1280, 720, 3_000_000);
    /// let fhd = Rendition::new(1920, 1080, 6_000_000);
    /// ```
    #[must_use]
    pub const fn new(width: u32, height: u32, bitrate: u64) -> Self {
        Self {
            width,
            height,
            bitrate,
        }
    }
}

/// Produces multi-rendition HLS or DASH output from a single input.
///
/// `AbrLadder` accepts one or more [`Rendition`]s and encodes the input at
/// each quality level, writing the results into a directory structure that a
/// player can consume with a single master playlist or MPD manifest.
///
/// # Examples
///
/// ```ignore
/// use ff_stream::{AbrLadder, Rendition};
///
/// AbrLadder::new("source.mp4")
///     .add_rendition(Rendition { width: 1920, height: 1080, bitrate: 6_000_000 })
///     .add_rendition(Rendition { width: 1280, height:  720, bitrate: 3_000_000 })
///     .hls("/var/www/hls")?;
/// ```
pub struct AbrLadder {
    input_path: String,
    renditions: Vec<Rendition>,
}

impl AbrLadder {
    /// Create a new ladder for the given input file.
    ///
    /// No renditions are added at construction time; use
    /// [`add_rendition`](Self::add_rendition) to populate the ladder before
    /// calling [`hls`](Self::hls) or [`dash`](Self::dash).
    #[must_use]
    pub fn new(input_path: &str) -> Self {
        Self {
            input_path: input_path.to_owned(),
            renditions: Vec::new(),
        }
    }

    /// Append a rendition to the ladder.
    ///
    /// Renditions are encoded in the order they are added. By convention,
    /// list them from highest to lowest quality so that the master playlist
    /// presents them in that order.
    #[must_use]
    pub fn add_rendition(mut self, r: Rendition) -> Self {
        self.renditions.push(r);
        self
    }

    /// Write a multi-variant HLS output to `output_dir`.
    ///
    /// Each rendition is written to a numbered sub-directory
    /// (`output_dir/0/`, `output_dir/1/`, …) containing its own
    /// `playlist.m3u8`. A master playlist at `output_dir/master.m3u8`
    /// references all renditions.
    ///
    /// # Errors
    ///
    /// - [`StreamError::InvalidConfig`] with `"no renditions added"` when the
    ///   ladder is empty.
    /// - Any [`StreamError`] returned by the underlying HLS muxer.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_stream::{AbrLadder, Rendition};
    ///
    /// // Empty ladder → error
    /// assert!(AbrLadder::new("src.mp4").hls("/tmp/hls").is_err());
    /// ```
    pub fn hls(self, output_dir: &str) -> Result<(), StreamError> {
        if self.renditions.is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "no renditions added".into(),
            });
        }
        for (i, rendition) in self.renditions.iter().enumerate() {
            let subdir = format!("{output_dir}/{i}");
            crate::hls::HlsOutput::new(&subdir)
                .input(&self.input_path)
                .segment_duration(Duration::from_secs(6))
                .bitrate(rendition.bitrate)
                .video_size(rendition.width, rendition.height)
                .build()?
                .write()?;
        }
        let mut content = String::from("#EXTM3U\n");
        for (i, rendition) in self.renditions.iter().enumerate() {
            let _ = write!(
                content,
                "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{}\n{i}/playlist.m3u8\n",
                rendition.bitrate, rendition.width, rendition.height
            );
        }
        std::fs::write(format!("{output_dir}/master.m3u8"), content.as_bytes())?;
        Ok(())
    }

    /// Write a multi-representation DASH output to `output_dir`.
    ///
    /// Each rendition is written to a numbered sub-directory
    /// (`output_dir/0/`, `output_dir/1/`, …) containing its own
    /// `manifest.mpd` and segments.
    ///
    /// # Errors
    ///
    /// - [`StreamError::InvalidConfig`] with `"no renditions added"` when the
    ///   ladder is empty.
    /// - Any [`StreamError`] returned by the underlying DASH muxer.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use ff_stream::{AbrLadder, Rendition};
    ///
    /// // Empty ladder → error
    /// assert!(AbrLadder::new("src.mp4").dash("/tmp/dash").is_err());
    /// ```
    pub fn dash(self, output_dir: &str) -> Result<(), StreamError> {
        if self.renditions.is_empty() {
            return Err(StreamError::InvalidConfig {
                reason: "no renditions added".into(),
            });
        }
        let rendition_params: Vec<(i64, i32, i32)> = self
            .renditions
            .iter()
            .map(|r| {
                (
                    r.bitrate.cast_signed(),
                    r.width.cast_signed(),
                    r.height.cast_signed(),
                )
            })
            .collect();
        crate::dash_inner::write_dash_abr(&self.input_path, output_dir, 4.0, &rendition_params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendition_should_store_all_fields() {
        let r = Rendition {
            width: 1920,
            height: 1080,
            bitrate: 6_000_000,
        };
        assert_eq!(r.width, 1920);
        assert_eq!(r.height, 1080);
        assert_eq!(r.bitrate, 6_000_000);
    }

    #[test]
    fn rendition_should_be_equal_when_fields_match() {
        let a = Rendition {
            width: 854,
            height: 480,
            bitrate: 1_500_000,
        };
        let b = Rendition {
            width: 854,
            height: 480,
            bitrate: 1_500_000,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn rendition_should_not_be_equal_when_fields_differ() {
        let a = Rendition {
            width: 1280,
            height: 720,
            bitrate: 3_000_000,
        };
        let b = Rendition {
            width: 1280,
            height: 720,
            bitrate: 2_000_000,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn rendition_should_implement_debug() {
        let r = Rendition {
            width: 640,
            height: 360,
            bitrate: 800_000,
        };
        let s = format!("{r:?}");
        assert!(s.contains("640"));
        assert!(s.contains("360"));
        assert!(s.contains("800000"));
    }

    #[test]
    fn rendition_should_be_copyable() {
        let original = Rendition {
            width: 1280,
            height: 720,
            bitrate: 3_000_000,
        };
        let copy = original;
        assert_eq!(copy.width, original.width);
        assert_eq!(copy.height, original.height);
        assert_eq!(copy.bitrate, original.bitrate);
    }

    #[test]
    fn new_should_store_input_path() {
        let ladder = AbrLadder::new("/src/video.mp4");
        assert_eq!(ladder.input_path, "/src/video.mp4");
    }

    #[test]
    fn add_rendition_should_store_rendition() {
        let ladder = AbrLadder::new("/src/video.mp4").add_rendition(Rendition {
            width: 1280,
            height: 720,
            bitrate: 3_000_000,
        });
        assert_eq!(ladder.renditions.len(), 1);
        assert_eq!(ladder.renditions[0].width, 1280);
    }

    #[test]
    fn hls_with_no_renditions_should_return_invalid_config() {
        let result = AbrLadder::new("/src/video.mp4").hls("/tmp/hls");
        assert!(
            matches!(result, Err(StreamError::InvalidConfig { reason }) if reason == "no renditions added")
        );
    }

    #[test]
    fn dash_with_no_renditions_should_return_invalid_config() {
        let result = AbrLadder::new("/src/video.mp4").dash("/tmp/dash");
        assert!(
            matches!(result, Err(StreamError::InvalidConfig { reason }) if reason == "no renditions added")
        );
    }
}
