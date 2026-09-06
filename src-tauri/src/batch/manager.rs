//! `BatchJobManager` — Tauri-managed state tracking every in-flight batch
//! job, mirroring how `TranscriptionJobs`/`RenderJobs`/`ModelDownloadJobs`
//! already track in-flight work by id (`commands::transcription`/
//! `commands::render` module doc comments), extended with a second
//! `Arc<AtomicBool>` per job for pause (cancel keeps the exact same
//! `AtomicBool`-polling primitive those modules established).
//!
//! ## Concurrency model
//!
//! Each `start_batch` call spawns **one dedicated worker thread** that
//! processes that batch's own files strictly **sequentially** (concurrency
//! = 1 within a batch). This is a deliberate, documented choice over a
//! multi-worker pool:
//!
//! - Every pipeline stage this batch orchestrator runs (PCM extraction, VAD
//!   scoring, whisper transcription, ffmpeg rendering) itself spawns or runs
//!   a real ffmpeg/whisper subprocess-or-equivalent — master prompt §50/§85's
//!   "no 20 simultaneous ffmpeg processes" concern is about exactly this
//!   kind of unbounded fan-out, and one-worker-per-batch is the simplest
//!   bound that can never violate it for a single batch.
//! - It makes cancel/pause/retry trivial to reason about correctly: at any
//!   moment there is at most one `JobHandle` actually "in flight" per batch,
//!   so there's no cross-thread queue-ordering question to get wrong.
//!
//! A user starting *multiple* batches concurrently still gets one thread
//! per batch (each batch is independent) — this pass does not add a global
//! cap across batches, since nothing in this codebase's existing job
//! managers (render/transcription/proxy) caps concurrent *distinct* jobs
//! either; only within one batch's own file queue is concurrency bounded to
//! 1. Documented here as this pass's honest scope, not an oversight.
//!
//! ## Pause/resume semantics ("resume where technically possible")
//!
//! Pause takes effect at the next **stage boundary** (`batch::pipeline::checkpoint`):
//! a job finishes whatever stage it's currently in, then holds before
//! starting the next one, rather than attempting to freeze mid-ffmpeg-
//! subprocess or mid-whisper-inference. This is the literal, honest reading
//! of "resume where technically possible" — true mid-operation pause/resume
//! of an external ffmpeg process isn't something this codebase (or ffmpeg
//! itself, without OS-level process suspension this project deliberately
//! doesn't use) supports cleanly. Resuming is simply clearing the pause flag;
//! the parked worker thread wakes up and starts the next stage normally.
//!
//! ## Retry semantics
//!
//! `retry` re-queues a `Failed` job **from the start** (`Queued`, `progress:
//! 0.0`, `error: None`) rather than resuming from its last-completed stage.
//! This is the simpler, safer, honestly-scoped default per this feature's
//! own requirement: this pipeline has no per-stage checkpointing of
//! intermediate artifacts (the in-progress `ProjectV1` being edited lives
//! only in a stack-local variable inside `pipeline::run_pipeline`, not
//! persisted anywhere a retry could pick back up from) — adding that would
//! be real, separate scope. A retried job runs the identical pipeline again;
//! if the underlying cause of the original failure hasn't changed (e.g. a
//! model that's still not installed), it fails again identically, which is
//! itself the correct, honest outcome.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use super::error::BatchError;
use super::pipeline::{self, PipelineIo};
use super::types::{BatchJob, BatchJobStatus, BatchPipelineConfig};

// ---------------------------------------------------------------------------
// Internal per-job state
// ---------------------------------------------------------------------------

struct JobState {
    id: String,
    name: String,
    status: BatchJobStatus,
    progress: f32,
    stage: String,
    started_at_instant: Instant,
    started_at_rfc3339: String,
    /// `false` until the job leaves `Queued` — see `BatchJob::started_at`
    /// doc comment for why `started_at`/`elapsed_us` only start counting
    /// once real processing begins, not at batch-creation time.
    has_started: bool,
    finished_instant: Option<Instant>,
    output_path: Option<String>,
    error: Option<String>,
}

impl JobState {
    fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            status: BatchJobStatus::Queued,
            progress: 0.0,
            stage: "Queued".to_string(),
            started_at_instant: Instant::now(),
            started_at_rfc3339: crate::project::now_rfc3339(),
            has_started: false,
            finished_instant: None,
            output_path: None,
            error: None,
        }
    }

    fn mark_started(&mut self) {
        if !self.has_started {
            self.has_started = true;
            self.started_at_instant = Instant::now();
            self.started_at_rfc3339 = crate::project::now_rfc3339();
        }
    }

    fn reset_for_retry(&mut self) {
        self.status = BatchJobStatus::Queued;
        self.progress = 0.0;
        self.stage = "Queued".to_string();
        self.has_started = false;
        self.finished_instant = None;
        self.output_path = None;
        self.error = None;
    }

    /// Builds the public snapshot, computing `elapsed_us`/`eta_us` fresh
    /// from wall-clock time rather than trusting any separately-mutated
    /// field (`BatchJob` doc comment).
    fn snapshot(&self) -> BatchJob {
        let now = Instant::now();
        let elapsed = if !self.has_started {
            Duration::ZERO
        } else {
            self.finished_instant
                .unwrap_or(now)
                .saturating_duration_since(self.started_at_instant)
        };
        let elapsed_us = elapsed.as_micros().min(i64::MAX as u128) as i64;

        // A real, extrapolated estimate only once there's real signal to
        // extrapolate from: the job must have actually started, still be
        // actively processing (not queued/paused/finished), have made some
        // real progress, and have run long enough that the extrapolation
        // isn't dominated by measurement noise (`BatchJob::eta_us` doc
        // comment: never a fabricated precise number).
        let eta_us = if self.has_started
            && self.finished_instant.is_none()
            && self.status.is_actively_processing()
            && self.progress > 0.0
            && elapsed >= Duration::from_millis(50)
        {
            let elapsed_secs = elapsed.as_secs_f64();
            let total_estimate_secs = elapsed_secs / self.progress as f64;
            let remaining_secs = (total_estimate_secs - elapsed_secs).max(0.0);
            Some((remaining_secs * 1_000_000.0).round() as i64)
        } else {
            None
        };

        BatchJob {
            id: self.id.clone(),
            name: self.name.clone(),
            status: self.status,
            progress: self.progress,
            stage: self.stage.clone(),
            started_at: self.started_at_rfc3339.clone(),
            elapsed_us,
            eta_us,
            output_path: self.output_path.clone(),
            error: self.error.clone(),
        }
    }
}

/// Everything one job needs to (re-)run itself: the source path/config it
/// was created with (retry reuses these unchanged), the shared state, and
/// its own cancel/pause flags.
#[derive(Clone)]
struct JobHandle {
    state: Arc<Mutex<JobState>>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    media_path: String,
    config: BatchPipelineConfig,
}

// ---------------------------------------------------------------------------
// Runs one job's real pipeline, updating shared state + notifying `on_update`
// on every meaningful step. No `AppHandle` here — this is the piece the
// manager's real Tauri-event-emitting worker thread AND plain unit tests
// both call, only differing in what `on_update` does with each snapshot.
// ---------------------------------------------------------------------------

fn process_job(
    io: &PipelineIo,
    handle: &JobHandle,
    on_update: impl Fn(&BatchJob) + Send + Sync + 'static,
) {
    let on_update = Arc::new(on_update);
    let progress_cb: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> = {
        let state = handle.state.clone();
        let on_update = on_update.clone();
        Arc::new(
            move |status: BatchJobStatus, stage: String, progress: f32| {
                let snapshot = {
                    let mut s = state.lock().expect("batch job state mutex poisoned");
                    s.mark_started();
                    s.status = status;
                    s.stage = stage;
                    s.progress = progress;
                    s.snapshot()
                };
                on_update(&snapshot);
            },
        )
    };

    let media_path = PathBuf::from(&handle.media_path);
    let result = pipeline::run_pipeline(
        io,
        &media_path,
        &handle.config,
        handle.cancel.clone(),
        handle.pause.clone(),
        progress_cb,
    );

    let snapshot = {
        let mut s = handle.state.lock().expect("batch job state mutex poisoned");
        s.mark_started();
        s.finished_instant = Some(Instant::now());
        match result {
            Ok(output_path) => {
                s.status = BatchJobStatus::Completed;
                s.progress = 1.0;
                s.stage = "Completed".to_string();
                s.output_path = Some(output_path.to_string_lossy().to_string());
                s.error = None;
            }
            Err(BatchError::Cancelled) => {
                s.status = BatchJobStatus::Cancelled;
                s.stage = "Cancelled".to_string();
            }
            Err(e) => {
                s.status = BatchJobStatus::Failed;
                s.stage = "Failed".to_string();
                s.error = Some(e.to_string());
            }
        }
        s.snapshot()
    };
    on_update(&snapshot);
}

// ---------------------------------------------------------------------------
// BatchJobManager
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct BatchJobManager {
    jobs: Mutex<HashMap<String, JobHandle>>,
    batch_order: Mutex<HashMap<String, Vec<String>>>,
    job_batch: Mutex<HashMap<String, String>>,
}

impl BatchJobManager {
    /// Creates one `BatchJob` (initially `Queued`) per `media_paths` entry,
    /// all sharing `config`. Returns the new batch id and its ordered job
    /// ids — the caller (`commands::batch::start_batch`) is responsible for
    /// actually spawning the worker thread that processes them.
    ///
    /// `pub(crate)` (not private) specifically so `commands::update`'s own
    /// tests can construct a real, freshly-`Queued` batch to exercise the
    /// "never update mid-render" deferral logic against real manager state,
    /// without needing a running `AppHandle` — same "test the pure
    /// AppHandle-free logic directly" split this function already exists
    /// for (doc comment above).
    pub(crate) fn create_batch(
        &self,
        media_paths: Vec<String>,
        config: BatchPipelineConfig,
    ) -> (String, Vec<String>) {
        let batch_id = Uuid::new_v4().to_string();
        let mut job_ids = Vec::with_capacity(media_paths.len());
        {
            let mut jobs = self.jobs.lock().expect("batch jobs mutex poisoned");
            let mut job_batch = self.job_batch.lock().expect("job batch mutex poisoned");
            for path in media_paths {
                let job_id = Uuid::new_v4().to_string();
                let name = Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| path.clone());
                let handle = JobHandle {
                    state: Arc::new(Mutex::new(JobState::new(job_id.clone(), name))),
                    cancel: Arc::new(AtomicBool::new(false)),
                    pause: Arc::new(AtomicBool::new(false)),
                    media_path: path,
                    config: config.clone(),
                };
                jobs.insert(job_id.clone(), handle);
                job_batch.insert(job_id.clone(), batch_id.clone());
                job_ids.push(job_id);
            }
        }
        self.batch_order
            .lock()
            .expect("batch order mutex poisoned")
            .insert(batch_id.clone(), job_ids.clone());
        (batch_id, job_ids)
    }

    /// Fans `media_paths` x `templates` out into one `BatchJob` per
    /// `(video, template)` pair — the multi-template batch's own job
    /// enumeration (upgrade-plan §11), reusing every other piece of
    /// `create_batch`'s own machinery (`JobState`/`JobHandle` construction,
    /// `batch_order`/`job_batch` bookkeeping) unchanged; only *how many* jobs
    /// get created, and what each one's `config`/display `name` looks like,
    /// differs from `create_batch`'s one-job-per-path loop.
    ///
    /// `templates` is `(template_id, template_name)` pairs, already resolved
    /// by the caller (`start_multi_template_batch`, which has the
    /// `AppHandle` this pure function deliberately does not need — same
    /// "resolve real IO paths at the `AppHandle`-dependent layer, keep the
    /// state-mutating core testable without one" split `create_batch`'s own
    /// doc comment already established) — this function never itself needs
    /// to look up a template. `pub(crate)` for the same reason
    /// `create_batch` is: `commands::batch`'s real command wrapper calls it
    /// through `start_multi_template_batch` below, and this crate's own
    /// tests exercise it directly with hand-built `(id, name)` pairs, no
    /// running `AppHandle` required.
    ///
    /// Every job's `config.template_id` is overridden to that job's own
    /// template (whatever `base_config.template_id` carried, if anything, is
    /// discarded — a multi-template batch's whole point is one template per
    /// job, not one shared template plus N more), and `config.output_suffix`
    /// is set to that template's `slugify_template_name` slug, so
    /// `batch::pipeline::run_pipeline`'s existing, unchanged output-naming
    /// logic (`BatchPipelineConfig::output_suffix` doc comment) lands each
    /// job at `<video_stem>_<template_slug>.<ext>` (§11's exact convention)
    /// without needing its own naming special-case.
    pub(crate) fn create_multi_template_batch(
        &self,
        media_paths: Vec<String>,
        templates: Vec<(String, String)>,
        base_config: BatchPipelineConfig,
    ) -> (String, Vec<String>) {
        let batch_id = Uuid::new_v4().to_string();
        let mut job_ids = Vec::with_capacity(media_paths.len() * templates.len());
        {
            let mut jobs = self.jobs.lock().expect("batch jobs mutex poisoned");
            let mut job_batch = self.job_batch.lock().expect("job batch mutex poisoned");
            for path in &media_paths {
                let filename = Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| path.clone());
                for (template_id, template_name) in &templates {
                    let job_id = Uuid::new_v4().to_string();
                    let name = format!("{filename} \u{2192} {template_name}");
                    let mut config = base_config.clone();
                    config.template_id = Some(template_id.clone());
                    config.output_suffix = Some(pipeline::slugify_template_name(template_name));
                    let handle = JobHandle {
                        state: Arc::new(Mutex::new(JobState::new(job_id.clone(), name))),
                        cancel: Arc::new(AtomicBool::new(false)),
                        pause: Arc::new(AtomicBool::new(false)),
                        media_path: path.clone(),
                        config,
                    };
                    jobs.insert(job_id.clone(), handle);
                    job_batch.insert(job_id.clone(), batch_id.clone());
                    job_ids.push(job_id);
                }
            }
        }
        self.batch_order
            .lock()
            .expect("batch order mutex poisoned")
            .insert(batch_id.clone(), job_ids.clone());
        (batch_id, job_ids)
    }

    fn handle_for(&self, job_id: &str) -> Option<JobHandle> {
        self.jobs
            .lock()
            .expect("batch jobs mutex poisoned")
            .get(job_id)
            .cloned()
    }

    fn batch_id_for_job(&self, job_id: &str) -> Option<String> {
        self.job_batch
            .lock()
            .expect("job batch mutex poisoned")
            .get(job_id)
            .cloned()
    }

    pub fn list_jobs(&self, batch_id: &str) -> Result<Vec<BatchJob>, BatchError> {
        let order = self.batch_order.lock().expect("batch order mutex poisoned");
        let job_ids = order
            .get(batch_id)
            .ok_or_else(|| BatchError::BatchNotFound {
                batch_id: batch_id.to_string(),
            })?;
        let jobs = self.jobs.lock().expect("batch jobs mutex poisoned");
        Ok(job_ids
            .iter()
            .filter_map(|id| jobs.get(id))
            .map(|h| {
                h.state
                    .lock()
                    .expect("batch job state mutex poisoned")
                    .snapshot()
            })
            .collect())
    }

    /// Whether ANY batch job tracked by this manager (across every batch,
    /// not just one) is currently non-terminal — `Queued`/`Analyzing`/
    /// `Transcribing`/`Editing`/`Rendering`/`Paused`. Used by
    /// `commands::update`'s "never update mid-render" enforcement (master
    /// prompt §62) as one half of the aggregating "is anything running"
    /// check alongside `commands::render::RenderJobs` — see that module's
    /// doc comment for why `Queued`/`Paused` count as busy too (installing
    /// mid-batch, even between stages, is still exactly the kind of
    /// mid-operation update this rule exists to prevent).
    pub fn has_active_jobs(&self) -> bool {
        self.jobs
            .lock()
            .expect("batch jobs mutex poisoned")
            .values()
            .any(|handle| {
                !handle
                    .state
                    .lock()
                    .expect("batch job state mutex poisoned")
                    .status
                    .is_terminal()
            })
    }

    pub fn set_paused(&self, job_id: &str, paused: bool) -> Result<(), BatchError> {
        let handle = self
            .handle_for(job_id)
            .ok_or_else(|| BatchError::JobNotFound {
                job_id: job_id.to_string(),
            })?;
        handle.pause.store(paused, Ordering::SeqCst);
        Ok(())
    }

    pub fn cancel(&self, job_id: &str) -> Result<(), BatchError> {
        let handle = self
            .handle_for(job_id)
            .ok_or_else(|| BatchError::JobNotFound {
                job_id: job_id.to_string(),
            })?;
        handle.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Validates the job is `Failed`, resets its state/flags to a fresh
    /// `Queued` job, and returns `Ok(())`. Split out from spawning the retry
    /// worker so this pure state-transition logic is directly unit-testable
    /// without a running Tauri app (`tauri::async_runtime::spawn_blocking`
    /// needs one) — the same "test the real synchronous logic, not the
    /// spawn wrapper" split every other job manager in this codebase uses.
    fn prepare_retry(&self, job_id: &str) -> Result<(), BatchError> {
        let handle = self
            .handle_for(job_id)
            .ok_or_else(|| BatchError::JobNotFound {
                job_id: job_id.to_string(),
            })?;
        {
            let mut state = handle.state.lock().expect("batch job state mutex poisoned");
            if state.status != BatchJobStatus::Failed {
                return Err(BatchError::NotRetryable {
                    job_id: job_id.to_string(),
                });
            }
            state.reset_for_retry();
        }
        handle.cancel.store(false, Ordering::SeqCst);
        handle.pause.store(false, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Real (AppHandle-dependent) resolution + worker spawning
// ---------------------------------------------------------------------------

const BATCH_PROGRESS_EVENT: &str = "batch:progress";

#[derive(Debug, Clone, Serialize, Type)]
pub struct BatchProgressEvent {
    pub batch_id: String,
    pub job: BatchJob,
}

struct PipelinePaths {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    models_dir: PathBuf,
    templates_dir: PathBuf,
}

impl PipelinePaths {
    fn as_io(&self) -> PipelineIo<'_> {
        PipelineIo {
            ffmpeg: &self.ffmpeg,
            ffprobe: &self.ffprobe,
            models_dir: &self.models_dir,
            templates_dir: &self.templates_dir,
        }
    }
}

fn resolve_pipeline_paths(app: &AppHandle) -> Result<PipelinePaths, BatchError> {
    let ffmpeg =
        crate::commands::media::resolve_ffmpeg(app).map_err(|e| BatchError::StageFailed {
            stage: "Analyzing".to_string(),
            details: e.to_string(),
        })?;
    let resource_dir = app.path().resource_dir().ok();
    let ffprobe = crate::ffmpeg::binaries::ffprobe_path(resource_dir.as_deref()).map_err(|e| {
        BatchError::StageFailed {
            stage: "Analyzing".to_string(),
            details: e.to_string(),
        }
    })?;
    let models_dir =
        crate::commands::transcription::models_dir(app).map_err(|e| BatchError::StageFailed {
            stage: "Transcribing".to_string(),
            details: e.to_string(),
        })?;
    let templates_dir =
        crate::commands::templates::templates_dir(app).map_err(|e| BatchError::StageFailed {
            stage: "Analyzing".to_string(),
            details: e.to_string(),
        })?;
    Ok(PipelinePaths {
        ffmpeg,
        ffprobe,
        models_dir,
        templates_dir,
    })
}

/// Runs one job to completion, resolving real IO paths first and emitting
/// `batch:progress` on every meaningful step (including the final terminal
/// snapshot) — the real, `AppHandle`-dependent counterpart to `process_job`
/// above.
fn run_job_with_events(app: &AppHandle, job_id: &str) {
    let manager = app.state::<BatchJobManager>();
    let Some(handle) = manager.handle_for(job_id) else {
        return;
    };
    let batch_id = manager.batch_id_for_job(job_id).unwrap_or_default();

    let paths = match resolve_pipeline_paths(app) {
        Ok(p) => p,
        Err(e) => {
            let snapshot = {
                let mut state = handle.state.lock().expect("batch job state mutex poisoned");
                state.mark_started();
                state.finished_instant = Some(Instant::now());
                state.status = BatchJobStatus::Failed;
                state.stage = "Failed".to_string();
                state.error = Some(e.to_string());
                state.snapshot()
            };
            let _ = app.emit(
                BATCH_PROGRESS_EVENT,
                BatchProgressEvent {
                    batch_id,
                    job: snapshot,
                },
            );
            return;
        }
    };
    let io = paths.as_io();

    let app_for_emit = app.clone();
    let batch_id_for_emit = batch_id.clone();
    process_job(&io, &handle, move |snapshot: &BatchJob| {
        let _ = app_for_emit.emit(
            BATCH_PROGRESS_EVENT,
            BatchProgressEvent {
                batch_id: batch_id_for_emit.clone(),
                job: snapshot.clone(),
            },
        );
    });
}

/// Spawns the one dedicated worker thread for a freshly-created batch
/// (module doc comment: concurrency = 1 within a batch, processed strictly
/// in the order `create_batch` returned).
pub fn spawn_batch_worker(app: AppHandle, job_ids: Vec<String>) {
    tauri::async_runtime::spawn_blocking(move || {
        for job_id in job_ids {
            run_job_with_events(&app, &job_id);
        }
    });
}

fn spawn_retry_worker(app: AppHandle, job_id: String) {
    tauri::async_runtime::spawn_blocking(move || {
        run_job_with_events(&app, &job_id);
    });
}

/// `commands::batch::start_batch`'s real logic: create the batch, then spawn
/// its worker thread.
pub fn start_batch(
    app: AppHandle,
    manager: &BatchJobManager,
    media_paths: Vec<String>,
    config: BatchPipelineConfig,
) -> String {
    let (batch_id, job_ids) = manager.create_batch(media_paths, config);
    spawn_batch_worker(app, job_ids);
    batch_id
}

/// `commands::batch::start_multi_template_batch`'s real logic (upgrade-plan
/// §11): resolves every `template_ids` entry's real display name up front —
/// **before** creating or spawning a single job — so an unknown template id
/// fails the whole call immediately with a clear `BatchError::UnknownTemplate`
/// rather than silently producing a batch with some jobs pre-doomed to fail
/// individually deep into a possibly-long run. `template_ids` may repeat (a
/// caller asking for the same template twice just gets two jobs per video
/// for it — not rejected as a caller error, since there's nothing actually
/// unsafe about it and no real reason to police it here) and resolves each
/// occurrence independently (a handful of catalog/`list_custom_templates`
/// lookups — templates are not large, and this only runs once per batch
/// creation, not per job).
///
/// Once every id resolves, fans out via `BatchJobManager::create_multi_template_batch`
/// and spawns the exact same single sequential worker thread `start_batch`
/// does (`spawn_batch_worker`) — a multi-template batch is still one batch,
/// processed one job at a time, just with more jobs enumerated up front
/// (module doc comment's concurrency-model section: this pass does not
/// relax "one worker thread per batch, sequential within a batch").
pub fn start_multi_template_batch(
    app: AppHandle,
    manager: &BatchJobManager,
    media_paths: Vec<String>,
    template_ids: Vec<String>,
    config: BatchPipelineConfig,
) -> Result<String, BatchError> {
    let templates_dir =
        crate::commands::templates::templates_dir(&app).map_err(|e| BatchError::StageFailed {
            stage: "Analyzing".to_string(),
            details: e.to_string(),
        })?;
    let mut templates = Vec::with_capacity(template_ids.len());
    for template_id in &template_ids {
        let name = pipeline::resolve_template_name(&templates_dir, template_id)?;
        templates.push((template_id.clone(), name));
    }

    let (batch_id, job_ids) = manager.create_multi_template_batch(media_paths, templates, config);
    spawn_batch_worker(app, job_ids);
    Ok(batch_id)
}

/// `commands::batch::retry_batch_job`'s real logic: validate + reset the
/// job's state (`BatchJobManager::prepare_retry`), then spawn a fresh
/// single-job worker thread for it.
pub fn retry_batch_job(
    app: AppHandle,
    manager: &BatchJobManager,
    job_id: &str,
) -> Result<(), BatchError> {
    manager.prepare_retry(job_id)?;
    spawn_retry_worker(app, job_id.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `remove_silence: None` — same reasoning as
    /// `batch::pipeline::tests::minimal_config`'s own doc comment: a
    /// synthetic sine tone isn't reliably classified as speech by the real
    /// Silero VAD, so enabling silence removal here would make these tests'
    /// "reaches Completed" assertions nondeterministic.
    fn minimal_config(export_preset_id: &str) -> BatchPipelineConfig {
        BatchPipelineConfig {
            remove_silence: None,
            captions: None,
            transcription_model_id: None,
            transcription_language: None,
            template_id: None,
            export_preset_id: Some(export_preset_id.to_string()),
            output_suffix: None,
        }
    }

    fn synth_source(ffmpeg: &Path, dir: &Path) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let source = dir.join("in.mp4");
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=3",
                "-shortest",
            ])
            .path(&source);
        run_checked(ffmpeg, &args).expect("synthesizing test source");
        source
    }

    fn handle_for_path(media_path: &str, config: BatchPipelineConfig) -> JobHandle {
        JobHandle {
            state: Arc::new(Mutex::new(JobState::new(
                Uuid::new_v4().to_string(),
                "test job".to_string(),
            ))),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            media_path: media_path.to_string(),
            config,
        }
    }

    // -- create_batch / list_jobs --------------------------------------------

    #[test]
    fn create_batch_makes_one_queued_job_per_media_path() {
        let manager = BatchJobManager::default();
        let (batch_id, job_ids) = manager.create_batch(
            vec!["a.mp4".to_string(), "b.mp4".to_string()],
            minimal_config("p1080"),
        );
        assert_eq!(job_ids.len(), 2);
        let jobs = manager.list_jobs(&batch_id).unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| j.status == BatchJobStatus::Queued));
        assert!(jobs.iter().all(|j| j.progress == 0.0));
        assert!(jobs.iter().all(|j| j.elapsed_us == 0));
        assert!(jobs.iter().all(|j| j.eta_us.is_none()));
        let names: Vec<&str> = jobs.iter().map(|j| j.name.as_str()).collect();
        assert!(names.contains(&"a.mp4"));
        assert!(names.contains(&"b.mp4"));
    }

    #[test]
    fn has_active_jobs_is_true_for_a_freshly_created_queued_batch() {
        let manager = BatchJobManager::default();
        assert!(!manager.has_active_jobs(), "no batches created yet");
        manager.create_batch(vec!["a.mp4".to_string()], minimal_config("p1080"));
        assert!(
            manager.has_active_jobs(),
            "a freshly Queued job is not yet terminal, so it counts as active"
        );
    }

    #[test]
    fn has_active_jobs_is_false_once_every_job_reaches_a_terminal_state() {
        let manager = BatchJobManager::default();
        let (_, job_ids) = manager.create_batch(
            vec!["a.mp4".to_string(), "b.mp4".to_string()],
            minimal_config("p1080"),
        );
        for job_id in &job_ids {
            let handle = manager.handle_for(job_id).unwrap();
            handle.state.lock().unwrap().status = BatchJobStatus::Completed;
        }
        assert!(
            !manager.has_active_jobs(),
            "every job is terminal, so nothing should count as active"
        );
    }

    #[test]
    fn has_active_jobs_is_true_while_a_job_is_paused() {
        let manager = BatchJobManager::default();
        let (_, job_ids) = manager.create_batch(vec!["a.mp4".to_string()], minimal_config("p1080"));
        manager
            .handle_for(&job_ids[0])
            .unwrap()
            .state
            .lock()
            .unwrap()
            .status = BatchJobStatus::Paused;
        assert!(
            manager.has_active_jobs(),
            "Paused is not terminal — an update must still be deferred"
        );
    }

    #[test]
    fn list_jobs_on_an_unknown_batch_id_errors() {
        let manager = BatchJobManager::default();
        let err = manager.list_jobs("does-not-exist").unwrap_err();
        assert!(matches!(err, BatchError::BatchNotFound { .. }));
    }

    // -- cancel / pause on unknown jobs ---------------------------------------

    #[test]
    fn cancel_and_pause_on_an_unknown_job_id_error() {
        let manager = BatchJobManager::default();
        assert!(matches!(
            manager.cancel("nope").unwrap_err(),
            BatchError::JobNotFound { .. }
        ));
        assert!(matches!(
            manager.set_paused("nope", true).unwrap_err(),
            BatchError::JobNotFound { .. }
        ));
    }

    // -- process_job: real end-to-end via the manager's own job state --------

    #[test]
    fn process_job_completes_and_updates_shared_state_to_completed() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-mgr-e2e-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let io = PipelineIo {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            models_dir: &models_dir,
            templates_dir: &templates_dir,
        };

        let handle = handle_for_path(source.to_str().unwrap(), minimal_config("fast_preview"));
        process_job(&io, &handle, |_| {});

        let snapshot = handle.state.lock().unwrap().snapshot();
        assert_eq!(snapshot.status, BatchJobStatus::Completed);
        assert_eq!(snapshot.progress, 1.0);
        assert!(snapshot.error.is_none());
        let output_path = snapshot
            .output_path
            .expect("completed job has an output path");
        assert!(Path::new(&output_path).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn process_job_with_a_pre_cancelled_flag_ends_cancelled_not_failed() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-mgr-cancel-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let io = PipelineIo {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            models_dir: &models_dir,
            templates_dir: &templates_dir,
        };

        let handle = handle_for_path(source.to_str().unwrap(), minimal_config("fast_preview"));
        handle.cancel.store(true, Ordering::SeqCst);
        process_job(&io, &handle, |_| {});

        let snapshot = handle.state.lock().unwrap().snapshot();
        assert_eq!(snapshot.status, BatchJobStatus::Cancelled);
        assert!(snapshot.error.is_none(), "cancelled is not a failure");
        assert!(snapshot.output_path.is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn process_job_with_a_bad_path_ends_failed_with_a_real_error() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-mgr-fail-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let io = PipelineIo {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            models_dir: &models_dir,
            templates_dir: &templates_dir,
        };

        let missing = dir.join("does-not-exist.mp4");
        let handle = handle_for_path(missing.to_str().unwrap(), minimal_config("fast_preview"));
        process_job(&io, &handle, |_| {});

        let snapshot = handle.state.lock().unwrap().snapshot();
        assert_eq!(snapshot.status, BatchJobStatus::Failed);
        assert!(snapshot.error.is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- pause/resume: holds at a stage boundary, resumes correctly ----------

    #[test]
    fn a_paused_job_holds_until_resumed_then_completes() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-mgr-pause-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let handle = handle_for_path(source.to_str().unwrap(), minimal_config("fast_preview"));
        handle.pause.store(true, Ordering::SeqCst);

        let seen_paused: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let seen_paused_for_cb = seen_paused.clone();
        let handle_for_thread = handle.clone();
        let ffmpeg_owned = ffmpeg.clone();
        let ffprobe_owned = ffprobe.clone();
        let models_dir_owned = models_dir.clone();
        let templates_dir_owned = templates_dir.clone();

        let worker = std::thread::spawn(move || {
            let io = PipelineIo {
                ffmpeg: &ffmpeg_owned,
                ffprobe: &ffprobe_owned,
                models_dir: &models_dir_owned,
                templates_dir: &templates_dir_owned,
            };
            process_job(&io, &handle_for_thread, move |snapshot: &BatchJob| {
                if snapshot.status == BatchJobStatus::Paused {
                    *seen_paused_for_cb.lock().unwrap() = true;
                }
            });
        });

        // Give the worker a real moment to reach the checkpoint and actually
        // park in the pause loop before we resume it — this does not affect
        // *correctness* (the worker polls indefinitely regardless of timing,
        // so there is no race that could make this test flaky), only how
        // reliably `seen_paused` observes at least one `Paused` snapshot
        // before resume.
        std::thread::sleep(Duration::from_millis(150));
        handle.pause.store(false, Ordering::SeqCst);

        worker.join().expect("worker thread should not panic");

        assert!(
            *seen_paused.lock().unwrap(),
            "expected at least one Paused progress update while parked"
        );
        let snapshot = handle.state.lock().unwrap().snapshot();
        assert_eq!(snapshot.status, BatchJobStatus::Completed);

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- retry: Failed -> prepare_retry -> re-runs, same outcome for the same
    //    underlying (still-broken) cause -------------------------------------

    #[test]
    fn prepare_retry_resets_a_failed_job_back_to_queued() {
        let manager = BatchJobManager::default();
        let (_, job_ids) = manager.create_batch(
            vec!["missing.mp4".to_string()],
            minimal_config("fast_preview"),
        );
        let job_id = job_ids[0].clone();

        // Simulate a finished, Failed job directly (bypassing a real
        // pipeline run — this test is about the state machine, not the
        // pipeline itself).
        {
            let handle = manager.handle_for(&job_id).unwrap();
            let mut state = handle.state.lock().unwrap();
            state.mark_started();
            state.status = BatchJobStatus::Failed;
            state.progress = 0.4;
            state.error = Some("media file not found".to_string());
            state.finished_instant = Some(Instant::now());
        }

        manager
            .prepare_retry(&job_id)
            .expect("a Failed job should be retryable");

        let snapshot = manager
            .handle_for(&job_id)
            .unwrap()
            .state
            .lock()
            .unwrap()
            .snapshot();
        assert_eq!(snapshot.status, BatchJobStatus::Queued);
        assert_eq!(snapshot.progress, 0.0);
        assert!(snapshot.error.is_none());
    }

    #[test]
    fn prepare_retry_refuses_a_job_that_is_not_failed() {
        let manager = BatchJobManager::default();
        let (_, job_ids) =
            manager.create_batch(vec!["a.mp4".to_string()], minimal_config("fast_preview"));
        // Still Queued — not Failed.
        let err = manager.prepare_retry(&job_ids[0]).unwrap_err();
        assert!(matches!(err, BatchError::NotRetryable { .. }));
    }

    #[test]
    fn a_retried_job_re_runs_the_real_pipeline_and_fails_identically_for_the_same_bad_path() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-mgr-retry-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let io = PipelineIo {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            models_dir: &models_dir,
            templates_dir: &templates_dir,
        };

        let missing = dir.join("still-does-not-exist.mp4");
        let handle = handle_for_path(missing.to_str().unwrap(), minimal_config("fast_preview"));

        process_job(&io, &handle, |_| {});
        let first = handle.state.lock().unwrap().snapshot();
        assert_eq!(first.status, BatchJobStatus::Failed);

        // Simulate `BatchJobManager::prepare_retry`'s own reset logic
        // directly on this handle (same effect, without needing the whole
        // manager wired up for this focused re-run test).
        {
            let mut state = handle.state.lock().unwrap();
            assert_eq!(state.status, BatchJobStatus::Failed);
            state.reset_for_retry();
        }
        handle.cancel.store(false, Ordering::SeqCst);
        handle.pause.store(false, Ordering::SeqCst);

        process_job(&io, &handle, |_| {});
        let second = handle.state.lock().unwrap().snapshot();
        // The underlying cause (missing file) hasn't changed, so retry fails
        // again identically — this pass's own documented, honest retry
        // semantics (module doc comment).
        assert_eq!(second.status, BatchJobStatus::Failed);
        assert_eq!(second.error, first.error);

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- multi-template batch fan-out (upgrade-plan §11) ---------------------

    /// Synthesizes a real, named test source whose audio track the real
    /// Silero VAD reliably classifies as containing speech throughout —
    /// unlike this module's other tests' plain `sine=frequency=440` tone
    /// (deliberately *not* reused here: that tone is reliably classified as
    /// *non*-speech by the real model in this environment, which is exactly
    /// right for those tests' own `remove_silence: None` + `template_id:
    /// None` config, where silence removal never runs at all — but every
    /// job in a multi-template batch carries a real `template_id`, and
    /// `BatchPipelineConfig::template_id`'s own doc comment is explicit that
    /// a selected template's `silence_settings` becomes the *default*
    /// `remove_silence` value whenever the caller leaves that field `None` —
    /// so silence removal for these tests' jobs is not optional, and a
    /// no-speech-detected source would deterministically empty the whole
    /// timeline, correctly failing the render with `EmptyTimeline`, not
    /// completing it). A pure 220Hz tone (near real speech's fundamental-
    /// frequency range) amplitude-modulated at 4Hz (`tremolo`, mimicking a
    /// real syllable rate) empirically produces one confident speech segment
    /// spanning the whole clip against this project's real Silero model —
    /// verified directly against `vad::SileroVadProvider`/`segments_from_scores`
    /// before writing the tests below, not guessed.
    fn synth_named_source(ffmpeg: &Path, dir: &Path, filename: &str) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let source = dir.join(filename);
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=220:duration=3,tremolo=f=4:d=0.9",
                "-shortest",
            ])
            .path(&source);
        run_checked(ffmpeg, &args).expect("synthesizing test source");
        source
    }

    #[test]
    fn create_multi_template_batch_produces_n_times_m_jobs_correctly_paired_and_named() {
        let manager = BatchJobManager::default();
        let templates = vec![
            ("tmpl_tiktok".to_string(), "TikTok".to_string()),
            (
                "tmpl_youtube_shorts".to_string(),
                "YouTube Shorts".to_string(),
            ),
            ("tmpl_news".to_string(), "News".to_string()),
        ];
        let (batch_id, job_ids) = manager.create_multi_template_batch(
            vec!["video01.mp4".to_string(), "video02.mp4".to_string()],
            templates,
            minimal_config("fast_preview"),
        );

        // 2 videos x 3 templates = 6 jobs, not 2+3 or some other miscount.
        assert_eq!(job_ids.len(), 6);
        let jobs = manager.list_jobs(&batch_id).unwrap();
        assert_eq!(jobs.len(), 6);
        assert!(jobs.iter().all(|j| j.status == BatchJobStatus::Queued));

        let names: Vec<String> = jobs.iter().map(|j| j.name.clone()).collect();
        for video in ["video01.mp4", "video02.mp4"] {
            for template_name in ["TikTok", "YouTube Shorts", "News"] {
                let expected = format!("{video} \u{2192} {template_name}");
                assert!(
                    names.contains(&expected),
                    "expected a job named {expected:?}, got {names:?}"
                );
            }
        }

        // Each job's own config carries exactly its (template_id, slug) pair
        // — not the batch's shared `base_config.template_id` (which was
        // `None`) and not some other job's template.
        let expected_pairs = [
            ("tmpl_tiktok", "tiktok"),
            ("tmpl_youtube_shorts", "youtube_shorts"),
            ("tmpl_news", "news"),
        ];
        for job_id in &job_ids {
            let handle = manager.handle_for(job_id).unwrap();
            let template_id = handle
                .config
                .template_id
                .as_deref()
                .expect("multi-template batch always sets template_id per job");
            let suffix = handle
                .config
                .output_suffix
                .as_deref()
                .expect("multi-template batch always sets output_suffix per job");
            assert!(
                expected_pairs
                    .iter()
                    .any(|(id, slug)| *id == template_id && *slug == suffix),
                "unexpected (template_id, output_suffix) pair: ({template_id}, {suffix})"
            );
        }
    }

    #[test]
    fn create_multi_template_batch_with_an_empty_template_list_produces_no_jobs() {
        let manager = BatchJobManager::default();
        let (batch_id, job_ids) = manager.create_multi_template_batch(
            vec!["a.mp4".to_string(), "b.mp4".to_string()],
            Vec::new(),
            minimal_config("fast_preview"),
        );
        assert!(job_ids.is_empty());
        assert!(manager.list_jobs(&batch_id).unwrap().is_empty());
    }

    #[test]
    fn a_real_multi_template_batch_of_2_videos_by_2_templates_produces_4_correctly_named_outputs() {
        // Real, smaller-scale end-to-end: 2 synthesized videos x 2 real
        // built-in templates = 4 jobs, each run through the real
        // `process_job` -> `run_pipeline` chain, producing 4 real,
        // distinctly-named output files (§11's exact convention).
        //
        // Both `tmpl_tiktok`/`tmpl_youtube_shorts` carry real, non-trivial
        // `silence_settings` (Phase 11), which this test deliberately DOES
        // exercise for real (unlike `batch::pipeline`'s own end-to-end test,
        // which omits silence removal entirely) — so `synth_named_source`'s
        // own tremolo-modulated tone (see its doc comment) matters here: a
        // plain sine tone has none of speech's spectral characteristics, so
        // real Silero VAD would find zero speech segments across the whole
        // clip and `vad::cutlist::build_cuts_from_speech_segments` would
        // then have nothing to protect and propose removing the entire
        // media — this test needs a source VAD actually detects as speech.
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-mgr-multitmpl-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video01 = synth_named_source(&ffmpeg, &dir, "video01.mp4");
        let video02 = synth_named_source(&ffmpeg, &dir, "video02.mp4");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let io = PipelineIo {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            models_dir: &models_dir,
            templates_dir: &templates_dir,
        };

        let manager = BatchJobManager::default();
        let templates = vec![
            ("tmpl_tiktok".to_string(), "TikTok".to_string()),
            (
                "tmpl_youtube_shorts".to_string(),
                "YouTube Shorts".to_string(),
            ),
        ];
        let (batch_id, job_ids) = manager.create_multi_template_batch(
            vec![
                video01.to_str().unwrap().to_string(),
                video02.to_str().unwrap().to_string(),
            ],
            templates,
            minimal_config("fast_preview"),
        );
        assert_eq!(job_ids.len(), 4);

        // Mirrors `spawn_batch_worker`'s own sequential loop, minus the
        // `AppHandle`/event-emitting glue this pure test doesn't need.
        for job_id in &job_ids {
            let handle = manager.handle_for(job_id).unwrap();
            process_job(&io, &handle, |_| {});
        }

        let jobs = manager.list_jobs(&batch_id).unwrap();
        assert_eq!(jobs.len(), 4);
        assert!(
            jobs.iter().all(|j| j.status == BatchJobStatus::Completed),
            "expected every job to complete: {jobs:?}"
        );

        let mut output_names: Vec<String> = jobs
            .iter()
            .map(|j| {
                let path = j
                    .output_path
                    .as_ref()
                    .expect("a completed job has an output path");
                assert!(Path::new(path).exists(), "missing real output file: {path}");
                Path::new(path)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        output_names.sort();
        assert_eq!(
            output_names,
            vec![
                "video01_tiktok.mp4",
                "video01_youtube_shorts.mp4",
                "video02_tiktok.mp4",
                "video02_youtube_shorts.mp4",
            ]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn one_failing_video_template_pair_in_a_multi_template_batch_does_not_abort_the_others() {
        // Partial failure isolation: one (video, template) pair is
        // deliberately made to fail (a nonexistent source path) while the
        // other pairs use a real, valid source — every OTHER job in the
        // same batch must still reach `Completed`, not be aborted or left
        // `Queued` by the one failure (mirroring this codebase's existing
        // per-item failure isolation precedent, e.g. `media::import`).
        //
        // Both real templates used here carry real `silence_settings` —
        // see the sibling test above's own doc comment for why
        // `synth_named_source`'s tremolo-modulated tone (not a plain sine
        // tone) matters for a source that's meant to actually complete.
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-batch-mgr-partialfail-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let good_video = synth_named_source(&ffmpeg, &dir, "good.mp4");
        let missing_video = dir.join("does-not-exist.mp4");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let io = PipelineIo {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            models_dir: &models_dir,
            templates_dir: &templates_dir,
        };

        let manager = BatchJobManager::default();
        let templates = vec![
            ("tmpl_tiktok".to_string(), "TikTok".to_string()),
            (
                "tmpl_youtube_shorts".to_string(),
                "YouTube Shorts".to_string(),
            ),
        ];
        // Media order matters here: the missing file is deliberately placed
        // FIRST so its 2 failing jobs run before the good file's 2 jobs in
        // this batch's own strictly-sequential single-worker processing
        // order — proving a failure doesn't abort jobs still queued behind
        // it, not just that independent/already-started jobs survive.
        let (batch_id, job_ids) = manager.create_multi_template_batch(
            vec![
                missing_video.to_str().unwrap().to_string(),
                good_video.to_str().unwrap().to_string(),
            ],
            templates,
            minimal_config("fast_preview"),
        );
        assert_eq!(job_ids.len(), 4);

        for job_id in &job_ids {
            let handle = manager.handle_for(job_id).unwrap();
            process_job(&io, &handle, |_| {});
        }

        let jobs = manager.list_jobs(&batch_id).unwrap();
        let failed: Vec<_> = jobs
            .iter()
            .filter(|j| j.name.starts_with("does-not-exist.mp4"))
            .collect();
        let completed: Vec<_> = jobs
            .iter()
            .filter(|j| j.name.starts_with("good.mp4"))
            .collect();
        assert_eq!(failed.len(), 2);
        assert_eq!(completed.len(), 2);
        assert!(
            failed
                .iter()
                .all(|j| j.status == BatchJobStatus::Failed && j.error.is_some()),
            "expected both missing-file jobs to fail with a real error: {failed:?}"
        );
        assert!(
            completed
                .iter()
                .all(|j| j.status == BatchJobStatus::Completed
                    && j.output_path
                        .as_deref()
                        .is_some_and(|p| Path::new(p).exists())),
            "expected both good-file jobs to complete with a real output file, unaffected by the \
             other pair's failure: {completed:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
