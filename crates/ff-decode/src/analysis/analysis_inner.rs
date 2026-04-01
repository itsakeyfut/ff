//! Inner implementation details for media analysis tools.
//!
//! Analysis tools that require direct `FFmpeg` calls (filter graphs,
//! packet-level access) will add their `unsafe` implementation here.
//! `WaveformAnalyzer` uses only the safe [`crate::AudioDecoder`] API
//! and therefore has no code in this file.
