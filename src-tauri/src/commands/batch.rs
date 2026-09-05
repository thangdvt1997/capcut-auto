//! Batch Processing Tauri command surface (master prompt §42/§43). Thin per
//! master prompt §66 — all real logic lives in `crate::batch::{manager,
//! pipeline}`.

use tauri::{AppHandle, State};

use crate::batch::{self, BatchJob, BatchJobManager, BatchPipelineConfig};
use crate::error::AppErrorPayload;

/// Starts a new batch: one `BatchJob` per `media_paths` entry, all `config`.
/// Returns the batch id immediately; per-job progress arrives via
/// `batch:progress` events, and `list_batch_jobs` can be polled at any time.
#[tauri::command]
#[specta::specta]
pub fn start_batch(
    app: AppHandle,
    manager: State<'_, BatchJobManager>,
    media_paths: Vec<String>,
    config: BatchPipelineConfig,
) -> String {
    batch::manager::start_batch(app, &manager, media_paths, config)
}

#[tauri::command]
#[specta::specta]
pub fn list_batch_jobs(
    manager: State<'_, BatchJobManager>,
    batch_id: String,
) -> Result<Vec<BatchJob>, AppErrorPayload> {
    manager
        .list_jobs(&batch_id)
        .map_err(|e| AppErrorPayload::from(&e))
}

#[tauri::command]
#[specta::specta]
pub fn pause_batch_job(
    manager: State<'_, BatchJobManager>,
    job_id: String,
) -> Result<(), AppErrorPayload> {
    manager
        .set_paused(&job_id, true)
        .map_err(|e| AppErrorPayload::from(&e))
}

#[tauri::command]
#[specta::specta]
pub fn resume_batch_job(
    manager: State<'_, BatchJobManager>,
    job_id: String,
) -> Result<(), AppErrorPayload> {
    manager
        .set_paused(&job_id, false)
        .map_err(|e| AppErrorPayload::from(&e))
}

#[tauri::command]
#[specta::specta]
pub fn cancel_batch_job(
    manager: State<'_, BatchJobManager>,
    job_id: String,
) -> Result<(), AppErrorPayload> {
    manager
        .cancel(&job_id)
        .map_err(|e| AppErrorPayload::from(&e))
}

#[tauri::command]
#[specta::specta]
pub fn retry_batch_job(
    app: AppHandle,
    manager: State<'_, BatchJobManager>,
    job_id: String,
) -> Result<(), AppErrorPayload> {
    batch::manager::retry_batch_job(app, &manager, &job_id).map_err(|e| AppErrorPayload::from(&e))
}
