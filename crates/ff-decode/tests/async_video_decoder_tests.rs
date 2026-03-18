#![cfg(feature = "tokio")]

mod fixtures;
use fixtures::test_video_path;

use ff_decode::AsyncVideoDecoder;

#[tokio::test]
async fn async_video_decoder_open_should_succeed_on_valid_file() {
    let result = AsyncVideoDecoder::open(test_video_path()).await;
    assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
}

#[tokio::test]
async fn async_video_decoder_decode_frame_should_return_first_frame() {
    let mut decoder = match AsyncVideoDecoder::open(test_video_path()).await {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frame = decoder.decode_frame().await;
    assert!(
        matches!(frame, Ok(Some(_))),
        "expected first frame, got {frame:?}"
    );
}

#[tokio::test]
async fn async_video_decoder_should_fail_on_missing_file() {
    let result = AsyncVideoDecoder::open("/nonexistent/video.mp4").await;
    assert!(matches!(
        result,
        Err(ff_decode::DecodeError::FileNotFound { .. })
    ));
}
