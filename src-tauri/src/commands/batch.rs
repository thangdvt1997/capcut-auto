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

/// Starts a **multi-template** batch (upgrade-plan §11 — master prompt's
/// own §11 worked example: one video through TikTok/YouTube Shorts/Facebook
/// Reel/Original produces 4 distinctly-named outputs; this command
/// generalizes that to N `media_paths` x M `template_ids`, producing N x M
/// `BatchJob`s in one batch). A sibling of [`start_batch`] rather than an
/// extension of its own signature — `start_batch`'s existing single-
/// `template_id`-in-`config` shape is left completely undisturbed for every
/// existing caller, and this command's own `config.template_id` (if the
/// caller sets one anyway) is ignored: each fanned-out job gets its own
/// `template_id` from `template_ids`, one job per `(media_paths[i],
/// template_ids[j])` pair — see `batch::manager::start_multi_template_batch`'s
/// doc comment for the full fan-out/naming/failure-isolation writeup.
///
/// Every `template_ids` entry is resolved (built-in or custom) **before**
/// any job is created — an unknown id fails this whole call up front with a
/// clear error, rather than leaving some jobs pre-doomed to fail
/// individually. Concurrency is unchanged from every other batch: one
/// dedicated worker thread processes this batch's N x M jobs strictly
/// sequentially (`batch::manager` module doc comment's concurrency model —
/// this pass does not relax it), and one job's failure never aborts the
/// others (each job's own `Failed` status/error is independent, exactly
/// like `start_batch`'s jobs).
#[tauri::command]
#[specta::specta]
pub fn start_multi_template_batch(
    app: AppHandle,
    manager: State<'_, BatchJobManager>,
    media_paths: Vec<String>,
    template_ids: Vec<String>,
    config: BatchPipelineConfig,
) -> Result<String, AppErrorPayload> {
    batch::manager::start_multi_template_batch(app, &manager, media_paths, template_ids, config)
        .map_err(|e| AppErrorPayload::from(&e))
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
