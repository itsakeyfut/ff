//! Unsafe `FFmpeg` calls for the playback subsystem.
//!
//! This module is the only place in `ff-preview` where `unsafe` code is
//! permitted. All `unsafe` blocks must carry a `// SAFETY:` comment explaining
//! why the invariants hold.
//!
//! Future additions:
//! - `sws_scale` conversion of `AVFrame` to contiguous RGBA bytes (for `FrameSink`)
//! - `swr_convert` resampling to f32 / 48 kHz / stereo (for `pop_audio_samples`)
//! - `avformat_seek_file` + `avcodec_flush_buffers` (for `DecodeBuffer::seek`)
