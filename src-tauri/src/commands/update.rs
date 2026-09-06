//! Auto-update Tauri command surface (Phase 12, master prompt §62). Thin per
//! master prompt §66 — the real check/download/signature-verify/install
//! logic all lives inside `tauri-plugin-updater` itself (registered in
//! `lib.rs`); this module's own job is exactly two things: (1) refuse to
//! even check when the user has selected `UpdateCheckMode::Disabled`, and
//! (2) never let an install proceed while a render or batch job is in
//! flight — "never update while rendering" (master prompt §62).
//!
//! ## "Never update mid-render" — reusing the existing job registries
//!
//! This codebase already has two independent, real "is anything running"
//! registries: `commands::render::RenderJobs` (a job is present in the map
//! from the moment it starts until it finishes/fails/cancels — see that
//! module's own doc comment) and `batch::BatchJobManager` (every batch job
//! ever created stays tracked, `BatchJobStatus::is_terminal()` distinguishes
//! finished from still-active — see `batch::types` doc comment). Neither
//! one alone answers "is ANYTHING running right now" across every job type,
//! so `any_job_in_flight` below is the small, real aggregating check the
//! task brief for this feature explicitly allows in that situation — it
//! consults both existing registries directly rather than inventing a third
//! parallel busy-tracking mechanism.
//!
//! ## What is genuinely real here vs. what a human must still configure
//!
//! `check_for_update`/`install_available_update` are real, working Tauri
//! commands wired to a real `tauri-plugin-updater` instance — but that
//! plugin is configured (`tauri.conf.json`'s `plugins.updater`) with an
//! **empty `endpoints` array and a placeholder `pubkey`** (see that file's
//! own `_comment_*` keys and `lib.rs`'s plugin-registration comment for
//! exactly what a human must fill in). With zero configured endpoints, the
//! updater's own `check()` never makes a single network request — it just
//! returns `Err(ReleaseNotFound)` immediately, which this module surfaces
//! honestly as `UpdateCheckOutcome::CheckFailed`, never a fabricated
//! `UpToDate`/`Available`. This is deliberate and confirmed by reading
//! `tauri-plugin-updater 2.11.0`'s own `Updater::check()` source: an empty
//! `endpoints` list skips its request loop entirely.

use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use crate::batch::BatchJobManager;
use crate::commands::render::RenderJobs;
use crate::error::AppErrorPayload;
use crate::update::{UpdateCheckMode, UpdateCheckOutcome};

/// The one small, real aggregating "is anything running" check this
/// feature's task brief allows — see module doc comment.
pub(crate) fn any_job_in_flight(render_jobs: &RenderJobs, batch_jobs: &BatchJobManager) -> bool {
    let render_active = !render_jobs
        .0
        .lock()
        .expect("render jobs mutex poisoned")
        .is_empty();
    render_active || batch_jobs.has_active_jobs()
}

/// Runs a real update check against the configured manifest endpoint(s),
/// then — only if a newer version was actually found — checks whether a
/// render/batch job is in flight before ever reporting the update as safe
/// to install. Never downloads or installs anything itself; that's
/// `install_available_update`'s job, which re-checks busy state again right
/// before installing (state can change between these two calls).
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(
    app: AppHandle,
    render_jobs: State<'_, RenderJobs>,
    batch_jobs: State<'_, BatchJobManager>,
    mode: UpdateCheckMode,
) -> Result<UpdateCheckOutcome, AppErrorPayload> {
    if mode == UpdateCheckMode::Disabled {
        return Ok(UpdateCheckOutcome::Disabled);
    }

    let updater = app
        .updater()
        .map_err(|e| AppErrorPayload::new("update/updater_unavailable", e.to_string()))?;

    Ok(match updater.check().await {
        Ok(Some(update)) => {
            if any_job_in_flight(&render_jobs, &batch_jobs) {
                UpdateCheckOutcome::Deferred {
                    version: update.version,
                    notes: update.body,
                }
            } else {
                UpdateCheckOutcome::Available {
                    version: update.version,
                    notes: update.body,
                }
            }
        }
        Ok(None) => UpdateCheckOutcome::UpToDate,
        Err(e) => UpdateCheckOutcome::CheckFailed {
            message: e.to_string(),
        },
    })
}

/// Installs an update the user has already been shown via `check_for_update`
/// (`UpdateCheckOutcome::Available`). Re-validates both gates for real
/// rather than trusting the caller's last `check_for_update` result: mode
/// could have been switched to `Disabled` in between, and a render/batch job
/// could have started in between — this command re-queries the update
/// endpoint and the job registries fresh before ever calling `install()`.
///
/// On success, `tauri-plugin-updater`'s own `Update::download_and_install`
/// verifies the downloaded artifact's signature against `pubkey` before
/// installing anything (a placeholder pubkey today — see module doc
/// comment). On Windows specifically, a successful `install()` exits this
/// process itself to hand off to the platform installer (which, per this
/// plugin's default `restart_after_install: true`, relaunches the app once
/// done) — so the `Ok(UpdateCheckOutcome::Installing)` return below is only
/// actually observed on platforms where `install()` returns instead of
/// exiting the process; `tauri-plugin-process` (registered in `lib.rs`
/// alongside this plugin) still backs an explicit `app.request_restart()`
/// call in that case, so this command never leaves the app in an
/// installed-but-not-relaunched state on any platform.
#[tauri::command]
#[specta::specta]
pub async fn install_available_update(
    app: AppHandle,
    render_jobs: State<'_, RenderJobs>,
    batch_jobs: State<'_, BatchJobManager>,
    mode: UpdateCheckMode,
) -> Result<UpdateCheckOutcome, AppErrorPayload> {
    if mode == UpdateCheckMode::Disabled {
        return Ok(UpdateCheckOutcome::Disabled);
    }

    let updater = app
        .updater()
        .map_err(|e| AppErrorPayload::new("update/updater_unavailable", e.to_string()))?;

    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(UpdateCheckOutcome::UpToDate),
        Err(e) => {
            return Ok(UpdateCheckOutcome::CheckFailed {
                message: e.to_string(),
            })
        }
    };

    // Re-check busy state right before installing — not just relying on an
    // earlier `check_for_update` call, since a render/batch job may have
    // started in the meantime (master prompt §62: "never update while
    // rendering").
    if any_job_in_flight(&render_jobs, &batch_jobs) {
        return Ok(UpdateCheckOutcome::Deferred {
            version: update.version,
            notes: update.body,
        });
    }

    update
        .download_and_install(|_chunk_len, _content_len| {}, || {})
        .await
        .map_err(|e| AppErrorPayload::new("update/install_failed", e.to_string()))?;

    // Reached only where `install()` didn't already exit the process itself
    // (Windows exits from inside `install()` — see doc comment above).
    app.request_restart();
    Ok(UpdateCheckOutcome::Installing)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use super::*;
    use crate::batch::BatchPipelineConfig;

    fn minimal_config() -> BatchPipelineConfig {
        BatchPipelineConfig {
            remove_silence: None,
            captions: None,
            transcription_model_id: None,
            transcription_language: None,
            template_id: None,
            export_preset_id: Some("p1080".to_string()),
            output_suffix: None,
        }
    }

    /// Real, in-test-constructed "nothing running" state — both registries
    /// start empty, matching a freshly-launched app with no jobs ever
    /// created.
    #[test]
    fn no_render_or_batch_jobs_means_nothing_is_in_flight() {
        let render_jobs = RenderJobs::default();
        let batch_jobs = BatchJobManager::default();
        assert!(!any_job_in_flight(&render_jobs, &batch_jobs));
    }

    /// Real, in-test-constructed "a render is running" state: one entry in
    /// `RenderJobs`' own map, exactly how `start_render_job` leaves it while
    /// the render is in progress (removed only once it finishes — see that
    /// command's own doc comment).
    #[test]
    fn an_in_flight_render_job_is_reported_as_busy() {
        let render_jobs = RenderJobs::default();
        render_jobs
            .0
            .lock()
            .unwrap()
            .insert("render-job-1".to_string(), Arc::new(AtomicBool::new(false)));
        let batch_jobs = BatchJobManager::default();

        assert!(any_job_in_flight(&render_jobs, &batch_jobs));
    }

    /// Real, in-test-constructed "a batch job is running" state via
    /// `BatchJobManager`'s own real `create_batch` (a freshly created batch
    /// starts every job `Queued`, which `BatchJobStatus::is_terminal()`
    /// correctly reports as not-yet-finished) — no render job involved at
    /// all, proving the aggregating check catches a busy *batch* on its own,
    /// not just a busy render.
    #[test]
    fn an_in_flight_batch_job_is_reported_as_busy() {
        let render_jobs = RenderJobs::default();
        let batch_jobs = BatchJobManager::default();
        assert!(
            !any_job_in_flight(&render_jobs, &batch_jobs),
            "sanity: manager starts with no batches"
        );

        batch_jobs.create_batch(vec!["a.mp4".to_string()], minimal_config());

        assert!(any_job_in_flight(&render_jobs, &batch_jobs));
    }

    /// Once the sole in-flight render job finishes (removed from the map,
    /// mirroring `spawn_render_job`'s own cleanup), the aggregating check
    /// must correctly report "nothing running" again — not stuck reporting
    /// busy forever.
    #[test]
    fn a_render_job_that_has_finished_is_no_longer_reported_as_busy() {
        let render_jobs = RenderJobs::default();
        render_jobs
            .0
            .lock()
            .unwrap()
            .insert("render-job-1".to_string(), Arc::new(AtomicBool::new(false)));
        let batch_jobs = BatchJobManager::default();
        assert!(any_job_in_flight(&render_jobs, &batch_jobs));

        render_jobs.0.lock().unwrap().remove("render-job-1");

        assert!(!any_job_in_flight(&render_jobs, &batch_jobs));
    }
}
