//! Batch Processing Tauri command surface (master prompt §42/§43). Thin per
//! master prompt §66 — all real logic lives in `crate::batch::{manager,
//! pipeline, settings}`.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::batch::{
    self, BatchError, BatchJob, BatchJobManager, BatchPipelineConfig, DryRunResult,
    MaxConcurrentJobsSetting, WorkerPoolStatus,
};
use crate::commands::ai::AiProviderSettings;
use crate::error::AppErrorPayload;

/// `$APPLOCALDATA/batch_settings.json` — the on-disk home of the persisted
/// `max_concurrent_jobs` worker-pool-size setting (Phase D11,
/// `STUDIO_PLAN.md`). The exact same this-app's-own-data-directory
/// convention `commands::assets::assets_dir`/`commands::templates::templates_dir`/
/// `commands::automation::automation_dir` all use elsewhere, just one flat
/// file instead of a subdirectory of many items.
///
/// `pub(crate)`, not private: `lib.rs`'s own `setup` hook resolves this exact
/// same path to read the persisted value at startup, before spawning the
/// real worker pool (`batch::manager::spawn_worker_pool`) — never a second,
/// parallel resolution of where this file lives.
pub(crate) fn batch_settings_file(app: &AppHandle) -> Result<PathBuf, BatchError> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("batch_settings.json"))
        .map_err(|e| BatchError::SettingsStorageUnavailable {
            details: format!("resolving app local data dir: {e}"),
        })
}

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

/// Preview / Dry Run (upgrade spec §18, `UPGRADE_PLAN.md` Phase U3): runs the
/// real resolution/decision logic one batch job for `media_path` would run —
/// real probing, real template/export-preset resolution, real (cheap) VAD
/// analysis when silence removal would apply, and (optionally, when no
/// template was chosen and real `ai_settings` are given) a real AI Auto
/// Template recommendation — without ever rendering or actually
/// transcribing. See `batch::dry_run` module doc comment for the full
/// writeup of which analysis steps are real vs. estimated.
/// A real, live "Workers: N, Running: R, Queue: Q" snapshot (Phase D4a,
/// `STUDIO_PLAN.md`'s own worker/slot rearchitecture) — the backend surface
/// `promt.md`'s dashboard mock needs to honestly show real worker/queue
/// numbers, rather than a per-batch approximation. See
/// `batch::manager::WorkerPoolStatus` doc comment for what each field means.
#[tauri::command]
#[specta::specta]
pub fn get_worker_pool_status(manager: State<'_, BatchJobManager>) -> WorkerPoolStatus {
    manager.worker_pool_status()
}

/// Reads the persisted `max_concurrent_jobs` worker-pool-size setting (Phase
/// D11, `STUDIO_PLAN.md`) alongside the currently-*active* pool size the
/// running app was actually started with (`WorkerPoolStatus::workers`), so
/// the frontend can tell whether a just-saved change is still waiting for a
/// restart to take effect. See `batch::settings` module doc comment for why
/// this is a real, deliberately-chosen restart-to-apply setting rather than
/// a live-resizable one.
#[tauri::command]
#[specta::specta]
pub fn get_max_concurrent_jobs(
    app: AppHandle,
    manager: State<'_, BatchJobManager>,
) -> Result<MaxConcurrentJobsSetting, AppErrorPayload> {
    let path = batch_settings_file(&app).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(MaxConcurrentJobsSetting {
        persisted: batch::settings::load_max_concurrent_jobs(&path),
        active: manager.worker_pool_status().workers,
        min: batch::settings::MIN_MAX_CONCURRENT_JOBS,
        max: batch::settings::MAX_MAX_CONCURRENT_JOBS,
    })
}

/// Validates and persists a new `max_concurrent_jobs` value. Does **not**
/// resize the currently-running worker pool — a real, deliberate choice (see
/// `batch::settings` module doc comment): it only takes effect the next time
/// the app starts, when `lib.rs`'s own `setup` hook reads this same
/// persisted file before calling `batch::manager::spawn_worker_pool`. The
/// returned snapshot's `active` field will still show the pool's current
/// (unchanged) size, so the frontend can honestly show "restart to apply"
/// exactly when `persisted != active`.
#[tauri::command]
#[specta::specta]
pub fn set_max_concurrent_jobs(
    app: AppHandle,
    manager: State<'_, BatchJobManager>,
    value: usize,
) -> Result<MaxConcurrentJobsSetting, AppErrorPayload> {
    let path = batch_settings_file(&app).map_err(|e| AppErrorPayload::from(&e))?;
    batch::settings::save_max_concurrent_jobs(&path, value)
        .map_err(|e| AppErrorPayload::from(&e))?;
    Ok(MaxConcurrentJobsSetting {
        persisted: value,
        active: manager.worker_pool_status().workers,
        min: batch::settings::MIN_MAX_CONCURRENT_JOBS,
        max: batch::settings::MAX_MAX_CONCURRENT_JOBS,
    })
}

#[tauri::command]
#[specta::specta]
pub fn dry_run_batch_job(
    app: AppHandle,
    media_path: String,
    config: BatchPipelineConfig,
    ai_settings: Option<AiProviderSettings>,
) -> Result<DryRunResult, AppErrorPayload> {
    batch::dry_run::run_dry_run_for_media(&app, media_path, config, ai_settings)
}
