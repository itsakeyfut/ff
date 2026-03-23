//! Integration tests for `FanoutOutput`.
//!
//! Fans synthetic frames to two `LiveHlsOutput` targets simultaneously and
//! verifies that both output directories contain valid `index.m3u8` playlists.
//! All tests skip gracefully when the required encoder is unavailable.

// Tests are allowed to use unwrap() / expect() for simplicity.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

mod fixtures;
use fixtures::DirGuard;

use ff_format::{AudioFrame, PixelFormat, SampleFormat, VideoFrame};
use ff_stream::{FanoutOutput, LiveHlsOutput, StreamOutput};
use std::time::Duration;

// ============================================================================
// Helpers
// ============================================================================

fn make_video_frame(pts_ms: i64, width: u32, height: u32) -> VideoFrame {
    VideoFrame::new_black(width, height, PixelFormat::Yuv420p, pts_ms)
}

fn make_audio_frame(pts_ms: i64, sample_rate: u32, channels: u32) -> AudioFrame {
    AudioFrame::new_silent(sample_rate, channels, SampleFormat::F32p, pts_ms)
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn fanout_output_should_deliver_frames_to_all_targets() {
    let dir_a = tempfile::tempdir().expect("temp dir a");
    let dir_b = tempfile::tempdir().expect("temp dir b");
    let _guard_a = DirGuard(dir_a.path().to_path_buf());
    let _guard_b = DirGuard(dir_b.path().to_path_buf());

    let hls_a = match LiveHlsOutput::new(dir_a.path())
        .segment_duration(Duration::from_secs(2))
        .playlist_size(3)
        .video(640, 360, 30.0)
        .audio(44100, 2)
        .build()
    {
        Ok(h) => h,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let hls_b = match LiveHlsOutput::new(dir_b.path())
        .segment_duration(Duration::from_secs(2))
        .playlist_size(3)
        .video(320, 180, 30.0)
        .audio(44100, 2)
        .build()
    {
        Ok(h) => h,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let mut fanout = FanoutOutput::new(vec![Box::new(hls_a), Box::new(hls_b)]);

    // Push 30 seconds of synthetic frames (30 fps × 30 s = 900 video frames).
    for frame_idx in 0..900_u64 {
        let pts_ms = (frame_idx * 1000 / 30) as i64;
        fanout
            .push_video(&make_video_frame(pts_ms, 640, 360))
            .expect("push_video");
        if frame_idx.is_multiple_of(30) {
            fanout
                .push_audio(&make_audio_frame(pts_ms, 44100, 2))
                .expect("push_audio");
        }
    }
    Box::new(fanout).finish().expect("finish");

    // Both targets must have produced valid playlists.
    for (label, dir) in [("target-a", dir_a.path()), ("target-b", dir_b.path())] {
        let playlist = dir.join("index.m3u8");
        assert!(
            playlist.exists(),
            "{label}: index.m3u8 must exist after finish()"
        );
        let content = std::fs::read_to_string(&playlist).unwrap();
        assert!(
            content.contains("#EXTM3U"),
            "{label}: m3u8 must start with #EXTM3U"
        );
        assert!(
            content.contains("#EXT-X-ENDLIST") || content.contains("#EXTINF"),
            "{label}: m3u8 must contain segment entries"
        );
    }
}

#[test]
fn fanout_output_with_one_failing_target_should_return_fanout_failure() {
    use ff_stream::StreamError;

    let dir_a = tempfile::tempdir().expect("temp dir a");
    let _guard_a = DirGuard(dir_a.path().to_path_buf());

    let hls_a = match LiveHlsOutput::new(dir_a.path())
        .segment_duration(Duration::from_secs(2))
        .playlist_size(3)
        .video(640, 360, 30.0)
        .audio(44100, 2)
        .build()
    {
        Ok(h) => h,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // Build a second target that has already been finished (simulated by
    // calling finish() before fanout pushes any frame).  We use a second
    // LiveHlsOutput that points to the same directory — the muxer will still
    // open but any subsequent push will encounter state errors once we force it
    // into a bad state by calling finish first.
    //
    // Simpler: use an already-finished HLS output so push_video returns an error.
    let dir_b = tempfile::tempdir().expect("temp dir b");
    let _guard_b = DirGuard(dir_b.path().to_path_buf());

    let hls_b_built = match LiveHlsOutput::new(dir_b.path())
        .segment_duration(Duration::from_secs(2))
        .playlist_size(3)
        .video(640, 360, 30.0)
        .audio(44100, 2)
        .build()
    {
        Ok(h) => h,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };
    // Finish hls_b before it enters the fanout so its first push_video will
    // return InvalidConfig("push_video called after finish()").
    let boxed_b: Box<dyn StreamOutput> = Box::new(hls_b_built);
    // Re-box as a raw pointer trick: call finish, then wrap back.  Since
    // StreamOutput::finish consumes Box<Self> we need to rebuild a finished
    // wrapper.  The simplest approach: use a wrapper that always errors.
    struct AlwaysError;
    impl StreamOutput for AlwaysError {
        fn push_video(&mut self, _: &VideoFrame) -> Result<(), StreamError> {
            Err(StreamError::InvalidConfig {
                reason: "forced failure".into(),
            })
        }
        fn push_audio(&mut self, _: &AudioFrame) -> Result<(), StreamError> {
            Err(StreamError::InvalidConfig {
                reason: "forced failure".into(),
            })
        }
        fn finish(self: Box<Self>) -> Result<(), StreamError> {
            Ok(())
        }
    }
    drop(boxed_b);

    let mut fanout = FanoutOutput::new(vec![Box::new(hls_a), Box::new(AlwaysError)]);

    let pts_ms = 0_i64;
    let result = fanout.push_video(&make_video_frame(pts_ms, 640, 360));
    assert!(
        matches!(
            result,
            Err(StreamError::FanoutFailure {
                failed: 1,
                total: 2,
                ..
            })
        ),
        "expected FanoutFailure with 1/2 failed; got: {result:?}"
    );
}
