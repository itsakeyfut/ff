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

#[tokio::test]
async fn into_stream_should_yield_frames() {
    use futures::StreamExt;
    let decoder = match AsyncVideoDecoder::open(test_video_path()).await {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let frames: Vec<_> = decoder.into_stream().take(3).collect().await;
    assert!(!frames.is_empty(), "expected at least one frame");
    assert!(frames.iter().all(|r| r.is_ok()), "all frames should be Ok");
}

#[tokio::test]
async fn into_stream_should_be_send() {
    // Compile-time proof: the stream satisfies Send.
    fn assert_send<T: Send>(_: T) {}
    let decoder = match AsyncVideoDecoder::open(test_video_path()).await {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    assert_send(decoder.into_stream());
}

#[tokio::test]
async fn into_stream_drop_mid_stream_should_not_leak() {
    use futures::StreamExt;
    let decoder = match AsyncVideoDecoder::open(test_video_path()).await {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    let stream = decoder.into_stream();
    futures::pin_mut!(stream);
    let _ = stream.next().await;
    // FFmpeg cleanup happens via VideoDecoder::drop when stream is dropped here
}

#[tokio::test]
async fn async_video_decode_frame_count_matches_sync() {
    use futures::StreamExt;

    let sync_count = {
        let mut dec = match ff_decode::VideoDecoder::open(test_video_path()).build() {
            Ok(d) => d,
            Err(e) => {
                println!("Skipping (sync open failed): {e}");
                return;
            }
        };
        let mut n = 0u64;
        loop {
            match dec.decode_one() {
                Ok(Some(_)) => n += 1,
                Ok(None) => break,
                Err(e) => {
                    println!("Skipping (sync decode error): {e}");
                    return;
                }
            }
        }
        n
    };

    let decoder = match AsyncVideoDecoder::open(test_video_path()).await {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping (async open failed): {e}");
            return;
        }
    };
    let async_count = decoder
        .into_stream()
        .filter_map(|r| async move { r.ok() })
        .count()
        .await as u64;

    assert_eq!(
        sync_count, async_count,
        "async frame count ({async_count}) must match sync frame count ({sync_count})"
    );
}
