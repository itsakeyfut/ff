//! `Pacing::Unpaced` delivers every frame; `Pacing::RealTime` drops late ones (#1757).
//!
//! The runner's real-time loop paces against a wall clock and drops any frame more
//! than a period late, so an e2e test that puts a lower bound on delivered frames is
//! flaky by construction on a loaded runner (#1723, #1737, #1780 were all this). The
//! unpaced mode moves the clock one frame period per presented frame instead, so the
//! delivered PTS sequence is the source's, complete and evenly spaced, however slow
//! the machine is.
//!
//! Both tests stall the sink on purpose: the first delivered frame blocks for several
//! frame periods. That is the load a slow runner applies, made deterministic. Unpaced,
//! the stall changes nothing; real-time, it costs the frames that fell due meanwhile.
//! The pair is what makes each assertion mean something: the same stall, two modes,
//! two different sequences.
//!
//! Probe-gated: skips when the shared asset cannot be opened.

#![cfg(feature = "timeline")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ff_filter::{AnimatedValue, BlendMode, CompositeOp, RealtimeLayerDescriptor};
use ff_preview::{
    FrameSink, Pacing, PlayerHandle, Scene, ScenePlacement, ScenePlayer, SceneSource,
    SceneVideoTrack,
};

const FPS: f64 = 30.0;
/// Frames the sink accepts before stopping the player.
const FRAMES: usize = 30;
/// How long the first delivered frame blocks the runner: five periods, so under
/// real-time pacing at least three frames are unambiguously more than a period late
/// (the fifth sits on the boundary and may go either way).
const STALL: Duration = Duration::from_millis(170);

fn asset() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/test/av_sync_test_60s.mp4")
}

fn frame_period() -> Duration {
    Duration::from_secs_f64(1.0 / FPS)
}

fn layer_desc() -> RealtimeLayerDescriptor {
    RealtimeLayerDescriptor {
        effects: Vec::new(),
        opacity: AnimatedValue::Static(1.0),
        x: AnimatedValue::Static(0.0),
        y: AnimatedValue::Static(0.0),
        scale_x: AnimatedValue::Static(1.0),
        scale_y: AnimatedValue::Static(1.0),
        rotation: AnimatedValue::Static(0.0),
        blend_mode: BlendMode::Normal,
        composite_op: CompositeOp::Over,
    }
}

/// One clip of the shared asset, two seconds long, at the start of the timeline.
fn one_clip_scene() -> Scene {
    Scene {
        fps: FPS,
        canvas: None,
        lavfi_overlay: None,
        video_tracks: vec![SceneVideoTrack {
            placements: vec![ScenePlacement {
                source: SceneSource::File(asset()),
                offset: Duration::ZERO,
                in_point: Duration::ZERO,
                out_point: Some(Duration::from_secs(2)),
                speed: 1.0,
                xfade_dur: Duration::ZERO,
                xfade_kind: None,
                video_handle: Duration::ZERO,
                opacity: 1.0,
                layer: layer_desc(),
                fade_in: Duration::ZERO,
                fade_out: Duration::ZERO,
                volume: AnimatedValue::Static(0.0),
                pitch: 0.0,
                pan: AnimatedValue::Static(0.0),
            }],
        }],
        audio_tracks: Vec::new(),
    }
}

/// Records every delivered PTS, blocks on the first frame, stops after [`FRAMES`].
struct StallingSink {
    pts: Arc<Mutex<Vec<Duration>>>,
    handle: PlayerHandle,
}

impl FrameSink for StallingSink {
    fn push_frame(&mut self, _rgba: &[u8], _w: u32, _h: u32, pts: Duration) {
        let mut log = self.pts.lock().unwrap();
        if log.is_empty() {
            thread::sleep(STALL);
        }
        log.push(pts);
        if log.len() >= FRAMES {
            self.handle.stop();
        }
    }
}

/// The one-clip scene with the clip starting 300ms into the timeline, so the runner
/// has to synthesise gap frames up to it.
fn pre_roll_scene() -> Scene {
    let mut scene = one_clip_scene();
    scene.video_tracks[0].placements[0].offset = Duration::from_millis(300);
    scene
}

/// Runs the one-clip scene under `pacing` and returns the delivered PTS sequence, or
/// `None` when the environment cannot open the asset.
fn deliver(pacing: Pacing) -> Option<Vec<Duration>> {
    deliver_scene(pacing, one_clip_scene())
}

fn deliver_scene(pacing: Pacing, scene: Scene) -> Option<Vec<Duration>> {
    let (mut runner, handle) = ScenePlayer::open(&scene).ok()?;
    runner.set_pacing(pacing);
    let pts = Arc::new(Mutex::new(Vec::new()));
    runner.set_sink(Box::new(StallingSink {
        pts: Arc::clone(&pts),
        handle,
    }));
    runner.run().ok()?;
    let pts = pts.lock().unwrap().clone();
    Some(pts)
}

/// The largest step between consecutive delivered PTS values.
fn max_step(pts: &[Duration]) -> Duration {
    pts.windows(2)
        .map(|w| w[1].saturating_sub(w[0]))
        .max()
        .unwrap_or_default()
}

#[test]
fn unpaced_runner_should_deliver_every_frame_through_a_stall() {
    let Some(pts) = deliver(Pacing::Unpaced) else {
        return; // asset unavailable
    };
    // A period and a half: one period is the expected step, and the asset's
    // timebase rounds the step by a few microseconds either way. Two periods is a
    // dropped frame.
    let tolerance = frame_period().mul_f64(1.5);
    assert_eq!(
        pts.len(),
        FRAMES,
        "unpaced delivery must stop at the sink's count, got {pts:?}"
    );
    assert!(
        pts.windows(2).all(|w| w[1] > w[0]),
        "delivered PTS must strictly increase: {pts:?}"
    );
    assert!(
        max_step(&pts) <= tolerance,
        "unpaced delivery must not skip a frame: largest step {:?} over {:?}, sequence {pts:?}",
        max_step(&pts),
        tolerance
    );
    assert!(
        pts[0].is_zero(),
        "the clip starts at the timeline origin, got {:?}",
        pts[0]
    );
}

#[test]
fn unpaced_runner_should_fill_a_pre_roll_gap_without_duplicates() {
    // The gap loop is the one place the unpaced clock is moved by something other
    // than a presented frame: it has to advance itself one period per synthetic
    // frame, or it never reaches the clip and spins forever. So the gap must be
    // walked slot by slot, with no PTS repeated and none skipped, and the clip
    // must be reached on the other side of it.
    let Some(pts) = deliver_scene(Pacing::Unpaced, pre_roll_scene()) else {
        return;
    };
    let clip_start = Duration::from_millis(300);
    assert!(
        pts[0].is_zero(),
        "gap frames start at the timeline origin, got {:?}",
        pts[0]
    );
    assert!(
        pts.windows(2).all(|w| w[1] > w[0]),
        "no PTS may repeat across the gap: {pts:?}"
    );
    assert!(
        max_step(&pts) <= frame_period().mul_f64(1.5),
        "no slot may be skipped across the gap: largest step {:?}, sequence {pts:?}",
        max_step(&pts)
    );
    let in_gap = pts.iter().filter(|p| **p < clip_start).count();
    assert!(
        (8..=9).contains(&in_gap),
        "a 300ms gap at 30 fps is 9 slots before the clip, got {in_gap}: {pts:?}"
    );
    assert!(
        pts.iter().any(|p| *p >= clip_start),
        "the clip after the gap must be reached: {pts:?}"
    );
}

#[test]
fn real_time_runner_should_drop_the_frames_a_stall_makes_late() {
    // The contrast that keeps the test above honest: the same stall under the
    // default pacing costs frames, because that is what real-time playback is for.
    let Some(pts) = deliver(Pacing::RealTime) else {
        return;
    };
    // Three periods: the stall is five, the boundary frame is excluded, and a
    // step of three or more can only come from dropped frames.
    assert!(
        max_step(&pts) >= frame_period().mul_f64(3.0),
        "real-time pacing must drop the frames that fell due during the stall: \
         largest step {:?}, sequence {pts:?}",
        max_step(&pts)
    );
}
