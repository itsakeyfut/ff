//! Unsafe `FFmpeg` calls for the playback subsystem.
//!
//! This module is the only place in `ff-preview` where `unsafe` code is
//! permitted. All `unsafe` blocks must carry a `// SAFETY:` comment explaining
//! why the invariants hold.
//!
//! Future additions:
//! - `sws_scale` conversion of `AVFrame` to contiguous RGBA bytes (for `FrameSink`)
//! - `avformat_seek_file` + `avcodec_flush_buffers` (for `DecodeBuffer::seek`)

use ff_format::AudioFrame;

/// Extract interleaved `f32` PCM samples from a decoded [`AudioFrame`].
///
/// The caller must have configured the decoder for [`ff_format::SampleFormat::F32`]
/// (packed interleaved). Resampling to the target sample rate and channel count
/// is handled by [`ff_decode::AudioDecoder`] via `swr_convert` internally; this
/// function only copies the already-converted sample bytes into a `Vec<f32>`.
///
/// Returns an empty `Vec` when the frame is not in packed `F32` format (should
/// not occur when the decoder is configured with `SampleFormat::F32`).
pub(crate) fn audio_frame_to_f32(frame: &AudioFrame) -> Vec<f32> {
    frame.as_f32().map(<[f32]>::to_vec).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff_format::{AudioFrame, SampleFormat, Timestamp};

    #[test]
    fn audio_frame_to_f32_should_extract_packed_f32_samples() {
        // Build a 2-sample stereo F32 frame (4 values: L0, R0, L1, R1).
        let values: Vec<f32> = vec![1.0, -1.0, 0.5, -0.5];
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let frame = AudioFrame::new(
            vec![bytes],
            2, // 2 samples per channel
            2, // stereo
            48_000,
            SampleFormat::F32,
            Timestamp::default(),
        )
        .unwrap();

        let out = audio_frame_to_f32(&frame);

        assert_eq!(out.len(), 4);
        assert!(
            (out[0] - 1.0).abs() < f32::EPSILON,
            "first sample mismatch: expected 1.0 got {}",
            out[0]
        );
        assert!(
            (out[1] - (-1.0)).abs() < f32::EPSILON,
            "second sample mismatch: expected -1.0 got {}",
            out[1]
        );
        assert!(
            (out[2] - 0.5).abs() < f32::EPSILON,
            "third sample mismatch: expected 0.5 got {}",
            out[2]
        );
        assert!(
            (out[3] - (-0.5)).abs() < f32::EPSILON,
            "fourth sample mismatch: expected -0.5 got {}",
            out[3]
        );
    }

    #[test]
    fn audio_frame_to_f32_should_return_empty_for_non_f32_format() {
        // I16 format: 2 samples × 2 channels × 2 bytes/sample = 8 bytes in one packed plane.
        let bytes = vec![0u8; 8];
        let frame = AudioFrame::new(
            vec![bytes],
            2,
            2,
            48_000,
            SampleFormat::I16,
            Timestamp::default(),
        )
        .unwrap();

        let out = audio_frame_to_f32(&frame);
        assert!(
            out.is_empty(),
            "non-F32 frame should return an empty Vec, got {} samples",
            out.len()
        );
    }
}
