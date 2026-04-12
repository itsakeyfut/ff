//! Proxy file generation for ff-preview.
//!
//! This module is only compiled when the `proxy` feature is enabled.
//! It provides `ProxyGenerator` for generating lower-resolution proxy files
//! from original media, and `ProxyJob` for background generation.
//!
//! Full implementation tracked in issues #385–#387.

/// Generates a lower-resolution proxy file from an original media file.
///
/// Proxy files allow smooth real-time playback of high-resolution footage by
/// substituting a lower-quality copy during editing. The proxy API is
/// transparent — `PreviewPlayer` serves identical RGBA frames regardless of
/// whether it is reading the original or a proxy.
///
/// # Usage (stub — full implementation in #385)
///
/// ```ignore
/// let path = ProxyGenerator::new("4k_clip.mp4")?
///     .resolution(ProxyResolution::Half)
///     .output_dir("/tmp/proxies")
///     .generate()?;
/// ```
pub struct ProxyGenerator;
