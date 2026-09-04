//! Render Tauri command surface (master prompt §32/§33/§69): list presets,
//! detect hardware encoders, start/cancel a render job. Thin per master
//! prompt §66 — all real logic lives in `crate::render::{graph, plan,
//! presets, hwaccel, job}`.
//!
//! Job lifecycle mirrors `commands::media`'s proxy-job pattern exactly
//! (module doc comment there): a background thread built from
//! `tauri::async_runtime::spawn_blocking` calls into `render::job::run_render_job`,
//! emitting `render:progress` Tauri events, while a managed
//! `RenderJobs` map of `job_id -> Arc<AtomicBool>` lets `cancel_render_job`
//! flip the same cancellation flag `ffmpeg::command::run_with_progress`
//! already polls (master prompt §44/§45) — killing the ffmpeg child and
//! deleting the partial output is `render::job::run_render_job`'s job, not
//! this command layer's.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::media::resolve_ffmpeg;
use crate::error::AppErrorPayload;
use crate::project::ProjectV1;
use crate::render::error::RenderError;
use crate::render::graph::build_render_graph;
use crate::render::hwaccel::{self, DetectedEncoder, EncoderBackend};
use crate::render::job::{run_render_job, RenderJobProgress};
use crate::render::plan::build_ffmpeg_plan;
use crate::render::presets::{self, RenderPreset, RenderSettings};

/// Live render jobs: `job_id -> cancellation flag`. A job is removed from
/// the map once it finishes (success, failure, or cancellation) — checking
/// `cancel_render_job` against a since-finished job returns `JobNotFound`
/// rather than silently no-op'ing, so the frontend can tell the difference
/// between "already done" and "cancel request lost".
#[derive(Default)]
pub struct RenderJobs(pub Mutex<HashMap<String, Arc<AtomicBool>>>);

const RENDER_PROGRESS_EVENT: &str = "render:progress";

#[derive(Debug, Clone, Serialize, Type)]
pub struct RenderProgressEvent {
    pub job_id: String,
    pub fraction: Option<f64>,
    pub speed: Option<f64>,
    pub done: bool,
    pub output_path: Option<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Presets / hardware detection
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn list_render_presets() -> Vec<RenderPreset> {
    presets::all_presets()
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct HardwareEncoderReport {
    pub encoders: Vec<DetectedEncoder>,
    /// The master prompt's own display example: `"Encoder: NVIDIA NVENC"` or
    /// `"Encoder: CPU — libx264"`, for the H.264 codec (the common case);
    /// the frontend can recompute this per-codec via `encoders` directly if
    /// the user has chosen H.265/VP9.
    pub active_encoder_label: String,
}

#[tauri::command]
#[specta::specta]
pub fn detect_hardware_encoders(app: AppHandle) -> Result<HardwareEncoderReport, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let encoders = hwaccel::detect_encoders(&ffmpeg).map_err(|e| AppErrorPayload::from(&e))?;
    let active_encoder_label = hwaccel::active_encoder_label(&encoders, presets::VideoCodec::H264);
    Ok(HardwareEncoderReport {
        encoders,
        active_encoder_label,
    })
}

// ---------------------------------------------------------------------------
// Render settings input (preset + independent overrides)
// ---------------------------------------------------------------------------

/// What the frontend actually sends: an optional preset id to seed from,
/// plus any fields the user overrode. `None` fields fall back to the
/// preset's value (or, with no preset, to the 1080p preset's defaults —
/// documented here rather than silently picking something arbitrary).
#[derive(Debug, Clone, serde::Deserialize, Type)]
pub struct RenderSettingsInput {
    pub preset_id: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<crate::project::Rational>,
    pub container: Option<presets::Container>,
    pub video_codec: Option<presets::VideoCodec>,
    pub x264_preset: Option<String>,
    pub crf: Option<u8>,
    pub video_bitrate_kbps: Option<u32>,
    pub audio_codec: Option<presets::AudioCodec>,
    pub audio_bitrate_kbps: Option<u32>,
    /// `None` = auto-detect the best available hardware encoder, falling
    /// back to software (master prompt §33). `Some(_)` requests a specific
    /// backend, itself falling back to software if that hardware isn't
    /// actually working on this machine (`hwaccel::resolve_backend_for_render`).
    pub hardware_encoder: Option<EncoderBackend>,
}

fn resolve_settings(
    input: &RenderSettingsInput,
    detected: &[DetectedEncoder],
) -> Result<RenderSettings, RenderError> {
    let mut settings = match &input.preset_id {
        Some(id) => presets::find_preset(id)?.settings,
        None => presets::find_preset("p1080")?.settings,
    };

    if let Some(v) = input.width {
        settings.width = v;
    }
    if let Some(v) = input.height {
        settings.height = v;
    }
    if let Some(v) = input.fps {
        settings.fps = v;
    }
    if let Some(v) = input.container {
        settings.container = v;
    }
    if let Some(v) = input.video_codec {
        settings.video_codec = v;
    }
    if let Some(v) = &input.x264_preset {
        settings.x264_preset = v.clone();
    }
    if input.crf.is_some() {
        settings.crf = input.crf;
        // An explicit CRF override with no explicit bitrate switches the
        // preset back to CRF mode, even if the preset itself was
        // bitrate-based (e.g. overriding YouTube 1080p's bitrate mode with
        // a CRF value) — otherwise the override would be silently ignored
        // by `plan`'s "crf wins if both are set" rule for the wrong reason
        // (the preset's stale bitrate would still be set too, which is
        // harmless since crf wins, but clearing it keeps the settings
        // struct honest about which mode is active).
        if input.video_bitrate_kbps.is_none() {
            settings.video_bitrate_kbps = None;
        }
    }
    if let Some(v) = input.video_bitrate_kbps {
        settings.video_bitrate_kbps = Some(v);
        if input.crf.is_none() {
            settings.crf = None;
        }
    }
    if let Some(v) = input.audio_codec {
        settings.audio_codec = v;
    }
    if let Some(v) = input.audio_bitrate_kbps {
        settings.audio_bitrate_kbps = v;
    }

    settings.hardware_encoder = Some(hwaccel::resolve_backend_for_render(
        input.hardware_encoder,
        detected,
    ));

    settings.validate()?;
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Start / cancel a render job
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn start_render_job(
    app: AppHandle,
    jobs: State<'_, RenderJobs>,
    project: ProjectV1,
    settings: RenderSettingsInput,
    output_path: String,
) -> Result<String, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;

    let detected = hwaccel::detect_encoders(&ffmpeg).map_err(|e| AppErrorPayload::from(&e))?;
    let resolved_settings =
        resolve_settings(&settings, &detected).map_err(|e| AppErrorPayload::from(&e))?;

    let graph = build_render_graph(&project).map_err(|e| AppErrorPayload::from(&e))?;
    let output = PathBuf::from(&output_path);
    let plan = build_ffmpeg_plan(&graph, &resolved_settings, &output)
        .map_err(|e| AppErrorPayload::from(&e))?;

    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = jobs.0.lock().expect("render jobs mutex poisoned");
        guard.insert(job_id.clone(), cancel.clone());
    }

    spawn_render_job(app, job_id.clone(), ffmpeg, plan, output, cancel);
    Ok(job_id)
}

fn spawn_render_job(
    app: AppHandle,
    job_id: String,
    ffmpeg: PathBuf,
    plan: crate::render::plan::RenderPlan,
    output: PathBuf,
    cancel: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn_blocking(move || {
        let job_id_for_progress = job_id.clone();
        let app_for_progress = app.clone();
        let outcome = run_render_job(
            &ffmpeg,
            &plan,
            &output,
            Some(cancel.as_ref()),
            move |p: RenderJobProgress| {
                let _ = app_for_progress.emit(
                    RENDER_PROGRESS_EVENT,
                    RenderProgressEvent {
                        job_id: job_id_for_progress.clone(),
                        fraction: p.fraction,
                        speed: p.speed,
                        done: false,
                        output_path: None,
                        error: None,
                    },
                );
            },
        );

        // Remove this job from the live map regardless of outcome — it is
        // no longer cancellable once it's finished.
        if let Some(jobs) = app.try_state::<RenderJobs>() {
            if let Ok(mut guard) = jobs.0.lock() {
                guard.remove(&job_id);
            }
        }

        match outcome {
            Ok(()) => {
                let _ = app.emit(
                    RENDER_PROGRESS_EVENT,
                    RenderProgressEvent {
                        job_id,
                        fraction: Some(1.0),
                        speed: None,
                        done: true,
                        output_path: Some(output.to_string_lossy().to_string()),
                        error: None,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    RENDER_PROGRESS_EVENT,
                    RenderProgressEvent {
                        job_id,
                        fraction: None,
                        speed: None,
                        done: true,
                        output_path: None,
                        error: Some(e.to_string()),
                    },
                );
            }
        }
    });
}

#[tauri::command]
#[specta::specta]
pub fn cancel_render_job(
    jobs: State<'_, RenderJobs>,
    job_id: String,
) -> Result<(), AppErrorPayload> {
    let guard = jobs.0.lock().expect("render jobs mutex poisoned");
    match guard.get(&job_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(AppErrorPayload::from(&RenderError::JobNotFound { job_id })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_settings_falls_back_to_1080p_when_no_preset_given() {
        let input = RenderSettingsInput {
            preset_id: None,
            width: None,
            height: None,
            fps: None,
            container: None,
            video_codec: None,
            x264_preset: None,
            crf: None,
            video_bitrate_kbps: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            hardware_encoder: None,
        };
        let settings = resolve_settings(&input, &[]).expect("resolves");
        assert_eq!((settings.width, settings.height), (1920, 1080));
        assert_eq!(settings.hardware_encoder, Some(EncoderBackend::Software));
    }

    #[test]
    fn resolve_settings_applies_overrides_on_top_of_a_preset() {
        let input = RenderSettingsInput {
            preset_id: Some("p1080".into()),
            width: None,
            height: None,
            fps: None,
            container: None,
            video_codec: None,
            x264_preset: None,
            crf: Some(15),
            video_bitrate_kbps: None,
            audio_codec: None,
            audio_bitrate_kbps: Some(256),
            hardware_encoder: None,
        };
        let settings = resolve_settings(&input, &[]).expect("resolves");
        assert_eq!(settings.crf, Some(15));
        assert_eq!(settings.audio_bitrate_kbps, 256);
        // Unoverridden fields keep the preset's values.
        assert_eq!((settings.width, settings.height), (1920, 1080));
    }

    #[test]
    fn resolve_settings_errors_on_an_unknown_preset_id() {
        let input = RenderSettingsInput {
            preset_id: Some("nope".into()),
            width: None,
            height: None,
            fps: None,
            container: None,
            video_codec: None,
            x264_preset: None,
            crf: None,
            video_bitrate_kbps: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            hardware_encoder: None,
        };
        assert!(resolve_settings(&input, &[]).is_err());
    }
}
