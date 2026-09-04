//! Timeline Tauri command surface: clip split/trim/move/delete/duplicate,
//! track lock/hide/mute/solo + effective-mute query, undo/redo, copy/paste,
//! and a stateless snap-query. Thin per master prompt §66 — all real logic
//! lives in `crate::timeline::{ops, clipboard, command, session}`.
//!
//! Every mutating command here takes the shared `TimelineState` (managed in
//! `lib.rs`'s `run()` setup, the same pattern `MediaLibrary` uses),
//! re-derives a `Command` from the *current* session project via
//! `crate::timeline::ops`/`clipboard`, applies it through the session's
//! bounded undo `History`, and returns the resulting `ProjectV1` so the
//! frontend never has to separately re-fetch it after every edit.

use std::collections::HashMap;

use tauri::State;

use crate::error::AppErrorPayload;
use crate::project::ProjectV1;
use crate::timeline::clipboard;
use crate::timeline::error::TimelineError;
use crate::timeline::ops;
use crate::timeline::session::{TimelineSession, TimelineState};

fn with_session<T>(
    state: &TimelineState,
    f: impl FnOnce(&mut TimelineSession) -> Result<T, TimelineError>,
) -> Result<T, AppErrorPayload> {
    let mut guard = state.0.lock().expect("timeline session mutex poisoned");
    let session = guard
        .as_mut()
        .ok_or(TimelineError::NoActiveProject)
        .map_err(|e| AppErrorPayload::from(&e))?;
    f(session).map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

/// Loads `project` into the timeline session, replacing whatever was there
/// (fresh undo history, empty clipboard). Called whenever the frontend
/// opens or creates a project.
#[tauri::command]
#[specta::specta]
pub fn load_timeline_project(state: State<'_, TimelineState>, project: ProjectV1) {
    let mut guard = state.0.lock().expect("timeline session mutex poisoned");
    *guard = Some(TimelineSession::new(project));
}

/// Fetches the current in-session project (e.g. after a reload, or just to
/// resync).
#[tauri::command]
#[specta::specta]
pub fn get_timeline_project(state: State<'_, TimelineState>) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| Ok(session.project.clone()))
}

// ---------------------------------------------------------------------------
// Clip operations
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn split_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    split_at_us: i64,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::split_clip(&session.project, &clip_id, split_at_us)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn trim_clip_start(
    state: State<'_, TimelineState>,
    clip_id: String,
    new_start_us: i64,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::trim_clip_start(&session.project, &clip_id, new_start_us)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn trim_clip_end(
    state: State<'_, TimelineState>,
    clip_id: String,
    new_end_us: i64,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::trim_clip_end(&session.project, &clip_id, new_end_us)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn move_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    target_track_id: String,
    new_position_us: i64,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::move_clip(
            &session.project,
            &clip_id,
            &target_track_id,
            new_position_us,
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn delete_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::delete_clip(&session.project, &clip_id)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

/// Deletes every clip in `clip_ids` as one atomic undo step — the
/// backend-side multi-select delete (master prompt §11's "composite/batch
/// command" requirement) so the frontend never needs N separate undo
/// entries for one multi-select action.
#[tauri::command]
#[specta::specta]
pub fn delete_clips(
    state: State<'_, TimelineState>,
    clip_ids: Vec<String>,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::delete_clips(&session.project, &clip_ids)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn duplicate_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    new_position_us: i64,
    target_track_id: Option<String>,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::duplicate_clip(
            &session.project,
            &clip_id,
            new_position_us,
            target_track_id.as_deref(),
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

// ---------------------------------------------------------------------------
// Track flags
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn set_track_locked(
    state: State<'_, TimelineState>,
    track_id: String,
    locked: bool,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::set_track_locked(&session.project, &track_id, locked)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn set_track_hidden(
    state: State<'_, TimelineState>,
    track_id: String,
    hidden: bool,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::set_track_hidden(&session.project, &track_id, hidden)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn set_track_muted(
    state: State<'_, TimelineState>,
    track_id: String,
    muted: bool,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::set_track_muted(&session.project, &track_id, muted)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn set_track_solo(
    state: State<'_, TimelineState>,
    track_id: String,
    solo: bool,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = ops::set_track_solo(&session.project, &track_id, solo)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

/// Pure query, keyed by track id: whether solo state on any audio track
/// currently makes this track's audio effectively muted (master prompt: "if
/// any track has solo = true, all non-solo audio tracks are effectively
/// muted"). For the render/preview layer to consult; not itself an undoable
/// edit.
#[tauri::command]
#[specta::specta]
pub fn effective_track_mute_state(
    state: State<'_, TimelineState>,
) -> Result<HashMap<String, bool>, AppErrorPayload> {
    with_session(&state, |session| {
        Ok(ops::effective_track_mute_state(&session.project.tracks))
    })
}

// ---------------------------------------------------------------------------
// Undo / redo
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn undo_timeline(state: State<'_, TimelineState>) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        session.undo()?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn redo_timeline(state: State<'_, TimelineState>) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        session.redo()?;
        Ok(session.project.clone())
    })
}

// ---------------------------------------------------------------------------
// Copy / paste
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn copy_clips(
    state: State<'_, TimelineState>,
    clip_ids: Vec<String>,
) -> Result<(), AppErrorPayload> {
    with_session(&state, |session| {
        let clipboard = clipboard::copy_clips(&session.project, &clip_ids)?;
        session.clipboard = Some(clipboard);
        Ok(())
    })
}

#[tauri::command]
#[specta::specta]
pub fn paste_clips(
    state: State<'_, TimelineState>,
    target_track_id: Option<String>,
    target_position_us: i64,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let clipboard = session
            .clipboard
            .clone()
            .ok_or(TimelineError::ClipboardEmpty)?;
        let command = clipboard::paste_clips(
            &session.project,
            &clipboard,
            target_track_id.as_deref(),
            target_position_us,
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

// ---------------------------------------------------------------------------
// Snap (stateless)
// ---------------------------------------------------------------------------

/// Given a target time and a set of candidate snap points (other clip
/// edges, the playhead, markers — supplied by the frontend, which owns that
/// UI-level knowledge), returns the nearest candidate within `threshold_us`,
/// or `None`. Pure function, no session required.
#[tauri::command]
#[specta::specta]
pub fn snap_to_candidates(target_us: i64, candidates: Vec<i64>, threshold_us: i64) -> Option<i64> {
    ops::snap_to_candidates(target_us, &candidates, threshold_us)
}
