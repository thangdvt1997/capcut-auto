//! Automated Pan ("Ken Burns effect") Tauri command surface (`STUDIO_PLAN.md`
//! Phase S5). Thin per master prompt §66 — all real logic lives in
//! `crate::pan` (pure trigger detection + keyframe generation).
//!
//! **Preview-only surface, deliberately** — unlike `commands::zoom`'s own
//! third command (`apply_auto_zoom_to_clip`, which commits generated
//! keyframes to a real clip via the undo-tracked `Command` machinery), this
//! module does not yet expose an equivalent "apply to project" command.
//! `zoom::generate_zoom_keyframes` only ever writes its own `"scale"`
//! property, so `timeline::zoom::apply_zoom_keyframes_to_clip` can safely
//! clear every existing `"scale"` keyframe on one clip before writing new
//! ones. `pan::generate_pan_keyframes` writes `position_x`/`position_y` —
//! properties also written by manual keyframe editing and by
//! `reframe::smoothing::keyframes_from_smoothed` (Shorts' own auto-reframe).
//! A naive "clear every `position_x`/`position_y` keyframe on this clip
//! before applying a pan" (the direct analogue of zoom's own approach) risks
//! silently destroying a clip's unrelated manual or reframe-smoothing
//! keyframes — a real data-loss risk that needs its own careful
//! investigation (exactly how those other two features already use
//! `position_x`/`position_y`, and how to distinguish "this clip's own prior
//! pan" from "this clip's unrelated position keyframes" before safely
//! replacing only the former) before an apply command can be built
//! correctly. Left as real, honest, deliberately-scoped-out future work
//! rather than risking a rushed, unverified data-loss bug — a caller can
//! still preview real pan keyframes via [`generate_pan_keyframes`] below,
//! and apply them manually via whatever general keyframe-editing surface
//! this app already has, same as any other keyframe today.

use crate::pan::{self, ImageClipCandidate, PanIntensity, PanTrigger};
use crate::project::Keyframe;

/// Pure trigger detection — real still-image clips long enough to pan
/// across (`pan` module doc comment's trigger-scope decision), against real,
/// caller-supplied candidate data (a project's own real clip/media/scale
/// info — never a hardcoded assumption).
#[tauri::command]
#[specta::specta]
pub fn generate_pan_triggers(
    candidates: Vec<ImageClipCandidate>,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<PanTrigger> {
    pan::image_clip_triggers(&candidates, canvas_width, canvas_height)
}

/// The pure function this phase's own required shape calls for, exposed
/// directly (mirrors `commands::zoom::generate_zoom_keyframes`'s own
/// "preview before committing" rationale) — real `position_x`/`position_y`
/// keyframes a caller can inspect, and, for now, apply manually.
#[tauri::command]
#[specta::specta]
pub fn generate_pan_keyframes(triggers: Vec<PanTrigger>, intensity: PanIntensity) -> Vec<Keyframe> {
    pan::generate_pan_keyframes(&triggers, intensity)
}
