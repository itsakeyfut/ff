//! VP9 (libvpx-vp9) per-codec encoding options.

/// VP9 (libvpx-vp9) per-codec options.
#[derive(Debug, Clone, Default)]
pub struct Vp9Options {
    /// Encoder speed/quality trade-off: -8 = best quality / slowest, 8 = fastest.
    pub cpu_used: i8,
    /// Constrained Quality level (0–63).
    ///
    /// `Some(q)` enables CQ mode: sets `bit_rate = 0` and applies `q` as the `crf`
    /// option, producing variable-bitrate output governed by perceptual quality.
    /// `None` uses the bitrate mode configured on the builder.
    pub cq_level: Option<u8>,
    /// Log2 number of tile columns (0–6).
    pub tile_columns: u8,
    /// Log2 number of tile rows (0–6).
    pub tile_rows: u8,
    /// Enable row-based multithreading for better CPU utilisation.
    pub row_mt: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vp9_options_default_should_have_cpu_used_0() {
        let opts = Vp9Options::default();
        assert_eq!(opts.cpu_used, 0);
        assert_eq!(opts.tile_columns, 0);
        assert_eq!(opts.tile_rows, 0);
        assert!(!opts.row_mt);
    }

    #[test]
    fn vp9_options_default_should_have_no_cq_level() {
        let opts = Vp9Options::default();
        assert!(opts.cq_level.is_none());
    }
}
