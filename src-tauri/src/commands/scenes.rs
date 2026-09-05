//! Scene detection Tauri command surface (master prompt §25). Thin per
//! master prompt §66 — all real logic lives in `crate::media::scene`,
//! `crate::timeline::scenes`, and `crate::highlights::combine`.
//!
//! "Select scenes" (master prompt §25) is a frontend selection-state
//! concern — no command for it here, see `timeline::scenes` module doc
//! comment.

use std::path::Path;

use tauri::{AppHandle, State};

use crate::commands::media::resolve_ffmpeg;
use crate::commands::timeline::with_session;
use crate::error::AppErrorPayload;
use crate::highlights::{combine, Highlight};
use crate::media::scene::{self, Scene};
use crate::project::ProjectV1;
use crate::timeline::scenes as timeline_scenes;
use crate::timeline::session::TimelineState;

/// Real scene detection (master prompt §25's `Scene{start, end, thumbnail,
/// score}`) against one media file, each scene's thumbnail written into
/// `thumbnail_dir` (module doc comment / `media::scene::detect_scenes`).
#[tauri::command]
#[specta::specta]
pub fn detect_media_scenes(
    app: AppHandle,
    media_path: String,
    total_duration_us: i64,
    thumbnail_dir: String,
    threshold: Option<f32>,
) -> Result<Vec<Scene>, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    scene::detect_scenes(
        &ffmpeg,
        Path::new(&media_path),
        threshold.unwrap_or(scene::DEFAULT_SCENE_THRESHOLD),
        total_duration_us,
        Path::new(&thumbnail_dir),
    )
    .map_err(|e| AppErrorPayload::from(&e))
}

/// "Split at scenes" — splits `clip_id` at every given scene boundary that
/// falls strictly inside it (`timeline::scenes::split_clip_at_scenes`).
#[tauri::command]
#[specta::specta]
pub fn split_clip_at_scenes(
    state: State<'_, TimelineState>,
    clip_id: String,
    scene_boundaries_us: Vec<i64>,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = timeline_scenes::split_clip_at_scenes(
            &session.project,
            &clip_id,
            &scene_boundaries_us,
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

/// "Remove scenes" — cuts every given scene's span out of `clip_id`
/// (`timeline::scenes::remove_scenes_from_clip`, structurally a silence/
/// filler-word-style removal).
#[tauri::command]
#[specta::specta]
pub fn remove_scenes_from_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    scenes: Vec<Scene>,
    media_id: String,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = timeline_scenes::remove_scenes_from_clip(
            &session.project,
            &clip_id,
            &scenes,
            &media_id,
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

/// Same as [`remove_scenes_from_clip`], applied to every clip on
/// `track_id` (`timeline::scenes::remove_scenes_from_track`).
#[tauri::command]
#[specta::specta]
pub fn remove_scenes_from_track(
    state: State<'_, TimelineState>,
    track_id: String,
    scenes: Vec<Scene>,
    media_id: String,
) -> Result<ProjectV1, AppErrorPayload> {
    with_session(&state, |session| {
        let command = timeline_scenes::remove_scenes_from_track(
            &session.project,
            &track_id,
            &scenes,
            &media_id,
        )?;
        session.apply(command)?;
        Ok(session.project.clone())
    })
}

/// "Generate highlights from scenes" (master prompt §25's own checklist
/// item) — pure, no session needed (`highlights::combine::highlights_from_scenes`).
#[tauri::command]
#[specta::specta]
pub fn generate_highlights_from_scenes(
    scenes: Vec<Scene>,
    max_highlights: Option<usize>,
) -> Vec<Highlight> {
    combine::highlights_from_scenes(&scenes, max_highlights.unwrap_or(10))
}
