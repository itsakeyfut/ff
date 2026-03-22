//! Network configuration types for network-backed sources and outputs.
//!
//! This module provides [`NetworkOptions`] — shared connection and reconnect
//! settings consumed by network-aware decoders (e.g., RTMP, SRT, HLS ingest)
//! and live streaming outputs.
//!
//! # `FFmpeg` key mapping
//!
//! | Field             | `FFmpeg` option key | Unit         |
//! |-------------------|-------------------|--------------|
//! | `connect_timeout` | `timeout`         | microseconds |
//! | `read_timeout`    | `rw_timeout`      | microseconds |
//!
//! # Examples
//!
//! ```
//! use ff_format::network::NetworkOptions;
//! use std::time::Duration;
//!
//! let opts = NetworkOptions {
//!     connect_timeout: Duration::from_secs(5),
//!     read_timeout: Duration::from_secs(15),
//!     reconnect_on_error: true,
//!     max_reconnect_attempts: 5,
//! };
//! assert_eq!(opts.connect_timeout.as_micros(), 5_000_000);
//! ```

use std::time::Duration;

/// Shared network configuration for network-backed decoders and live outputs.
///
/// These settings map directly to `FFmpeg` `AVFormatContext` options that control
/// connection and read timeouts, and to application-level reconnection logic.
///
/// # `FFmpeg` option keys
///
/// Pass the converted microsecond values via `av_dict_set` on the
/// `AVFormatContext` before opening the stream:
///
/// - `connect_timeout` → key `"timeout"`, value `connect_timeout.as_micros()`
/// - `read_timeout` → key `"rw_timeout"`, value `read_timeout.as_micros()`
///
/// # Defaults
///
/// ```
/// use ff_format::network::NetworkOptions;
/// use std::time::Duration;
///
/// let opts = NetworkOptions::default();
/// assert_eq!(opts.connect_timeout, Duration::from_secs(10));
/// assert_eq!(opts.read_timeout, Duration::from_secs(30));
/// assert!(!opts.reconnect_on_error);
/// assert_eq!(opts.max_reconnect_attempts, 3);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkOptions {
    /// Maximum time to wait when establishing a connection.
    ///
    /// Maps to the `FFmpeg` `"timeout"` option key (value in microseconds).
    /// Default: 10 seconds.
    pub connect_timeout: Duration,

    /// Maximum time to wait for data after the connection is open.
    ///
    /// Maps to the `FFmpeg` `"rw_timeout"` option key (value in microseconds).
    /// Default: 30 seconds.
    pub read_timeout: Duration,

    /// Whether to attempt reconnection when a read error occurs.
    ///
    /// When `false`, `max_reconnect_attempts` is ignored.
    /// Default: `false`.
    pub reconnect_on_error: bool,

    /// Maximum number of reconnection attempts before giving up.
    ///
    /// Ignored when `reconnect_on_error` is `false`.
    /// Default: `3`.
    pub max_reconnect_attempts: u32,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(30),
            reconnect_on_error: false,
            max_reconnect_attempts: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_should_have_correct_timeout_values() {
        let opts = NetworkOptions::default();
        assert_eq!(opts.connect_timeout, Duration::from_secs(10));
        assert_eq!(opts.read_timeout, Duration::from_secs(30));
    }

    #[test]
    fn default_should_disable_reconnect() {
        let opts = NetworkOptions::default();
        assert!(!opts.reconnect_on_error);
        assert_eq!(opts.max_reconnect_attempts, 3);
    }

    #[test]
    fn connect_timeout_microseconds_should_match_duration() {
        let opts = NetworkOptions::default();
        assert_eq!(opts.connect_timeout.as_micros(), 10_000_000);
    }

    #[test]
    fn read_timeout_microseconds_should_match_duration() {
        let opts = NetworkOptions::default();
        assert_eq!(opts.read_timeout.as_micros(), 30_000_000);
    }
}
