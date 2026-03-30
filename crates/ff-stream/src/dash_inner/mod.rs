//! Internal DASH muxing implementation using `FFmpeg` directly.
//!
//! This module implements the decode → encode → DASH-mux loop that powers
//! [`DashOutput::write`](crate::dash::DashOutput::write).  All `unsafe` code is
//! isolated here; `dash.rs` is purely safe Rust.

// This module is intentionally unsafe — it drives the FFmpeg C API directly.
#![allow(unsafe_code)]
// Rust 2024: Allow unsafe operations in unsafe functions for FFmpeg C API
#![allow(unsafe_op_in_unsafe_fn)]
// FFmpeg C API frequently requires raw pointer casting and borrows-as-ptr
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::too_many_lines)]
// `&mut ptr` to get `*mut *mut T` is the standard FFmpeg double-pointer pattern
#![allow(clippy::borrow_as_ptr)]
// `&mut foo as *mut *mut _` is the standard way to pass double-pointers in FFmpeg
#![allow(clippy::ref_as_ptr)]
// ABR ladder uses multiple per-rendition fields
#![allow(clippy::struct_field_names)]

mod context;
mod streams;
mod write;

use crate::error::StreamError;

// ============================================================================
// Helper: map an FFmpeg error code to StreamError::Ffmpeg
// ============================================================================

fn ffmpeg_err(code: i32) -> StreamError {
    StreamError::Ffmpeg {
        code,
        message: ff_sys::av_error_string(code),
    }
}

fn ffmpeg_err_msg(msg: &str) -> StreamError {
    StreamError::Ffmpeg {
        code: 0,
        message: msg.to_owned(),
    }
}

// ============================================================================
// Public entry point (safe wrapper)
// ============================================================================

/// Write a DASH segmented stream for the given input file.
///
/// Creates `output_dir/manifest.mpd` and initialization/media segment files
/// (`init-stream0.m4s`, `chunk-stream0-NNNNN.m4s`, …).
///
/// # Errors
///
/// Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation fails, or
/// [`StreamError::Io`] when directory creation fails.
pub(crate) fn write_dash(
    input_path: &str,
    output_dir: &str,
    segment_duration_secs: f64,
) -> Result<(), StreamError> {
    std::fs::create_dir_all(output_dir)?;
    // SAFETY: All FFmpeg resources are allocated and freed within this call.
    unsafe { write::write_dash_unsafe(input_path, output_dir, segment_duration_secs) }
}

/// Write a single DASH manifest with one `Representation` per rendition.
///
/// Creates `output_dir/manifest.mpd` and associated segment files.
///
/// # Errors
///
/// Returns [`StreamError::Ffmpeg`] when any `FFmpeg` operation fails, or
/// [`StreamError::Io`] when directory creation fails.
pub(crate) fn write_dash_abr(
    input_path: &str,
    output_dir: &str,
    segment_duration_secs: f64,
    renditions: &[(i64, i32, i32)], // (bitrate_bps, width, height)
) -> Result<(), StreamError> {
    std::fs::create_dir_all(output_dir)?;
    // SAFETY: All FFmpeg resources are allocated and freed within this call.
    unsafe {
        write::write_dash_abr_unsafe(input_path, output_dir, segment_duration_secs, renditions)
    }
}
