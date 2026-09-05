//! `MotionTrackingSubjectTracker` — the one real, working `SubjectTracker`
//! implementation this pass builds (see `provider` module doc comment for
//! why motion tracking, and not face/person detection, was chosen).
//!
//! ## How it works
//!
//! Same "shell out to real ffmpeg, parse real output" discipline
//! `media::thumbnail`/`media::scene` already established, arguments built
//! through `ffmpeg::command::FfmpegArgs` (never shell string concatenation).
//!
//! 1. Sample frames at a fixed regular interval (`SAMPLE_FPS`) via ffmpeg's
//!    own `fps=` filter, downscaled to a small fixed analysis grid
//!    (`ANALYSIS_WIDTH`x`ANALYSIS_HEIGHT`) and converted to 8-bit grayscale
//!    (`format=gray`), piped out as headerless raw video
//!    (`-f rawvideo -pix_fmt gray -`) — one plain byte-per-pixel luminance
//!    frame per sample, read directly from ffmpeg's stdout.
//!
//!    The downscale is intentionally **not** aspect-ratio-preserving
//!    (`scale=W:H` with no `force_original_aspect_ratio`, unlike
//!    `media::thumbnail`): a pixel at fractional position `(i/W, j/H)` in the
//!    stretched analysis grid sits at exactly that same fractional position
//!    in the *source* frame regardless of aspect distortion, since each axis
//!    is scaled independently and linearly. Letterboxing would instead waste
//!    part of the grid on padding that carries no real image data — the
//!    stretch avoids that for free.
//!
//! 2. For every consecutive pair of sampled frames, compute a real
//!    per-pixel absolute luminance difference (basic, well-understood
//!    frame-differencing motion detection), threshold it to suppress
//!    encoder-noise-level flicker, and take the diff-weighted centroid of
//!    the surviving pixels as that time window's motion target — the
//!    "weighted-average position of the highest-motion region" this pass's
//!    brief calls for.
//!
//! 3. When a pair of frames shows no significant motion (a static scene),
//!    the previous known target position is carried forward rather than
//!    snapping to the frame center — an arbitrary snap-to-center on every
//!    static frame would itself manufacture the "camera jumping" master
//!    prompt §23 explicitly warns against, defeating the point before
//!    `smoothing` even runs.

use std::path::Path;

use crate::ffmpeg::command::{run_checked, FfmpegArgs};
use crate::media::probe;

use super::error::ReframeError;
use super::provider::{SubjectPosition, SubjectTracker};

/// Samples per second of source video. Low enough that even a long video
/// produces a small, fast-to-process byte stream (analysis grid size *
/// this rate bytes/sec — see `ANALYSIS_WIDTH`/`ANALYSIS_HEIGHT`), high
/// enough to catch normal-speed subject movement without aliasing into
/// nonsense.
const SAMPLE_FPS: u32 = 4;

/// Fixed analysis grid. Deliberately small and deliberately *not* matched to
/// the source's own aspect ratio (module doc comment) — this is a tracking
/// signal, not an image anyone looks at, so visual fidelity doesn't matter,
/// only where the luminance changed.
const ANALYSIS_WIDTH: usize = 160;
const ANALYSIS_HEIGHT: usize = 90;

/// A pixel's absolute luminance delta must exceed this to count as "real
/// motion" rather than encoder/sensor noise. 0-255 scale.
const MOTION_PIXEL_THRESHOLD: u8 = 24;

/// Minimum number of qualifying pixels (module constant above) for a
/// frame-pair to be considered to contain real motion at all, versus a
/// static scene whose target should carry forward the previous position
/// (module doc comment point 3). Small relative to the 14,400-pixel
/// analysis grid — enough to reject single-pixel noise, small enough to
/// still catch a modestly sized moving subject.
const MIN_MOTION_PIXELS: usize = 12;

/// Real, working `SubjectTracker` backed by ffmpeg frame sampling and
/// frame-difference motion detection (module doc comment). No ML model, no
/// new heavy dependency — see `provider` module doc comment for the scope
/// rationale.
pub struct MotionTrackingSubjectTracker;

impl SubjectTracker for MotionTrackingSubjectTracker {
    fn track(
        &self,
        ffmpeg: &Path,
        ffprobe: &Path,
        video_path: &Path,
    ) -> Result<Vec<SubjectPosition>, ReframeError> {
        let path_str = || video_path.display().to_string();

        let probed = probe::probe(ffprobe, video_path).map_err(|e| ReframeError::ProbeFailed {
            path: path_str(),
            details: e.to_string(),
        })?;
        if !probed.has_video {
            return Err(ReframeError::NoVideoStream { path: path_str() });
        }

        let frames = sample_grayscale_frames(ffmpeg, video_path)?;
        Ok(positions_from_frames(
            &frames,
            ANALYSIS_WIDTH,
            ANALYSIS_HEIGHT,
        ))
    }
}

/// One sampled grayscale analysis frame: `ANALYSIS_WIDTH * ANALYSIS_HEIGHT`
/// bytes, row-major, one byte per pixel.
type GrayFrame = Vec<u8>;

/// Runs ffmpeg's `fps=`+`scale=`+`format=gray` filter chain and reads the
/// resulting headerless raw video from stdout, splitting it into fixed-size
/// `GrayFrame`s. A short trailing partial frame (ffmpeg's own EOF flush,
/// smaller than one full frame) is silently dropped rather than treated as
/// an error — it carries no complete pixel data to diff against anyway.
fn sample_grayscale_frames(
    ffmpeg: &Path,
    video_path: &Path,
) -> Result<Vec<GrayFrame>, ReframeError> {
    let frame_size = ANALYSIS_WIDTH * ANALYSIS_HEIGHT;
    let filter = format!("fps={SAMPLE_FPS},scale={ANALYSIS_WIDTH}:{ANALYSIS_HEIGHT},format=gray");

    let args = FfmpegArgs::new()
        .args(["-v", "error"])
        .input(video_path)
        .args([
            "-an", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "gray", "-",
        ]);

    let output = run_checked(ffmpeg, &args).map_err(|e| ReframeError::SamplingFailed {
        path: video_path.display().to_string(),
        details: e.to_string(),
    })?;

    Ok(output
        .stdout
        .chunks_exact(frame_size)
        .map(|chunk| chunk.to_vec())
        .collect())
}

/// Turns a sequence of sampled grayscale frames into `SubjectPosition`s, one
/// per consecutive frame pair, per this module's doc comment.
///
/// Fewer than two frames (a source too short to diff at all) is treated as
/// a degenerate-but-valid case: a single centered position at `t=0`, rather
/// than an error — a caller downstream (crop-window computation) still gets
/// a usable, if uninteresting, result.
///
/// `width`/`height` are the dimensions of every frame in `frames` (each must
/// be exactly `width * height` bytes) — a parameter rather than the
/// `ANALYSIS_WIDTH`/`ANALYSIS_HEIGHT` constants directly, so this function's
/// own unit tests can exercise it against small, easy-to-hand-construct
/// frames instead of full-size 160x90 ones.
fn positions_from_frames(
    frames: &[GrayFrame],
    width: usize,
    height: usize,
) -> Vec<SubjectPosition> {
    if frames.len() < 2 {
        return vec![SubjectPosition {
            time_us: 0,
            target_x: 0.5,
            target_y: 0.5,
        }];
    }

    let mut last = (0.5f32, 0.5f32);
    let mut out = Vec::with_capacity(frames.len() - 1);
    for (i, pair) in frames.windows(2).enumerate() {
        if let Some(centroid) = motion_centroid(&pair[0], &pair[1], width, height) {
            last = centroid;
        }
        let time_us = (((i + 1) as f64 / SAMPLE_FPS as f64) * 1_000_000.0).round() as i64;
        out.push(SubjectPosition {
            time_us,
            target_x: last.0,
            target_y: last.1,
        });
    }
    out
}

/// Real per-pixel absolute luminance difference between two same-sized
/// grayscale frames, thresholded to suppress noise, reduced to the
/// diff-weighted centroid of the surviving pixels — "the weighted-average
/// position of the highest-motion region" (this pass's task brief).
///
/// Returns `None` when fewer than `MIN_MOTION_PIXELS` pixels cross
/// `MOTION_PIXEL_THRESHOLD` — a static (or below-noise-floor) frame pair,
/// which the caller treats as "carry the previous position forward" rather
/// than snapping to an arbitrary centroid of near-nothing.
fn motion_centroid(prev: &[u8], curr: &[u8], width: usize, height: usize) -> Option<(f32, f32)> {
    debug_assert_eq!(prev.len(), width * height);
    debug_assert_eq!(curr.len(), width * height);

    let mut qualifying_pixels = 0usize;
    let mut sum_weight = 0.0f64;
    let mut sum_x = 0.0f64;
    let mut sum_y = 0.0f64;

    for y in 0..height {
        let row = y * width;
        for x in 0..width {
            let idx = row + x;
            let diff = (prev[idx] as i16 - curr[idx] as i16).unsigned_abs();
            if diff > MOTION_PIXEL_THRESHOLD as u16 {
                qualifying_pixels += 1;
                let weight = diff as f64;
                sum_weight += weight;
                sum_x += weight * (x as f64 + 0.5);
                sum_y += weight * (y as f64 + 0.5);
            }
        }
    }

    if qualifying_pixels < MIN_MOTION_PIXELS || sum_weight <= 0.0 {
        return None;
    }

    let cx = (sum_x / sum_weight) / width as f64;
    let cy = (sum_y / sum_weight) / height as f64;
    Some((cx as f32, cy as f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::command::FfmpegArgs as Args;

    fn solid_frame(width: usize, height: usize, value: u8) -> GrayFrame {
        vec![value; width * height]
    }

    #[test]
    fn identical_frames_report_no_motion() {
        let a = solid_frame(10, 10, 50);
        let b = a.clone();
        assert_eq!(motion_centroid(&a, &b, 10, 10), None);
    }

    #[test]
    fn a_small_bright_patch_produces_a_centroid_at_its_own_location() {
        let width = 20;
        let height = 20;
        let mut a = solid_frame(width, height, 0);
        // A 4x4 bright patch near the top-left, well above the noise
        // threshold, comfortably above MIN_MOTION_PIXELS (16 pixels).
        for y in 2..6 {
            for x in 2..6 {
                a[y * width + x] = 255;
            }
        }
        let b = solid_frame(width, height, 0);

        let (cx, cy) = motion_centroid(&b, &a, width, height).expect("patch is real motion");
        // Patch spans columns/rows 2..6 -> center at (4.0, 4.0) in pixel
        // units -> normalized (4/20, 4/20) = (0.2, 0.2).
        assert!((cx - 0.2).abs() < 0.02, "{cx}");
        assert!((cy - 0.2).abs() < 0.02, "{cy}");
    }

    #[test]
    fn a_handful_of_noisy_pixels_below_the_count_threshold_is_not_motion() {
        let width = 20;
        let height = 20;
        let mut a = solid_frame(width, height, 0);
        // Only 2 pixels changed - below MIN_MOTION_PIXELS.
        a[0] = 255;
        a[1] = 255;
        let b = solid_frame(width, height, 0);
        assert_eq!(motion_centroid(&b, &a, width, height), None);
    }

    #[test]
    fn fewer_than_two_frames_yields_a_single_centered_fallback_position() {
        let positions = positions_from_frames(&[solid_frame(4, 4, 10)], 4, 4);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].time_us, 0);
        assert_eq!(positions[0].target_x, 0.5);
        assert_eq!(positions[0].target_y, 0.5);
    }

    #[test]
    fn a_static_run_of_frames_carries_the_last_known_position_forward() {
        // Motion once (frame 0 -> 1), then two static frames after -
        // positions for the static pairs must equal the position detected
        // at the one real motion event, not snap back to center.
        let width = 20;
        let height = 20;
        let still = solid_frame(width, height, 0);
        let mut moved = solid_frame(width, height, 0);
        for y in 10..14 {
            for x in 10..14 {
                moved[y * width + x] = 255;
            }
        }
        let frames = vec![still.clone(), moved.clone(), moved.clone(), moved.clone()];
        let positions = positions_from_frames(&frames, width, height);
        assert_eq!(positions.len(), 3);
        // All three should agree (the last two carry the first's position
        // forward since frames 1->2 and 2->3 show no further motion).
        for p in &positions {
            assert!((p.target_x - 0.6).abs() < 0.05, "{}", p.target_x);
            assert!((p.target_y - 0.6).abs() < 0.05, "{}", p.target_y);
        }
        // Timestamps are evenly spaced by 1/SAMPLE_FPS.
        let step_us = (1_000_000.0 / SAMPLE_FPS as f64).round() as i64;
        assert_eq!(positions[1].time_us - positions[0].time_us, step_us);
        assert_eq!(positions[2].time_us - positions[1].time_us, step_us);
    }

    /// The genuinely valuable test this pass's brief calls for: a *real*
    /// ffmpeg-synthesized video with a clear moving element (a 40x40 white
    /// box sliding left-to-right over a black background via the real
    /// `overlay` filter's time-varying `x` expression — two `lavfi` color
    /// sources composited with `overlay`, extending `media::scene`/
    /// `render::job`'s own `lavfi` synthetic-media-generation technique).
    ///
    /// An earlier version of this test used `drawbox`'s own `x`/`y`
    /// expression parameters instead of `overlay` — verified by hand (not
    /// just assumed) to silently fail on this project's actual ffmpeg build
    /// (6.1.1-3ubuntu5): neither `t` nor `n` resolves in a `drawbox` x/y
    /// expression there (the box is drawn at `x=INT_MIN`, i.e. entirely
    /// off-canvas, with no error). `overlay`'s `x`/`y` expressions accept
    /// `t` correctly on the same build, confirmed by directly inspecting
    /// `signalstats`' `YMAX` per frame before trusting this test to the
    /// tracker.
    #[test]
    fn tracks_a_real_moving_box_across_a_synthesized_video() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-reframe-motion-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("moving_box.mp4");

        // A 40x40 white box sliding from x=20 (its center at 40, 10% of a
        // 400-wide frame) to x=320 (center at 340, 85%) over 4 real
        // seconds, overlaid on a black 400x300 canvas, vertically fixed at
        // the frame's mid-height (`(300-40)/2 = 130`). `t` is `overlay`'s
        // own expression variable for the current timestamp in seconds.
        let gen_args = Args::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=black:size=400x300:duration=4:rate=25",
                "-f",
                "lavfi",
                "-i",
                "color=white:size=40x40:duration=4:rate=25",
                "-filter_complex",
                "[0:v][1:v]overlay=x='20+300*t/4':y=130",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args).expect("synthesizing a moving-box test video");

        let tracker = MotionTrackingSubjectTracker;
        let positions = tracker
            .track(&ffmpeg, &ffprobe, &source)
            .expect("tracking succeeds against a real synthetic moving-subject video");

        assert!(
            positions.len() >= 4,
            "expected several sampled positions, got {}",
            positions.len()
        );

        // The box's real x center at time t (seconds) is (20 + 300*t/4 + 20)
        // / 400 normalized - i.e. it moves from ~0.10 to ~0.925 over the
        // clip. The tracked target_x should follow that real rightward
        // trajectory: the first sample should sit well to the left of the
        // last sample.
        let first_x = positions.first().unwrap().target_x;
        let last_x = positions.last().unwrap().target_x;
        assert!(
            last_x - first_x > 0.3,
            "expected the tracked target to move meaningfully rightward: first={first_x} last={last_x}"
        );

        // Every sample's real ground-truth box position at that timestamp,
        // compared against the tracked target - generous tolerance for a
        // basic frame-difference technique (module doc comment), but tight
        // enough to prove real tracking, not a static/fallback response.
        for p in &positions {
            let t_seconds = p.time_us as f64 / 1_000_000.0;
            let ground_truth_x = (20.0 + 300.0 * (t_seconds / 4.0) + 20.0) / 400.0;
            assert!(
                (p.target_x as f64 - ground_truth_x).abs() < 0.20,
                "at t={t_seconds}s expected target_x near {ground_truth_x}, got {}",
                p.target_x
            );
            // The box never moves vertically - target_y should stay near
            // the frame's mid-height throughout.
            assert!(
                (p.target_y - 0.5).abs() < 0.15,
                "expected target_y to stay near mid-height, got {}",
                p.target_y
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn returns_no_video_stream_error_for_an_audio_only_file() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-reframe-audio-only-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("audio_only.wav");
        let gen_args = Args::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
            ])
            .path(&source);
        run_checked(&ffmpeg, &gen_args).expect("synthesizing an audio-only test file");

        let tracker = MotionTrackingSubjectTracker;
        let err = tracker.track(&ffmpeg, &ffprobe, &source).unwrap_err();
        assert!(matches!(err, ReframeError::NoVideoStream { .. }));

        std::fs::remove_dir_all(&dir).ok();
    }
}
