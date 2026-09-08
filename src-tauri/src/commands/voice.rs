//! Voice/TTS Tauri command surface (`promt.md` §9). Thin per master prompt
//! §66 — all real logic lives in `crate::voice::{provider, custom_api,
//! stub}`, following the exact same shape `commands::ai` already established
//! for `crate::ai::*` (`resolve_api_key`/`build_provider`/a connection-test
//! command that folds every failure into a `{success, message}` result
//! rather than a thrown error).
//!
//! ## `synthesize_speech` (STUDIO_PLAN.md Phase D16)
//!
//! Wires the already-real `VoiceProvider::synthesize` (`voice::custom_api::
//! CustomApiVoiceProvider::synthesize`, structurally correct and tested
//! against a mock server — see `voice::provider` module doc comment) through
//! to a real Tauri command, matching `commands::render`'s `RenderJobs`/
//! `commands::transcription`'s `TranscriptionJobs` background-job pattern
//! exactly: [`synthesize_speech`] resolves a real `output_path` under
//! `app_local_data_dir().join("voice_output")` (`voice_output_dir`, the same
//! pattern `commands::media`'s `media_cache_dir`/`commands::assets::assets_dir`
//! already use), returns a `job_id` immediately, and runs the real HTTP call
//! on a `tauri::async_runtime::spawn_blocking` background thread, emitting
//! exactly one terminal `voice:progress` event (`VoiceProgressEvent { done:
//! true, .. }`) on success, failure, or pre-start cancellation.
//!
//! **Honest progress model**: a single synthesize call is one HTTP
//! request/response, not an ffmpeg-style multi-tick process — there is no
//! real fractional progress to report mid-flight (`voice::custom_api`
//! doesn't and can't stream partial completion), so this deliberately does
//! *not* fabricate a percentage. The frontend is expected to treat "job_id
//! received, no terminal event yet" as its own local "running" state
//! (exactly the same convention `stores/render.svelte.ts`'s `isRendering`
//! already uses for `RenderProgressEvent`), and this command emits only the
//! real terminal outcome once it's known.
//!
//! **Honest cancellation granularity**: [`cancel_voice_job`] flips the same
//! `Arc<AtomicBool>` cancellation-flag convention `RenderJobs`/
//! `TranscriptionJobs` use, but `voice::custom_api::CustomApiVoiceProvider::
//! synthesize` takes no cancellation token of its own — it is a single
//! blocking `ureq` call with no polling hook inside it, unlike
//! `ffmpeg::command::run_with_progress` (which `render::job` already wires a
//! flag into) or `transcription::WhisperProvider::transcribe_with_progress`
//! (which polls its own flag between decode chunks). So cancellation here is
//! real but narrow: the background thread checks the flag once, immediately
//! before making the HTTP request; a cancel that lands in that (short, but
//! real) window skips the network call entirely and reports a cancelled
//! terminal event. A cancel requested *after* the HTTP request is already in
//! flight has no effect — the request runs to completion (or its own
//! natural failure) and that real outcome is reported, not a fabricated
//! "cancelled". This is a genuine, narrower guarantee than `RenderJobs`'
//! (which can interrupt a multi-second ffmpeg encode mid-run) — documented
//! here rather than silently claimed to be the same.
//!
//! ## Other commands here
//!
//! [`test_voice_connection`] calls the configured provider's `list_voices()`
//! as a lightweight reachability probe (cheaper than a real synthesis call,
//! and needs no caller-chosen text/voice_id) and folds every failure mode
//! (missing/invalid credential, unreachable server, malformed response, an
//! honestly-stubbed provider) into `VoiceConnectionTestResult { success:
//! false, .. }` rather than an `Err` — the same "plain pass/fail indicator,
//! no thrown error" contract `commands::ai::test_ai_connection` already
//! uses. [`list_voices`] is the real "Refresh Voices" (`promt.md` §9)
//! backing call — same provider construction, but returns the real voice
//! list (or a clear `Err`) instead of a boolean.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::ai::credentials;
use crate::error::AppErrorPayload;
use crate::voice::provider::{VoiceInfo, VoiceProvider, VoiceSynthesisSettings};
use crate::voice::{CustomApiVoiceProvider, GptSoVitsProvider, NtsGenAiProvider, VoiceError};

/// Which of `promt.md` §9's three named providers a configured profile
/// speaks. Mirrors `commands::ai::AiProviderKind`'s own shape/rationale —
/// the frontend needs to tell these apart (labeling, whether a key/URL is
/// normally required) even though only `CustomApi` has a real backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VoiceProviderKind {
    NtsGenAi,
    GptSoVits,
    CustomApi,
}

/// `promt.md` §9's Provider/Server-API-URL/API-Key settings — the
/// per-request shape every command below takes, same "frontend owns storing
/// this, backend never persists it" posture `commands::ai::AiProviderSettings`
/// already documents.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VoiceProviderSettings {
    pub provider: VoiceProviderKind,
    /// Server/API URL (`promt.md` §9). Ignored by the stub providers, which
    /// never make a network call at all.
    pub base_url: String,
    /// `None` for a keyless server, or when a stub provider is selected.
    pub credential_ref: Option<String>,
}

fn map_credential_error(err: crate::ai::error::AiProviderError) -> VoiceError {
    use crate::ai::error::AiProviderError;
    match err {
        AiProviderError::CredentialNotFound { credential_ref } => {
            VoiceError::CredentialNotFound { credential_ref }
        }
        AiProviderError::CredentialStoreFailed { details } => {
            VoiceError::CredentialStoreFailed { details }
        }
        // `CredentialStore::get` (the only call site below) can only ever
        // produce the two variants above (see `ai::credentials`'s own
        // implementations) — this arm exists so a future new
        // `AiProviderError` variant fails to compile here instead of
        // silently mismapping, rather than because it's reachable today.
        other => VoiceError::CredentialStoreFailed {
            details: other.to_string(),
        },
    }
}

/// Reuses `ai::credentials::default_store()` rather than a second secret
/// store — both subsystems store an opaque API key under an opaque
/// `credential_ref`, and `ai::credentials`'s own module doc comment's
/// Windows-Credential-Manager-vs-in-memory-fallback design applies exactly
/// as-is here; a voice provider's key is just another named entry in the
/// same store.
fn resolve_api_key(settings: &VoiceProviderSettings) -> Result<Option<String>, VoiceError> {
    match &settings.credential_ref {
        Some(credential_ref) => credentials::default_store()
            .get(credential_ref)
            .map(Some)
            .map_err(map_credential_error),
        None => Ok(None),
    }
}

/// Constructs the real (`CustomApi`) or honestly-stubbed (`NtsGenAi`/
/// `GptSoVits`) provider for `settings.provider` — infallible, unlike
/// `commands::ai::build_provider`, since no voice provider here requires an
/// API key to construct (`promt.md` §9 lists API Key as optional even for a
/// real server; the stub providers never look at `base_url`/`api_key` at
/// all).
fn build_provider(
    settings: &VoiceProviderSettings,
    api_key: Option<String>,
) -> Box<dyn VoiceProvider> {
    match settings.provider {
        VoiceProviderKind::CustomApi => Box::new(CustomApiVoiceProvider {
            base_url: settings.base_url.clone(),
            api_key,
        }),
        VoiceProviderKind::NtsGenAi => Box::new(NtsGenAiProvider),
        VoiceProviderKind::GptSoVits => Box::new(GptSoVitsProvider),
    }
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct VoiceConnectionTestResult {
    pub success: bool,
    /// Human-readable outcome — never a raw API key, on either path (same
    /// contract as `commands::ai::AiConnectionTestResult::message`).
    pub message: String,
}

#[tauri::command]
#[specta::specta]
pub fn test_voice_connection(settings: VoiceProviderSettings) -> VoiceConnectionTestResult {
    let api_key = match resolve_api_key(&settings) {
        Ok(key) => key,
        Err(e) => {
            return VoiceConnectionTestResult {
                success: false,
                message: e.to_string(),
            }
        }
    };
    let provider = build_provider(&settings, api_key);
    match provider.list_voices() {
        Ok(voices) => VoiceConnectionTestResult {
            success: true,
            message: format!(
                "Connected to {}. {} voice(s) available.",
                provider.name(),
                voices.len()
            ),
        },
        Err(e) => VoiceConnectionTestResult {
            success: false,
            message: e.to_string(),
        },
    }
}

/// "Refresh Voices" (`promt.md` §9) backing call.
#[tauri::command]
#[specta::specta]
pub fn list_voices(settings: VoiceProviderSettings) -> Result<Vec<VoiceInfo>, AppErrorPayload> {
    let api_key = resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider = build_provider(&settings, api_key);
    provider
        .list_voices()
        .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// synthesize_speech (STUDIO_PLAN.md Phase D16) — module doc comment above
// covers the honest progress/cancellation model in full.
// ---------------------------------------------------------------------------

/// Where synthesized audio files are written: `{app_local_data}/voice_output/`
/// — the exact `app_local_data_dir().join("voice_output")` pattern
/// `commands::media`'s `media_cache_dir`/`commands::assets::assets_dir`
/// already use for their own generated files. `pub(crate)`, not private, so
/// this pass's own tests (and any future caller needing to locate a past
/// synthesis output) can resolve it the same way, rather than re-deriving it.
pub(crate) fn voice_output_dir(app: &AppHandle) -> Result<PathBuf, VoiceError> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("voice_output"))
        .map_err(|e| VoiceError::StorageUnavailable {
            details: format!("resolving app local data dir: {e}"),
        })
}

/// Live voice synthesis jobs: `job_id -> cancellation flag`. Same shape as
/// `commands::render::RenderJobs`/`commands::transcription::TranscriptionJobs`
/// — a job is removed once it reaches a terminal state (success, failure, or
/// pre-start cancellation), so `cancel_voice_job` against a since-finished
/// job correctly reports `JobNotFound` rather than silently no-op'ing.
#[derive(Default)]
pub struct VoiceJobs(pub Mutex<HashMap<String, Arc<AtomicBool>>>);

const VOICE_PROGRESS_EVENT: &str = "voice:progress";

/// The one and only event a given `job_id` ever receives — always `done:
/// true` (module doc comment: no fabricated mid-flight percentage exists for
/// a single HTTP call). Exactly one of `output_path`/`error` is populated on
/// a non-cancelled outcome; `cancelled: true` means neither is (the HTTP call
/// was never made at all).
#[derive(Debug, Clone, Serialize, Type)]
pub struct VoiceProgressEvent {
    pub job_id: String,
    pub done: bool,
    pub cancelled: bool,
    pub output_path: Option<String>,
    pub duration_us: Option<i64>,
    pub error: Option<String>,
}

/// Pure event-shape builder for the "cancelled before the HTTP call started"
/// outcome — split out from [`spawn_voice_job`] so this pass's own tests can
/// verify the exact shape without spinning up a real provider/HTTP call.
fn cancelled_progress_event(job_id: String) -> VoiceProgressEvent {
    VoiceProgressEvent {
        job_id,
        done: true,
        cancelled: true,
        output_path: None,
        duration_us: None,
        error: None,
    }
}

/// Pure event-shape builder for a real `synthesize` outcome (success or
/// failure) — same "split out for direct testability" rationale as
/// [`cancelled_progress_event`].
fn outcome_progress_event(
    job_id: String,
    outcome: Result<crate::voice::provider::VoiceSynthesisOutput, VoiceError>,
) -> VoiceProgressEvent {
    match outcome {
        Ok(output) => VoiceProgressEvent {
            job_id,
            done: true,
            cancelled: false,
            output_path: Some(output.audio_path),
            duration_us: output.duration_us,
            error: None,
        },
        Err(e) => VoiceProgressEvent {
            job_id,
            done: true,
            cancelled: false,
            output_path: None,
            duration_us: None,
            error: Some(e.to_string()),
        },
    }
}

/// Pure job-map mutation shared by [`cancel_voice_job`] and this pass's own
/// tests (which construct a bare `VoiceJobs` directly, with no `AppHandle`/
/// `State` needed — the same reason `resolve_settings`/
/// `compute_voice_speech_segments` in `commands::render` are pure functions
/// tested directly rather than only through the `#[tauri::command]`
/// wrapper).
fn cancel_job_flag(jobs: &VoiceJobs, job_id: &str) -> Result<(), VoiceError> {
    let guard = jobs.0.lock().expect("voice jobs mutex poisoned");
    match guard.get(job_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(VoiceError::JobNotFound {
            job_id: job_id.to_string(),
        }),
    }
}

/// Starts a background voice synthesis job for `text` as `voice_id` (using
/// `settings` to construct the provider and `synthesis_settings` for
/// speed/pitch/volume/emotion/language) and returns a `job_id` immediately —
/// module doc comment for the full progress/cancellation contract. Provider
/// construction (credential lookup + `build_provider`) happens synchronously
/// here, matching `start_render_job`'s own "validate before spawning"
/// precedent: a bad `credential_ref` fails the call immediately with a clear
/// `Err`, rather than only surfacing via a job that starts and instantly
/// fails.
#[tauri::command]
#[specta::specta]
pub fn synthesize_speech(
    app: AppHandle,
    jobs: State<'_, VoiceJobs>,
    text: String,
    voice_id: String,
    settings: VoiceProviderSettings,
    synthesis_settings: VoiceSynthesisSettings,
) -> Result<String, AppErrorPayload> {
    let api_key = resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider = build_provider(&settings, api_key);

    let dir = voice_output_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppErrorPayload::from(&VoiceError::OutputWriteFailed {
            path: dir.to_string_lossy().to_string(),
            details: e.to_string(),
        })
    })?;
    let output_path = dir.join(format!("{}.wav", uuid::Uuid::new_v4()));

    let job_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = jobs.0.lock().expect("voice jobs mutex poisoned");
        guard.insert(job_id.clone(), cancel.clone());
    }

    spawn_voice_job(
        app,
        job_id.clone(),
        cancel,
        VoiceSynthesisRequest {
            provider,
            text,
            voice_id,
            synthesis_settings,
            output_path,
        },
    );
    Ok(job_id)
}

/// Everything [`spawn_voice_job`] needs to actually perform the synthesis
/// call, bundled into one struct purely to keep that function's own
/// argument count under clippy's `too_many_arguments` threshold — no
/// behavioral significance beyond that.
struct VoiceSynthesisRequest {
    provider: Box<dyn VoiceProvider>,
    text: String,
    voice_id: String,
    synthesis_settings: VoiceSynthesisSettings,
    output_path: PathBuf,
}

fn spawn_voice_job(
    app: AppHandle,
    job_id: String,
    cancel: Arc<AtomicBool>,
    request: VoiceSynthesisRequest,
) {
    tauri::async_runtime::spawn_blocking(move || {
        // Real, but narrow, cancellation (module doc comment): only checked
        // once, immediately before the HTTP call — never mid-request.
        let event = if cancel.load(Ordering::SeqCst) {
            cancelled_progress_event(job_id.clone())
        } else {
            let outcome = request.provider.synthesize(
                &request.text,
                &request.voice_id,
                &request.synthesis_settings,
                &request.output_path,
            );
            outcome_progress_event(job_id.clone(), outcome)
        };

        // Remove this job from the live map regardless of outcome — it is
        // no longer cancellable once it's finished (same convention as
        // `RenderJobs`/`TranscriptionJobs`).
        if let Some(jobs) = app.try_state::<VoiceJobs>() {
            if let Ok(mut guard) = jobs.0.lock() {
                guard.remove(&job_id);
            }
        }

        let _ = app.emit(VOICE_PROGRESS_EVENT, event);
    });
}

#[tauri::command]
#[specta::specta]
pub fn cancel_voice_job(jobs: State<'_, VoiceJobs>, job_id: String) -> Result<(), AppErrorPayload> {
    cancel_job_flag(&jobs, &job_id).map_err(|e| AppErrorPayload::from(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::spawn_one_shot;

    fn settings(provider: VoiceProviderKind, base_url: String) -> VoiceProviderSettings {
        VoiceProviderSettings {
            provider,
            base_url,
            credential_ref: None,
        }
    }

    fn voices_body_one_voice() -> String {
        r#"{"voices": [{"voice_id": "v1", "name": "Voice One", "gender": "female", "language": "en", "preview_url": null}]}"#.to_string()
    }

    #[test]
    fn test_voice_connection_reports_success_against_a_working_mock_server() {
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", voices_body_one_voice());
        let result = test_voice_connection(settings(VoiceProviderKind::CustomApi, base_url));
        assert!(result.success, "message: {}", result.message);
        assert!(result.message.contains("1 voice"));
    }

    #[test]
    fn test_voice_connection_reports_failure_against_an_unreachable_endpoint() {
        let dead_url = crate::ai::test_http::spawn_connection_refused();
        let result = test_voice_connection(settings(VoiceProviderKind::CustomApi, dead_url));
        assert!(!result.success);
    }

    #[test]
    fn test_voice_connection_reports_failure_against_a_non_2xx_response() {
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 500 Internal Server Error", "oops".to_string());
        let result = test_voice_connection(settings(VoiceProviderKind::CustomApi, base_url));
        assert!(!result.success);
    }

    #[test]
    fn test_voice_connection_reports_not_implemented_for_the_nts_gen_ai_stub() {
        let result = test_voice_connection(settings(
            VoiceProviderKind::NtsGenAi,
            "http://example.invalid".to_string(),
        ));
        assert!(!result.success);
        assert!(result.message.to_lowercase().contains("not implemented"));
    }

    #[test]
    fn test_voice_connection_reports_not_implemented_for_the_gpt_so_vits_stub() {
        let result = test_voice_connection(settings(
            VoiceProviderKind::GptSoVits,
            "http://example.invalid".to_string(),
        ));
        assert!(!result.success);
        assert!(result.message.to_lowercase().contains("not implemented"));
    }

    #[test]
    fn list_voices_command_round_trips_a_well_formed_mock_response() {
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", voices_body_one_voice());
        let voices =
            list_voices(settings(VoiceProviderKind::CustomApi, base_url)).expect("well-formed");
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].voice_id, "v1");
    }

    #[test]
    fn list_voices_command_surfaces_a_clear_error_for_a_malformed_mock_response() {
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", "not json".to_string());
        let err = list_voices(settings(VoiceProviderKind::CustomApi, base_url)).unwrap_err();
        assert_eq!(err.code, "VOICE_PROVIDER_INVALID_RESPONSE");
    }

    #[test]
    fn list_voices_command_surfaces_not_implemented_for_a_stub_provider() {
        let err = list_voices(settings(
            VoiceProviderKind::GptSoVits,
            "http://example.invalid".to_string(),
        ))
        .unwrap_err();
        assert_eq!(err.code, "VOICE_PROVIDER_NOT_IMPLEMENTED");
    }

    // -- synthesize_speech: provider construction (shared with test_voice_connection/
    //    list_voices above, exercised again here for this command's own settings shape) --

    #[test]
    fn build_provider_constructs_a_real_custom_api_provider_carrying_the_given_base_url_and_key() {
        let s = settings(
            VoiceProviderKind::CustomApi,
            "http://localhost:9999".to_string(),
        );
        let provider = build_provider(&s, Some("sk-test".to_string()));
        assert_eq!(provider.name(), "custom-api");
    }

    #[test]
    fn build_provider_constructs_the_honest_stub_for_nts_gen_ai_and_gpt_so_vits() {
        let s1 = settings(
            VoiceProviderKind::NtsGenAi,
            "http://example.invalid".to_string(),
        );
        let s2 = settings(
            VoiceProviderKind::GptSoVits,
            "http://example.invalid".to_string(),
        );
        assert!(build_provider(&s1, None)
            .synthesize(
                "hi",
                "v1",
                &VoiceSynthesisSettings::default(),
                std::path::Path::new("/nonexistent/dir/out.wav"),
            )
            .is_err());
        assert!(build_provider(&s2, None).list_voices().is_err());
    }

    // -- output-path resolution: the pure "voice_output/<uuid>.wav" naming
    //    convention, independent of the `AppHandle`-based directory resolver
    //    itself (no test harness for a real `AppHandle` exists anywhere in
    //    this codebase — matching `commands::media`/`commands::transcription`'s
    //    own precedent of only unit-testing the `AppHandle`-free pure logic). --

    #[test]
    fn synthesized_output_filenames_are_unique_wav_files_under_the_resolved_directory() {
        let dir = std::path::PathBuf::from("/app/local/data/voice_output");
        let a = dir.join(format!("{}.wav", uuid::Uuid::new_v4()));
        let b = dir.join(format!("{}.wav", uuid::Uuid::new_v4()));
        assert_ne!(
            a, b,
            "two synthesis calls must never collide on the same file"
        );
        assert_eq!(a.extension().unwrap(), "wav");
        assert!(a.starts_with(&dir));
    }

    // -- job-state transitions (`VoiceJobs`, shared by `synthesize_speech`/
    //    `cancel_voice_job`) --

    #[test]
    fn cancel_job_flag_flips_the_flag_for_a_live_job() {
        let jobs = VoiceJobs::default();
        let flag = Arc::new(AtomicBool::new(false));
        jobs.0
            .lock()
            .unwrap()
            .insert("job-1".to_string(), flag.clone());

        cancel_job_flag(&jobs, "job-1").expect("job-1 is live");
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_job_flag_reports_job_not_found_for_an_unknown_or_already_finished_job() {
        let jobs = VoiceJobs::default();
        let err = cancel_job_flag(&jobs, "never-existed").unwrap_err();
        assert!(matches!(err, VoiceError::JobNotFound { .. }));

        // Same outcome once a once-live job has been removed (the real
        // `spawn_voice_job` background thread's own "finished" cleanup step).
        let flag = Arc::new(AtomicBool::new(false));
        jobs.0.lock().unwrap().insert("job-2".to_string(), flag);
        jobs.0.lock().unwrap().remove("job-2");
        let err = cancel_job_flag(&jobs, "job-2").unwrap_err();
        assert!(matches!(err, VoiceError::JobNotFound { .. }));
    }

    // -- terminal event shape (pure, split out of `spawn_voice_job` for
    //    exactly this reason) --

    #[test]
    fn cancelled_progress_event_reports_done_and_cancelled_with_no_output_or_error() {
        let event = cancelled_progress_event("job-1".to_string());
        assert!(event.done);
        assert!(event.cancelled);
        assert!(event.output_path.is_none());
        assert!(event.error.is_none());
    }

    #[test]
    fn outcome_progress_event_reports_a_real_success_output_path_and_duration() {
        let output = crate::voice::provider::VoiceSynthesisOutput {
            audio_path: "/tmp/out.wav".to_string(),
            duration_us: Some(1_500_000),
        };
        let event = outcome_progress_event("job-1".to_string(), Ok(output));
        assert!(event.done);
        assert!(!event.cancelled);
        assert_eq!(event.output_path.as_deref(), Some("/tmp/out.wav"));
        assert_eq!(event.duration_us, Some(1_500_000));
        assert!(event.error.is_none());
    }

    #[test]
    fn outcome_progress_event_reports_a_real_error_message_with_no_output_path() {
        let err = VoiceError::NotImplemented {
            provider: "NTSGenAI".to_string(),
            reason: "no adapter".to_string(),
        };
        let event = outcome_progress_event("job-1".to_string(), Err(err));
        assert!(event.done);
        assert!(!event.cancelled);
        assert!(event.output_path.is_none());
        assert!(event.error.unwrap().contains("not implemented"));
    }
}
