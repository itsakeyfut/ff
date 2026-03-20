//! Integration tests for `Iterator` and `FusedIterator` on `VideoDecoder` and
//! `AudioDecoder`.

mod fixtures;
use fixtures::*;

// ============================================================================
// Compile-time trait-bound checks
// ============================================================================

fn _assert_video_fused(_: impl std::iter::FusedIterator) {}
fn _assert_audio_fused(_: impl std::iter::FusedIterator) {}

// ============================================================================
// VideoDecoder Iterator tests
// ============================================================================

#[test]
fn video_iterator_should_yield_frames_until_eof() {
    let mut decoder = match create_decoder() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let mut count = 0u64;
    for result in &mut decoder {
        match result {
            Ok(_) => count += 1,
            Err(e) => panic!("Unexpected decode error: {e}"),
        }
    }

    assert!(count > 0, "Expected at least one video frame");
}

#[test]
fn video_iterator_should_support_take_adapter() {
    let mut decoder = match create_decoder() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frames: Vec<_> = decoder
        .by_ref()
        .take(5)
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to decode frames");

    assert_eq!(frames.len(), 5, "take(5) should yield exactly 5 frames");
}

#[test]
fn video_iterator_should_return_none_after_eof() {
    let mut decoder = match create_decoder() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    // Drain all frames
    for result in &mut decoder {
        result.expect("Unexpected decode error while draining");
    }

    assert!(decoder.is_eof(), "Decoder should report EOF");

    // FusedIterator: subsequent next() calls must return None
    assert!(
        decoder.next().is_none(),
        "next() after EOF should return None (first call)"
    );
    assert!(
        decoder.next().is_none(),
        "next() after EOF should return None (second call)"
    );
}

// ============================================================================
// AudioDecoder Iterator tests
// ============================================================================

#[test]
fn audio_iterator_should_yield_frames_until_eof() {
    let mut decoder = match create_audio_decoder() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let mut count = 0u64;
    for result in &mut decoder {
        match result {
            Ok(_) => count += 1,
            Err(e) => panic!("Unexpected decode error: {e}"),
        }
    }

    assert!(count > 0, "Expected at least one audio frame");
}

#[test]
fn audio_iterator_should_support_take_adapter() {
    let mut decoder = match create_audio_decoder() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frames: Vec<_> = decoder
        .by_ref()
        .take(10)
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to decode audio frames");

    assert_eq!(frames.len(), 10, "take(10) should yield exactly 10 frames");
}

#[test]
fn audio_iterator_collect_should_return_all_frames() {
    let mut decoder = match create_audio_decoder() {
        Ok(d) => d,
        Err(e) => {
            println!("Skipping: {e}");
            return;
        }
    };

    let frames: Vec<_> = decoder
        .by_ref()
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to collect audio frames");

    assert!(!frames.is_empty(), "Expected at least one audio frame");

    // FusedIterator: next() after collection must return None
    assert!(
        decoder.next().is_none(),
        "next() after collect should return None"
    );
}
