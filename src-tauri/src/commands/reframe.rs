//! Auto-reframe Tauri command surface (Phase 11, master prompt §23). Thin
//! per master prompt §66 — all real logic lives in `crate::reframe`.
//!
//! [`run_auto_reframe`] carries every real step and takes already-resolved
//! `ffmpeg`/`ffprobe` paths rather than an `AppHandle` — the same
//! "the Tauri command is a one-line resolve-binaries + delegate to a plain
//! function" split `commands::highlights::run_detection` uses, so this
//! pass's tests can exercise the full real pipeline (real ffmpeg subprocess
//! calls against a real synthesized video) without needing to stand up a
//! Tauri `AppHandle` at all.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::error::AppErrorPayload;
use crate::ffmpeg::binaries;
use crate::media::probe;
use crate::project::Keyframe;
use crate::reframe::crop::{crop_windows_over_time, CropWindow};
use crate::reframe::error::ReframeError;
use crate::reframe::motion::MotionTrackingSubjectTracker;
use crate::reframe::provider::{SubjectPosition, SubjectTracker};
use crate::reframe::smoothing::{
    keyframes_from_smoothed, smooth_positions, DEFAULT_SMOOTHING_TAU_US,
};

/// Resolves both sidecar binaries under `ReframeError` (rather than reusing
/// `commands::media::resolve_ffmpeg`, which returns `MediaError` — a small,
/// deliberate duplication of that function's body, same shape, so this
/// subsystem's errors stay in its own error type end to end).
fn resolve_binaries(app: &AppHandle) -> Result<(PathBuf, PathBuf), ReframeError> {
    let resource_dir = app.path().resource_dir().ok();
    let ffmpeg = binaries::ffmpeg_path(resource_dir.as_deref()).map_err(|e| {
        ReframeError::BinaryNotFound {
            tool: "ffmpeg".into(),
            details: e.to_string(),
        }
    })?;
    let ffprobe = binaries::ffprobe_path(resource_dir.as_deref()).map_err(|e| {
        ReframeError::BinaryNotFound {
            tool: "ffprobe".into(),
            details: e.to_string(),
        }
    })?;
    Ok((ffmpeg, ffprobe))
}

/// Full auto-reframe result for one media file: the raw tracked positions,
/// the smoothed ones, the `project::Keyframe` entries derived from the
/// smoothed track, and the crop-window-over-time result derived from the
/// same smoothed track — every intermediate stage, not just the final
/// output, so a caller (and this pass's tests) can verify each step
/// independently.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AutoReframeResult {
    pub raw_positions: Vec<SubjectPosition>,
    pub smoothed_positions: Vec<SubjectPosition>,
    pub keyframes: Vec<Keyframe>,
    pub crop_windows: Vec<CropWindow>,
    pub source_width: u32,
    pub source_height: u32,
}

/// The real pipeline, parameterized over already-resolved binary paths
/// (module doc comment). `clip_id`/`clip_position_us` are only used to
/// place the returned `Keyframe`s (see `reframe::smoothing` module doc
/// comment for the absolute-time/half-canvas conventions they drive) — they
/// have no effect on tracking, smoothing, or crop-window computation
/// themselves.
#[allow(clippy::too_many_arguments)]
fn run_auto_reframe(
    ffmpeg: &Path,
    ffprobe: &Path,
    media_path: &Path,
    clip_id: &str,
    clip_position_us: i64,
    target_width: u32,
    target_height: u32,
    smoothing_tau_us: i64,
) -> Result<AutoReframeResult, ReframeError> {
    if target_width == 0 || target_height == 0 {
        return Err(ReframeError::InvalidTargetAspect {
            width: target_width,
            height: target_height,
        });
    }

    let probed = probe::probe(ffprobe, media_path).map_err(|e| ReframeError::ProbeFailed {
        path: media_path.display().to_string(),
        details: e.to_string(),
    })?;
    if probed.width == 0 || probed.height == 0 {
        return Err(ReframeError::InvalidSourceDimensions {
            path: media_path.display().to_string(),
        });
    }

    let tracker = MotionTrackingSubjectTracker;
    let raw_positions = tracker.track(ffmpeg, ffprobe, media_path)?;
    let smoothed_positions = smooth_positions(&raw_positions, smoothing_tau_us);
    let keyframes = keyframes_from_smoothed(&smoothed_positions, clip_id, clip_position_us);
    let crop_windows = crop_windows_over_time(
        &smoothed_positions,
        probed.width,
        probed.height,
        target_width,
        target_height,
    );

    Ok(AutoReframeResult {
        raw_positions,
        smoothed_positions,
        keyframes,
        crop_windows,
        source_width: probed.width,
        source_height: probed.height,
    })
}

/// Runs auto-reframe end-to-end for one real media file (master prompt
/// §23): motion-tracks the subject, smooths the track to prevent camera
/// jumping, and computes both real `project::Keyframe` position entries and
/// a real crop-window-over-time result for `target_width`x`target_height`
/// (e.g. `9`x`16` to convert a landscape source to portrait).
///
/// `clip_id`/`clip_position_us` place the returned keyframes on a specific
/// project clip (`smoothing::keyframes_from_smoothed`'s doc comment) — pass
/// the clip this reframe is being computed for and its on-timeline start.
/// `smoothing_tau_us`, if `None`, defaults to
/// `reframe::DEFAULT_SMOOTHING_TAU_US`.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub fn auto_reframe_media(
    app: AppHandle,
    media_path: String,
    clip_id: String,
    clip_position_us: i64,
    target_width: u32,
    target_height: u32,
    smoothing_tau_us: Option<i64>,
) -> Result<AutoReframeResult, AppErrorPayload> {
    let (ffmpeg, ffprobe) = resolve_binaries(&app).map_err(|e| AppErrorPayload::from(&e))?;
    run_auto_reframe(
        &ffmpeg,
        &ffprobe,
        Path::new(&media_path),
        &clip_id,
        clip_position_us,
        target_width,
        target_height,
        smoothing_tau_us.unwrap_or(DEFAULT_SMOOTHING_TAU_US),
    )
    .map_err(|e| AppErrorPayload::from(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::command::{run_checked, FfmpegArgs};

    /// Real synthesized moving-subject video (same `overlay`-based technique
    /// as `reframe::motion`'s own test — see that test's doc comment for why
    /// `overlay` was used instead of `drawbox`'s own x/y expressions) run
    /// through the *entire* pipeline this command wires together: tracker ->
    /// smoothing -> keyframes -> crop windows — this pass's required
    /// end-to-end test.
    #[test]
    fn end_to_end_pipeline_produces_a_sane_crop_window_sequence_for_a_real_moving_subject() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-reframe-e2e-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("moving_box.mp4");

        let gen_args = FfmpegArgs::new()
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

        let result = run_auto_reframe(
            &ffmpeg,
            &ffprobe,
            &source,
            "clip-1",
            0,
            9,
            16,
            DEFAULT_SMOOTHING_TAU_US,
        )
        .expect("the full auto-reframe pipeline succeeds against a real synthetic video");

        assert_eq!(result.source_width, 400);
        assert_eq!(result.source_height, 300);
        assert!(!result.raw_positions.is_empty());
        assert_eq!(result.smoothed_positions.len(), result.raw_positions.len());
        assert_eq!(result.keyframes.len(), result.smoothed_positions.len() * 2);
        assert_eq!(result.crop_windows.len(), result.smoothed_positions.len());

        // 9:16 target from a 400x300 (4:3) source: target aspect (0.5625)
        // is narrower than source aspect (1.333), so every crop window
        // keeps the full source height and derives a matching width.
        for window in &result.crop_windows {
            assert_eq!(window.height, 300);
            assert_eq!(window.width, (300.0 * 9.0 / 16.0f64).round() as u32);
            assert!(window.x + window.width <= result.source_width);
            assert!(window.y + window.height <= result.source_height);
        }

        // The crop should follow the box's real rightward movement: the
        // last window's x should sit meaningfully to the right of the
        // first's.
        let first_x = result.crop_windows.first().unwrap().x;
        let last_x = result.crop_windows.last().unwrap().x;
        assert!(
            last_x > first_x,
            "expected the crop window to move rightward following the tracked subject: first_x={first_x} last_x={last_x}"
        );

        // Keyframes are placed at clip_position_us=0, absolute time equals
        // each smoothed sample's own source-relative time_us.
        for (kf, sample) in result
            .keyframes
            .iter()
            .filter(|k| k.property == "position_x")
            .zip(result.smoothed_positions.iter())
        {
            assert_eq!(kf.time_offset_us, sample.time_us);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_zero_target_dimension_is_rejected_before_any_ffmpeg_work() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let err = run_auto_reframe(
            &ffmpeg,
            &ffprobe,
            Path::new("does-not-matter.mp4"),
            "clip-1",
            0,
            0,
            16,
            DEFAULT_SMOOTHING_TAU_US,
        )
        .unwrap_err();
        assert!(matches!(err, ReframeError::InvalidTargetAspect { .. }));
    }
}
