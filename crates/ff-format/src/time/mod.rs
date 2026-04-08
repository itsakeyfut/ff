//! Time primitives for video/audio processing.
//!
//! This module provides [`Rational`] for representing fractions (like time bases
//! and frame rates) and [`Timestamp`] for representing media timestamps with
//! their associated time base.
//!
//! # Examples
//!
//! ```
//! use ff_format::{Rational, Timestamp};
//! use std::time::Duration;
//!
//! // Create a rational number (e.g., 1/90000 time base)
//! let time_base = Rational::new(1, 90000);
//! assert_eq!(time_base.as_f64(), 1.0 / 90000.0);
//!
//! // Create a timestamp at 1 second (90000 * 1/90000)
//! let ts = Timestamp::new(90000, time_base);
//! assert!((ts.as_secs_f64() - 1.0).abs() < 0.0001);
//!
//! // Convert to Duration
//! let duration = ts.as_duration();
//! assert_eq!(duration.as_secs(), 1);
//! ```

mod rational;
mod timestamp;

pub use rational::Rational;
pub use timestamp::Timestamp;
