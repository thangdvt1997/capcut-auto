//! Transcription + Model Manager Tauri command surface (master prompt §14/
//! §60). Thin per master prompt §66 — all real logic lives in
//! `crate::transcription::{provider, whisper, models, download}`.
//!
//! Job lifecycle mirrors `commands::render`/`commands::media`'s background-
//! job pattern exactly: a `tauri::async_runtime::spawn_blocking` thread does
//! the real work, emits progress events, and a managed `job_id -> Arc<AtomicBool>`
//! map lets a `cancel_*` command flip the same flag the worker polls —
//! `models:download-progress` for model downloads,
//! `transcription:progress` for transcription jobs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::media::resolve_ffmpeg;
use crate::error::AppErrorPayload;
use crate::media::error::MediaError;
use crate::project::{Cut, TranscriptEntry};
use crate::transcription::filler::{self, FillerDictionary};
use crate::transcription::{self, ModelId};
use crate::vad::CutParams;

// ---------------------------------------------------------------------------
// Filler-word detection (master prompt §16) — thin wiring over
// `transcription::filler`'s pure, stateless candidate builder. No apply-side
// command here by design (see `transcription::filler` module doc comment):
// the frontend applies the returned `Cut`s through the existing
// `commands::timeline::apply_silence_cuts(_to_track)` commands, unmodified.
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn detect_filler_words(
    entries: Vec<TranscriptEntry>,
    dictionary: FillerDictionary,
    cut_params: CutParams,
) -> Vec<Cut> {
    filler::build_cuts_from_filler_words(&entries, &dictionary, cut_params)
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Model storage location (master prompt §60 "Storage location"):
/// `$APPLOCALDATA/models/` — this app's own managed data directory, the
/// same convention `commands::media`'s `media_cache_dir` uses for generated
/// thumbnails/proxies, **not** the repo-root `models/.gitkeep` placeholder
/// (that's a dev-tree placeholder only, never a runtime path).
fn models_dir(app: &AppHandle) -> Result<PathBuf, transcription::ModelError> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("models"))
        .map_err(|e| transcription::ModelError::StorageUnavailable {
            details: format!("resolving app local data dir: {e}"),
        })
}

// ---------------------------------------------------------------------------
// Model Manager: list / download / cancel / delete
// ---------------------------------------------------------------------------

/// Live model downloads: `model_id -> cancellation flag`. Starting a
/// download for a `model_id` already present here is a no-op (returns
/// `Ok(())` immediately) rather than an error — the caller's UI is already
/// subscribed to the same `models:download-progress` events the first
/// request kicked off, so there is nothing new to start.
#[derive(Default)]
pub struct ModelDownloadJobs(pub Mutex<HashMap<String, Arc<AtomicBool>>>);

/// Live transcription jobs: `job_id -> cancellation flag`.
#[derive(Default)]
pub struct TranscriptionJobs(pub Mutex<HashMap<String, Arc<AtomicBool>>>);

const MODEL_DOWNLOAD_PROGRESS_EVENT: &str = "models:download-progress";
const TRANSCRIPTION_PROGRESS_EVENT: &str = "transcription:progress";

#[derive(Debug, Clone, Serialize, Type)]
pub struct ModelDownloadProgressEvent {
    pub model_id: String,
    pub filename: String,
    pub size: u64,
    pub downloaded: u64,
    pub speed_bytes_per_sec: f64,
    pub eta_secs: Option<f64>,
    pub done: bool,
    pub error: Option<String>,
}

/// Catalog entry cross-referenced with what's actually on disk / actively
/// downloading (master prompt §60 "Available models").
#[derive(Debug, Clone, Serialize, Type)]
pub struct AvailableModel {
    pub entry: transcription::ModelCatalogEntry,
    pub installed: bool,
    pub download_in_progress: bool,
}

#[tauri::command]
#[specta::specta]
pub fn list_installed_models(
    app: AppHandle,
) -> Result<Vec<transcription::InstalledModel>, AppErrorPayload> {
    let dir = models_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(transcription::list_installed(&dir))
}

#[tauri::command]
#[specta::specta]
pub fn list_available_models(
    app: AppHandle,
    jobs: State<'_, ModelDownloadJobs>,
) -> Result<Vec<AvailableModel>, AppErrorPayload> {
    let dir = models_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let active = jobs.0.lock().expect("model download jobs mutex poisoned");
    Ok(transcription::catalog()
        .into_iter()
        .map(|entry| {
            let installed = transcription::is_installed(&dir, entry.id);
            let download_in_progress = active.contains_key(entry.id.as_str());
            AvailableModel {
                entry,
                installed,
                download_in_progress,
            }
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn download_model(
    app: AppHandle,
    jobs: State<'_, ModelDownloadJobs>,
    model_id: String,
) -> Result<(), AppErrorPayload> {
    let id = ModelId::from_str_id(&model_id).map_err(|e| AppErrorPayload::from(&e))?;
    let dir = models_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;

    {
        let mut guard = jobs.0.lock().expect("model download jobs mutex poisoned");
        if guard.contains_key(id.as_str()) {
            // Already downloading — the caller's UI is already subscribed
            // to the same progress event stream (module doc comment).
            return Ok(());
        }
        guard.insert(id.as_str().to_string(), Arc::new(AtomicBool::new(false)));
    }

    let cancel = jobs
        .0
        .lock()
        .expect("model download jobs mutex poisoned")
        .get(id.as_str())
        .cloned()
        .expect("just inserted");

    spawn_download_job(app, dir, id, cancel);
    Ok(())
}

fn spawn_download_job(app: AppHandle, dir: PathBuf, id: ModelId, cancel: Arc<AtomicBool>) {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = transcription::catalog_entry(id);
        let app_for_progress = app.clone();
        let filename = entry.filename.clone();
        let outcome =
            transcription::download_model(&entry, &dir, Some(cancel.as_ref()), move |p| {
                let _ = app_for_progress.emit(
                    MODEL_DOWNLOAD_PROGRESS_EVENT,
                    ModelDownloadProgressEvent {
                        model_id: id.as_str().to_string(),
                        filename: p.filename.clone(),
                        size: p.size,
                        downloaded: p.downloaded,
                        speed_bytes_per_sec: p.speed_bytes_per_sec,
                        eta_secs: p.eta_secs,
                        done: false,
                        error: None,
                    },
                );
            });

        if let Some(jobs) = app.try_state::<ModelDownloadJobs>() {
            if let Ok(mut guard) = jobs.0.lock() {
                guard.remove(id.as_str());
            }
        }

        match outcome {
            Ok(_) => {
                let _ = app.emit(
                    MODEL_DOWNLOAD_PROGRESS_EVENT,
                    ModelDownloadProgressEvent {
                        model_id: id.as_str().to_string(),
                        filename,
                        size: entry.approx_size_bytes,
                        downloaded: entry.approx_size_bytes,
                        speed_bytes_per_sec: 0.0,
                        eta_secs: Some(0.0),
                        done: true,
                        error: None,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    MODEL_DOWNLOAD_PROGRESS_EVENT,
                    ModelDownloadProgressEvent {
                        model_id: id.as_str().to_string(),
                        filename,
                        size: entry.approx_size_bytes,
                        downloaded: 0,
                        speed_bytes_per_sec: 0.0,
                        eta_secs: None,
                        done: true,
                        error: Some(e.to_string()),
                    },
                );
            }
        }
    });
}

#[tauri::command]
#[specta::specta]
pub fn cancel_model_download(
    jobs: State<'_, ModelDownloadJobs>,
    model_id: String,
) -> Result<(), AppErrorPayload> {
    let guard = jobs.0.lock().expect("model download jobs mutex poisoned");
    match guard.get(&model_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(AppErrorPayload::from(
            &transcription::ModelError::JobNotFound { model_id },
        )),
    }
}

#[tauri::command]
#[specta::specta]
pub fn delete_model(app: AppHandle, model_id: String) -> Result<(), AppErrorPayload> {
    let id = ModelId::from_str_id(&model_id).map_err(|e| AppErrorPayload::from(&e))?;
    let dir = models_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    transcription::delete_model(&dir, id).map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Transcription
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct TranscriptionProgressEvent {
    pub job_id: String,
    pub media_id: String,
    /// 0-100, `None` until whisper.cpp reports its first tick.
    pub percent: Option<i32>,
    pub done: bool,
    /// Populated only on the final, `done: true` event, success case.
    pub entries: Option<Vec<TranscriptEntry>>,
    pub error: Option<String>,
}

/// Starts a background transcription job against `media_path`'s audio
/// (extracted via `audio::pcm::extract_pcm`, the same 16kHz-mono pipeline
/// `vad` uses) using the installed model `model_id`. Returns a `job_id`
/// immediately; the real result — `Vec<TranscriptEntry>` with per-word
/// timestamps, `media_id` already filled in, `is_filler: false`, ready to
/// merge into `ProjectV1::transcript` — arrives via the final
/// `transcription:progress` event (`done: true`).
#[tauri::command]
#[specta::specta]
pub fn transcribe_media(
    app: AppHandle,
    jobs: State<'_, TranscriptionJobs>,
    media_id: String,
    media_path: String,
    model_id: String,
    language: Option<String>,
) -> Result<String, AppErrorPayload> {
    let id = ModelId::from_str_id(&model_id).map_err(|e| AppErrorPayload::from(&e))?;
    let dir = models_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    if !transcription::is_installed(&dir, id) {
        return Err(AppErrorPayload::from(
            &transcription::TranscriptionError::ModelNotInstalled {
                model_id: id.as_str().to_string(),
            },
        ));
    }
    if !Path::new(&media_path).exists() {
        return Err(AppErrorPayload::from(&MediaError::PathNotFound {
            path: media_path,
        }));
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = jobs.0.lock().expect("transcription jobs mutex poisoned");
        guard.insert(job_id.clone(), cancel.clone());
    }

    let model_path = dir.join(transcription::catalog_entry(id).filename);
    spawn_transcription_job(
        app,
        job_id.clone(),
        media_id,
        media_path,
        model_path,
        language,
        cancel,
    );
    Ok(job_id)
}

fn spawn_transcription_job(
    app: AppHandle,
    job_id: String,
    media_id: String,
    media_path: String,
    model_path: PathBuf,
    language: Option<String>,
    cancel: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let outcome = (|| -> Result<Vec<TranscriptEntry>, AppErrorPayload> {
            let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
            let samples = crate::audio::pcm::extract_pcm(&ffmpeg, Path::new(&media_path))
                .map_err(|e| AppErrorPayload::from(&e))?;

            let provider = transcription::WhisperProvider::load(&model_path)
                .map_err(|e| AppErrorPayload::from(&e))?;

            let job_id_for_progress = job_id.clone();
            let media_id_for_progress = media_id.clone();
            let app_for_progress = app.clone();
            let segments = provider
                .transcribe_with_progress(
                    &samples,
                    crate::audio::pcm::PCM_SAMPLE_RATE,
                    language.as_deref(),
                    Some(cancel.clone()),
                    move |percent| {
                        let _ = app_for_progress.emit(
                            TRANSCRIPTION_PROGRESS_EVENT,
                            TranscriptionProgressEvent {
                                job_id: job_id_for_progress.clone(),
                                media_id: media_id_for_progress.clone(),
                                percent: Some(percent),
                                done: false,
                                entries: None,
                                error: None,
                            },
                        );
                    },
                )
                .map_err(|e| AppErrorPayload::from(&e))?;

            Ok(segments
                .into_iter()
                .map(|s| TranscriptEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    media_id: media_id.clone(),
                    text: s.text,
                    start_us: s.start_us,
                    end_us: s.end_us,
                    confidence: s.confidence,
                    words: s.words,
                    is_filler: false,
                })
                .collect())
        })();

        if let Some(jobs) = app.try_state::<TranscriptionJobs>() {
            if let Ok(mut guard) = jobs.0.lock() {
                guard.remove(&job_id);
            }
        }

        match outcome {
            Ok(entries) => {
                let _ = app.emit(
                    TRANSCRIPTION_PROGRESS_EVENT,
                    TranscriptionProgressEvent {
                        job_id,
                        media_id,
                        percent: Some(100),
                        done: true,
                        entries: Some(entries),
                        error: None,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    TRANSCRIPTION_PROGRESS_EVENT,
                    TranscriptionProgressEvent {
                        job_id,
                        media_id,
                        percent: None,
                        done: true,
                        entries: None,
                        error: Some(e.message),
                    },
                );
            }
        }
    });
}

#[tauri::command]
#[specta::specta]
pub fn cancel_transcription(
    jobs: State<'_, TranscriptionJobs>,
    job_id: String,
) -> Result<(), AppErrorPayload> {
    let guard = jobs.0.lock().expect("transcription jobs mutex poisoned");
    match guard.get(&job_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(AppErrorPayload::from(
            &transcription::TranscriptionError::JobNotFound { job_id },
        )),
    }
}
