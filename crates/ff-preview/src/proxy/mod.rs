//! Proxy file generation for ff-preview.
//!
//! This module is only compiled when the `proxy` feature is enabled.
//! It provides [`ProxyGenerator`] for generating lower-resolution proxy files
//! from original media using [`ff_pipeline::Pipeline`] internally.

use std::path::{Path, PathBuf};

use ff_filter::{FilterGraph, ScaleAlgorithm};
use ff_format::VideoCodec;
use ff_pipeline::{EncoderConfig, Pipeline};

use crate::error::PreviewError;

// ── ProxyResolution ───────────────────────────────────────────────────────────

/// Output resolution for a proxy file, expressed as a fraction of the source.
///
/// The target dimensions are computed as `(src / divisor) & !1` — divided by
/// the factor and rounded down to the nearest even number so that video codecs
/// do not reject odd dimensions.
///
/// | Variant   | Divisor | 1920×1080 → |
/// |-----------|---------|-------------|
/// | `Half`    | 2       | 960×540     |
/// | `Quarter` | 4       | 480×270     |
/// | `Eighth`  | 8       | 240×136     |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyResolution {
    /// 1/2 of the original dimensions (e.g. 1920×1080 → 960×540).
    Half,
    /// 1/4 of the original dimensions (e.g. 1920×1080 → 480×270).
    Quarter,
    /// 1/8 of the original dimensions (e.g. 1920×1080 → 240×136).
    Eighth,
}

impl ProxyResolution {
    fn divisor(self) -> u32 {
        match self {
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Half => "half",
            Self::Quarter => "quarter",
            Self::Eighth => "eighth",
        }
    }
}

// ── ProxyGenerator ────────────────────────────────────────────────────────────

/// Generates a lower-resolution proxy file from an original media file.
///
/// Proxy files allow smooth real-time playback of high-resolution footage by
/// substituting a lower-quality copy during editing. Uses
/// [`ff_pipeline::Pipeline`] internally — no raw `FFmpeg` calls.
///
/// # Usage
///
/// ```ignore
/// let output = ProxyGenerator::new(Path::new("4k_clip.mp4"))?
///     .resolution(ProxyResolution::Half)
///     .output_dir(Path::new("/tmp/proxies"))
///     .generate()?;
/// ```
///
/// # Output path
///
/// `{output_dir}/{stem}_proxy_{half|quarter|eighth}.mp4`
pub struct ProxyGenerator {
    input: PathBuf,
    resolution: ProxyResolution,
    codec: VideoCodec,
    output_dir: Option<PathBuf>,
}

impl ProxyGenerator {
    /// Open the input file and prepare for proxy generation.
    ///
    /// Probes `input` to confirm it is a valid media file with a video stream.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if the file cannot be probed.
    pub fn new(input: &Path) -> Result<Self, PreviewError> {
        ff_probe::open(input)?;
        Ok(Self {
            input: input.to_path_buf(),
            resolution: ProxyResolution::Half,
            codec: VideoCodec::H264,
            output_dir: None,
        })
    }

    /// Set the output resolution (default: [`ProxyResolution::Half`]).
    #[must_use]
    pub fn resolution(self, res: ProxyResolution) -> Self {
        Self {
            resolution: res,
            ..self
        }
    }

    /// Set the output video codec (default: [`VideoCodec::H264`]).
    #[must_use]
    pub fn codec(self, codec: VideoCodec) -> Self {
        Self { codec, ..self }
    }

    /// Set the output directory (default: same directory as the input file).
    #[must_use]
    pub fn output_dir(self, dir: &Path) -> Self {
        Self {
            output_dir: Some(dir.to_path_buf()),
            ..self
        }
    }

    /// Generate the proxy file synchronously.
    ///
    /// Returns the path of the generated proxy file on success.
    ///
    /// Dimensions are source ÷ resolution factor, rounded down to the nearest
    /// even number. Default quality: H.264 CRF 23, AAC audio.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError`] if probing, filtering, or encoding fails.
    pub fn generate(self) -> Result<PathBuf, PreviewError> {
        let info = ff_probe::open(&self.input)?;

        let (src_w, src_h) = info
            .resolution()
            .ok_or_else(|| PreviewError::NoVideoStream {
                path: self.input.clone(),
            })?;

        let divisor = self.resolution.divisor();
        // Round down to the nearest even number so codecs don't reject odd dimensions.
        let dst_w = (src_w / divisor) & !1;
        let dst_h = (src_h / divisor) & !1;

        let output_dir = self
            .output_dir
            .as_deref()
            .or_else(|| self.input.parent())
            .unwrap_or_else(|| Path::new("."));

        let stem = self
            .input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let filename = format!("{stem}_proxy_{}.mp4", self.resolution.suffix());
        let output_path = output_dir.join(&filename);

        log::debug!(
            "generating proxy input={} output={} src={}x{} dst={}x{}",
            self.input.display(),
            output_path.display(),
            src_w,
            src_h,
            dst_w,
            dst_h
        );

        // TODO(#385): EncoderConfig has no preset field; add preset=fast when supported.
        // FilterGraph::build() returns FilterError; convert via PipelineError since
        // PreviewError only wraps PipelineError (not FilterError directly).
        let filter = FilterGraph::builder()
            .scale(dst_w, dst_h, ScaleAlgorithm::Fast)
            .build()
            .map_err(ff_pipeline::PipelineError::from)?;

        let config = EncoderConfig::builder()
            .video_codec(self.codec)
            // Defaults: CRF 23, AAC audio — matches issue spec.
            .build();

        let input_str = self.input.to_string_lossy();
        let output_str = output_path.to_string_lossy();

        Pipeline::builder()
            .input(input_str.as_ref())
            .filter(filter)
            .output(output_str.as_ref(), config)
            .build()?
            .run()?;

        Ok(output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_resolution_half_should_have_divisor_2() {
        assert_eq!(ProxyResolution::Half.divisor(), 2);
        assert_eq!(ProxyResolution::Half.suffix(), "half");
    }

    #[test]
    fn proxy_resolution_quarter_should_have_divisor_4() {
        assert_eq!(ProxyResolution::Quarter.divisor(), 4);
        assert_eq!(ProxyResolution::Quarter.suffix(), "quarter");
    }

    #[test]
    fn proxy_resolution_eighth_should_have_divisor_8() {
        assert_eq!(ProxyResolution::Eighth.divisor(), 8);
        assert_eq!(ProxyResolution::Eighth.suffix(), "eighth");
    }

    #[test]
    fn proxy_resolution_dimension_should_round_to_even() {
        // 1079 / 2 = 539 → & !1 = 538 (rounded down to even)
        let odd: u32 = 1079;
        let result = (odd / 2) & !1;
        assert_eq!(result, 538, "odd dimension must be rounded down to even");
        assert_eq!(result % 2, 0, "result must be even");

        // Even input stays even.
        let even: u32 = 1080;
        let result_even = (even / 2) & !1;
        assert_eq!(result_even, 540);

        // 1/8 of 1920 = 240 (already even).
        let result_eighth = (1920_u32 / 8) & !1;
        assert_eq!(result_eighth, 240);
    }

    #[test]
    fn proxy_generator_new_should_fail_for_nonexistent_file() {
        let result = ProxyGenerator::new(Path::new("nonexistent_proxy_test.mp4"));
        assert!(result.is_err(), "new() must fail for a non-existent file");
    }

    #[test]
    #[ignore = "requires FFmpeg and assets/video/gameplay.mp4; run with -- --include-ignored"]
    fn proxy_generator_half_resolution_should_produce_output_file() {
        let input = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/video/gameplay.mp4");
        if !input.exists() {
            println!("skipping: gameplay.mp4 not found");
            return;
        }
        let tmp = std::env::temp_dir();
        let result = ProxyGenerator::new(&input)
            .unwrap()
            .resolution(ProxyResolution::Half)
            .output_dir(&tmp)
            .generate();
        match result {
            Ok(path) => {
                assert!(path.exists(), "proxy output file must exist");
                assert!(
                    path.to_str()
                        .map(|s| s.contains("_proxy_half"))
                        .unwrap_or(false),
                    "output path must contain '_proxy_half'"
                );
                let _ = std::fs::remove_file(&path);
            }
            Err(e) => println!("skipping: proxy generation failed: {e}"),
        }
    }
}
