#![no_main]

use ff_format::{PixelFormat, PooledBuffer, Timestamp, VideoFrame};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fixed encoder parameters to avoid config-level panics.
    let Ok(tmp) = tempfile::NamedTempFile::new() else {
        return;
    };
    let mut enc = match ff_encode::VideoEncoder::create(tmp.path())
        .video(64, 64, 30.0)
        .video_codec(ff_format::VideoCodec::Mpeg4)
        .build()
    {
        Ok(e) => e,
        Err(_) => return,
    };

    // Fill a YUV420P frame with fuzzer-supplied bytes.
    let y_size = 64 * 64;
    let uv_size = 32 * 32;
    let total = y_size + uv_size * 2;
    if data.len() < total {
        return;
    }

    let y = PooledBuffer::standalone(data[..y_size].to_vec());
    let u = PooledBuffer::standalone(data[y_size..y_size + uv_size].to_vec());
    let v = PooledBuffer::standalone(data[y_size + uv_size..total].to_vec());
    let Ok(frame) = VideoFrame::new(
        vec![y, u, v],
        vec![64, 32, 32],
        64,
        64,
        PixelFormat::Yuv420p,
        Timestamp::default(),
        true,
    ) else {
        return;
    };

    let _ = enc.push_video(&frame);
    let _ = enc.finish();
});
