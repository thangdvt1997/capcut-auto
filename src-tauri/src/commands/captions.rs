//! Caption Tauri command surface: generation from the current project's
//! transcript, the built-in style template catalog, and the correction
//! operations (split/merge/retime/find-replace/bulk-style). Thin per master
//! prompt §66 — all real logic lives in `crate::captions::{generate, styles}`
//! and `crate::timeline::captions`; this module only translates between
//! Tauri's IPC boundary and those pure functions/command-builders, the same
//! shape `commands::timeline` already uses.

use tauri::State;

use crate::captions::generate::{self, CaptionGenerationSettings};
use crate::captions::styles;
use crate::error::AppErrorPayload;
use crate::project::{CaptionStyle, ProjectV1};
use crate::timeline::captions::{self as caption_ops, CaptionSplitPoint, FindReplaceOptions};
use crate::timeline::command::{BatchCommand, Command, InsertCaptionCommand};
use crate::timeline::ops::find_track;
use crate::timeline::session::TimelineState;

use super::timeline::with_session;

// ---------------------------------------------------------------------------
// Style catalog
// ---------------------------------------------------------------------------

/// The six built-in caption style templates (Minimal/TikTok/Podcast/News/
/// Gaming/Karaoke, master prompt §26). Pure — no session required, same
/// pattern as `commands::render::list_render_presets`.
#[tauri::command]
#[specta::specta]
pub fn list_caption_templates() -> Vec<CaptionStyle> {
    styles::all_caption_templates()
}

/// Replaces the project's own `caption_styles` catalog wholesale. Applied
/// directly to the session (bypassing `Command`/`History`, the same way
/// `load_timeline_project` replaces the whole session) rather than through a
/// new `Command` primitive: `caption_styles` is a settings catalog (like
/// `RenderSettings`), not an independently-addressed timeline entity the way
/// `Caption`/`Clip` are — undoability for *caption* edits (the operations
/// below, which set a caption's `style_id`) is what actually matters for
/// timeline history; editing the style definitions themselves is a
/// settings-form edit, not a timeline edit.
#[tauri::command]
#[specta::specta]
pub fn set_caption_styles(
    state: State<'_, TimelineState>,
    styles: Vec<CaptionStyle>,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        session.project.caption_styles = styles;
        Ok(session.project.clone())
    })
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Generates captions from the current session project's own
/// `ProjectV1::transcript`, assigns them all to `track_id`, and inserts them
/// as one atomic undo step (a `Batch` of `InsertCaption`).
#[tauri::command]
#[specta::specta]
pub fn generate_captions(
    state: State<'_, TimelineState>,
    track_id: String,
    settings: CaptionGenerationSettings,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        find_track(&session.project, &track_id)?;
        let mut captions =
            generate::generate_captions_from_transcript(&session.project.transcript, &settings);
        for caption in &mut captions {
            caption.track_id = track_id.clone();
        }
        let commands = captions
            .into_iter()
            .map(|caption| Command::InsertCaption(InsertCaptionCommand { caption }))
            .collect();
        session.apply(Command::Batch(BatchCommand { commands }))?;
        Ok(session.project.clone())
    })
}

// ---------------------------------------------------------------------------
// Correction operations (master prompt §28)
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn split_caption(
    state: State<'_, TimelineState>,
    caption_id: String,
    split_point: CaptionSplitPoint,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = caption_ops::split_caption(&session.project, &caption_id, split_point)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn merge_captions(
    state: State<'_, TimelineState>,
    caption_ids: Vec<String>,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = caption_ops::merge_captions(&session.project, &caption_ids)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

/// Adjusts a caption's start/end boundary — the same command whether it
/// came from a precise numeric edit or a drag gesture on either edge.
/// `scale_words` decides whether per-word timing is proportionally rescaled
/// to the new span (see `timeline::captions::retime_caption` doc comment).
#[tauri::command]
#[specta::specta]
pub fn retime_caption(
    state: State<'_, TimelineState>,
    caption_id: String,
    new_start_us: i64,
    new_end_us: i64,
    scale_words: bool,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = caption_ops::retime_caption(
            &session.project,
            &caption_id,
            new_start_us,
            new_end_us,
            scale_words,
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn find_replace_captions(
    state: State<'_, TimelineState>,
    find: String,
    replace: String,
    options: FindReplaceOptions,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command =
            caption_ops::find_replace_captions(&session.project, &find, &replace, options)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

#[tauri::command]
#[specta::specta]
pub fn bulk_set_caption_style(
    state: State<'_, TimelineState>,
    caption_ids: Vec<String>,
    style_id: Option<String>,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command =
            caption_ops::bulk_set_caption_style(&session.project, &caption_ids, style_id)?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}
