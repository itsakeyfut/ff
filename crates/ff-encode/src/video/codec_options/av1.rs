//! AV1 (libaom-av1) per-codec encoding options.

/// AV1 per-codec options (libaom-av1).
#[derive(Debug, Clone)]
pub struct Av1Options {
    /// CPU effort level: `0` = slowest / best quality, `8` = fastest / lowest quality.
    pub cpu_used: u8,
    /// Log2 of the number of tile rows (0–6).
    pub tile_rows: u8,
    /// Log2 of the number of tile columns (0–6).
    pub tile_cols: u8,
    /// Encoding usage mode.
    pub usage: Av1Usage,
}

impl Default for Av1Options {
    fn default() -> Self {
        Self {
            cpu_used: 4,
            tile_rows: 0,
            tile_cols: 0,
            usage: Av1Usage::VoD,
        }
    }
}

/// AV1 encoding usage mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Av1Usage {
    /// Video-on-demand: maximise quality at the cost of speed.
    #[default]
    VoD,
    /// Real-time: minimise encoding latency.
    RealTime,
}

impl Av1Usage {
    pub(in crate::video) fn as_str(self) -> &'static str {
        match self {
            Self::VoD => "vod",
            Self::RealTime => "realtime",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn av1_usage_should_return_correct_str() {
        assert_eq!(Av1Usage::VoD.as_str(), "vod");
        assert_eq!(Av1Usage::RealTime.as_str(), "realtime");
    }

    #[test]
    fn av1_options_default_should_have_vod_usage() {
        let opts = Av1Options::default();
        assert_eq!(opts.cpu_used, 4);
        assert_eq!(opts.tile_rows, 0);
        assert_eq!(opts.tile_cols, 0);
        assert_eq!(opts.usage, Av1Usage::VoD);
    }
}
