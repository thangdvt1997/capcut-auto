//! Auto-Zoom Tauri command surface (master prompt §24). Thin per master
//! prompt §66 — all real logic lives in `crate::zoom` (pure trigger
//! detection + keyframe generation) and `crate::timeline::zoom` (wiring the
//! result into a real clip via the standard `Command`/undo machinery).

use tauri::State;

use crate::commands::timeline::with_session;
use crate::error::AppErrorPayload;
use crate::project::{Keyframe, ProjectV1};
use crate::timeline::session::TimelineState;
use crate::timeline::zoom as timeline_zoom;
use crate::zoom::{self, EmphasisWindow, ZoomIntensity, ZoomTrigger};

/// Pure trigger detection (`crate::zoom`'s exact required
/// `Vec<ZoomTrigger> + ZoomIntensity -> Vec<Keyframe>` pure-function
/// signature lives in `generate_zoom_keyframes` below; this command runs the
/// three real trigger detectors and merges their output) against real,
/// caller-supplied data: a media file's own detected `Scene`s (long static
/// scenes), manual marker timestamps, and real RMS-energy-scored candidate
/// windows (emphasized speech).
#[tauri::command]
#[specta::specta]
pub fn generate_zoom_triggers(
    scenes: Vec<crate::media::scene::Scene>,
    manual_marker_timestamps_us: Vec<i64>,
    emphasis_windows: Vec<EmphasisWindow>,
) -> Vec<ZoomTrigger> {
    let mut triggers = Vec::new();
    triggers.extend(zoom::static_scene_triggers(&scenes));
    triggers.extend(zoom::manual_marker_triggers(&manual_marker_timestamps_us));
    triggers.extend(zoom::emphasis_triggers(&emphasis_windows));
    zoom::merge_triggers(&triggers)
}

/// The pure function this phase's brief calls for, exposed directly as its
/// own command (in addition to being used internally by
/// [`apply_auto_zoom_to_clip`]) so a caller can preview generated keyframes
/// before committing them to a clip.
#[tauri::command]
#[specta::specta]
pub fn generate_zoom_keyframes(
    triggers: Vec<ZoomTrigger>,
    intensity: ZoomIntensity,
    clip_id: String,
) -> Vec<Keyframe> {
    zoom::generate_zoom_keyframes(&triggers, intensity, &clip_id)
}

/// Wires [`generate_zoom_keyframes`]'s pure output against a real clip's
/// data: generates the keyframes, then applies them to `clip_id` in the
/// live timeline session (`timeline::zoom::apply_zoom_keyframes_to_clip`),
/// going through the standard `Command`/undo machinery.
#[tauri::command]
#[specta::specta]
pub fn apply_auto_zoom_to_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    triggers: Vec<ZoomTrigger>,
    intensity: ZoomIntensity,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let keyframes = zoom::generate_zoom_keyframes(&triggers, intensity, &clip_id);
        let command =
            timeline_zoom::apply_zoom_keyframes_to_clip(&session.project, &clip_id, keyframes)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}
