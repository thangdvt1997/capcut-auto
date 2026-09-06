//! Video Processing History Tauri command surface (upgrade spec §21,
//! `UPGRADE_PLAN.md` Phase U3). Thin per master prompt §66 — all real
//! persistence/CRUD logic lives in `crate::history::io`; this file's own
//! logic is just resolving the shared `MediaLibrary` connection (reused
//! as-is for this table too — see `crate::history` module doc comment for
//! why) and, for the two "re-run" commands, delegating straight into
//! `BatchJobManager::create_batch` (Phase 11) exactly like any other new
//! batch start.
//!
//! §21's "Download output" and "View logs" actions map onto mechanisms that
//! already exist elsewhere, not new backend surface added here:
//! - **Download output**: the rendered file already sits at a
//!   `HistoryEntry::output_path` returned by `get_history_entry`/`list_history`
//!   — a frontend "reveal in folder" action against that real path, not a
//!   new command.
//! - **View logs**: `commands::diagnostics::get_logs_folder_path`/
//!   `open_logs_folder` (Phase 12) already expose the real structured-log
//!   location this whole app writes to.

use tauri::{AppHandle, State};

use crate::batch::{manager as batch_manager, BatchJobManager, BatchPipelineConfig};
use crate::db::MediaLibrary;
use crate::error::AppErrorPayload;
use crate::history::{self, HistoryEntry, HistoryError, RerunResult};

fn fetch_entry(
    library: &State<'_, MediaLibrary>,
    id: &str,
) -> Result<HistoryEntry, AppErrorPayload> {
    let conn = library.0.lock().expect("media library mutex poisoned");
    history::io::get_history_entry(&conn, id)
        .map_err(|e| AppErrorPayload::from(&e))?
        .ok_or_else(|| AppErrorPayload::from(&HistoryError::NotFound { id: id.to_string() }))
}

/// Newest-first, real `LIMIT`/`OFFSET` pagination (`history::io::list_history`'s
/// own doc comment). `limit`/`offset` default to `50`/`0` when omitted —
/// same "a bounded default, never an unbounded query" posture
/// `commands::media::search_media_library` already established.
#[tauri::command]
#[specta::specta]
pub fn list_history(
    library: State<'_, MediaLibrary>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<HistoryEntry>, AppErrorPayload> {
    let conn = library.0.lock().expect("media library mutex poisoned");
    history::io::list_history(&conn, limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(|e| AppErrorPayload::from(&e))
}

#[tauri::command]
#[specta::specta]
pub fn get_history_entry(
    library: State<'_, MediaLibrary>,
    id: String,
) -> Result<HistoryEntry, AppErrorPayload> {
    fetch_entry(&library, &id)
}

/// §21's "Clone settings": returns the exact `BatchPipelineConfig` that
/// shaped this history entry's own job — a caller can start a brand-new
/// batch with it (e.g. after tweaking a couple of fields), but this command
/// itself starts nothing.
#[tauri::command]
#[specta::specta]
pub fn clone_history_entry_settings(
    library: State<'_, MediaLibrary>,
    id: String,
) -> Result<BatchPipelineConfig, AppErrorPayload> {
    Ok(fetch_entry(&library, &id)?.execution_plan)
}

/// §21's "Re-run": re-queues the entry's original input through its own
/// `execution_plan` as a brand-new batch (`history::build_rerun_config` +
/// `BatchJobManager::create_batch`, Phase 11's real batch creation, reused
/// unchanged) — never a parallel "resume the old job" mechanism. The
/// original history entry is left completely untouched; the new job earns
/// its own fresh row once it finishes.
#[tauri::command]
#[specta::specta]
pub fn rerun_from_history(
    app: AppHandle,
    library: State<'_, MediaLibrary>,
    manager: State<'_, BatchJobManager>,
    id: String,
) -> Result<RerunResult, AppErrorPayload> {
    let entry = fetch_entry(&library, &id)?;
    let config = history::build_rerun_config(&entry);
    let (batch_id, job_ids) = manager.create_batch(vec![entry.input_path.clone()], config);
    batch_manager::spawn_batch_worker(app, job_ids.clone());
    Ok(RerunResult { batch_id, job_ids })
}

/// §21's "Run with another template": same as [`rerun_from_history`], but
/// `execution_plan.template_id` is swapped for `new_template_id` first
/// (`history::build_rerun_with_template_config`) — every other setting from
/// the original run (silence removal, captions, export preset) is kept.
#[tauri::command]
#[specta::specta]
pub fn rerun_from_history_with_template(
    app: AppHandle,
    library: State<'_, MediaLibrary>,
    manager: State<'_, BatchJobManager>,
    id: String,
    new_template_id: String,
) -> Result<RerunResult, AppErrorPayload> {
    let entry = fetch_entry(&library, &id)?;
    let config = history::build_rerun_with_template_config(&entry, new_template_id);
    let (batch_id, job_ids) = manager.create_batch(vec![entry.input_path.clone()], config);
    batch_manager::spawn_batch_worker(app, job_ids.clone());
    Ok(RerunResult { batch_id, job_ids })
}

/// Not one of §21's own listed actions, but a real primitive worth exposing
/// alongside the rest of this CRUD surface (a "delete this run" row action)
/// — see `history::io::delete_history_entry`'s own doc comment.
#[tauri::command]
#[specta::specta]
pub fn delete_history_entry(
    library: State<'_, MediaLibrary>,
    id: String,
) -> Result<(), AppErrorPayload> {
    let conn = library.0.lock().expect("media library mutex poisoned");
    history::io::delete_history_entry(&conn, &id).map_err(|e| AppErrorPayload::from(&e))
}
