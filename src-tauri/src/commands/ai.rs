//! AI Tauri command surface (Phase 10, master prompt §17/§18). Thin per
//! master prompt §66 — all real logic lives in `crate::ai::{provider,
//! openai_compat, anthropic, gemini, credentials, edit_plan}`.
//!
//! Non-secret AI settings (provider/base URL/model/temperature/timeout)
//! deliberately have **no backend persistence command** here: per this
//! phase's brief, these follow the same "not genuinely per-project" call
//! Phase 7's Model Manager and Phase 9's CapCut settings already made
//! (localStorage/app-data, not `project.json`) — the frontend owns storing
//! `AiProviderSettings` and passes it into each call below. Only the secret
//! itself (the API key) has a backend command, and that command is
//! deliberately **write-only**: [`set_ai_api_key`] stores a key,
//! [`delete_ai_api_key`] removes one, and there is no `get_ai_api_key`
//! command anywhere — the only way a stored key is ever read back is
//! server-side, inside [`test_ai_connection`]/`build_provider`, to build an
//! outgoing request header. It is never returned to the frontend.
//!
//! Pipeline commands mirror the "propose, don't mutate; apply is a separate
//! explicit step" shape `commands::vad`/`commands::timeline`'s silence-cut
//! commands already use: [`validate_edit_plan`] turns raw AI output into a
//! validated `EditPlan` (or a clear error) without touching any project;
//! [`apply_edit_plan_to_clip`]/[`apply_edit_plan_to_track`] are the only
//! commands that actually mutate the timeline, and they do so by delegating
//! straight to the *existing* `commands::timeline::apply_silence_cuts`/
//! `apply_silence_cuts_to_track` (same `Cut`/`Command::Batch`/undo-history
//! machinery VAD and filler-word cuts already use) — never a second,
//! parallel mutation path.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use crate::ai::error::AiProviderError;
use crate::ai::provider::{AIProvider, AiRequest};
use crate::ai::{anthropic, credentials, edit_plan, gemini, openai_compat};
use crate::error::AppErrorPayload;
use crate::project::{Cut, ProjectV1};
use crate::timeline::session::TimelineState;

/// Which wire protocol a configured provider profile speaks. `OpenAi`,
/// `Ollama`, and `CustomOpenAiCompatible` all construct the same
/// `OpenAiCompatProvider` (`ai::openai_compat` module doc comment) — they
/// are still distinct variants here because the *frontend* still needs to
/// tell them apart for defaults/labeling/whether a key is normally required,
/// even though the backend adapter code doesn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    OpenAi,
    Ollama,
    CustomOpenAiCompatible,
    Anthropic,
    Gemini,
}

/// Master prompt §17's exact settings list (Provider/Base URL/Model/
/// Temperature/Timeout), minus API Key — the key itself never travels
/// through this struct; `credential_ref`, if present, names where
/// [`credentials::CredentialStore`] should look one up.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AiProviderSettings {
    pub provider: AiProviderKind,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub timeout_ms: u64,
    /// `None` for a provider that needs no auth (e.g. a local Ollama
    /// instance).
    pub credential_ref: Option<String>,
}

fn resolve_api_key(settings: &AiProviderSettings) -> Result<Option<String>, AiProviderError> {
    match &settings.credential_ref {
        Some(credential_ref) => credentials::default_store().get(credential_ref).map(Some),
        None => Ok(None),
    }
}

fn build_provider(
    settings: &AiProviderSettings,
    api_key: Option<String>,
) -> Result<Box<dyn AIProvider>, AiProviderError> {
    match settings.provider {
        AiProviderKind::OpenAi
        | AiProviderKind::Ollama
        | AiProviderKind::CustomOpenAiCompatible => {
            Ok(Box::new(openai_compat::OpenAiCompatProvider {
                base_url: settings.base_url.clone(),
                api_key,
                model: settings.model.clone(),
            }))
        }
        AiProviderKind::Anthropic => {
            let api_key = api_key.ok_or_else(|| AiProviderError::MissingApiKey {
                provider: "anthropic".to_string(),
            })?;
            Ok(Box::new(anthropic::AnthropicProvider {
                base_url: settings.base_url.clone(),
                api_key,
                model: settings.model.clone(),
            }))
        }
        AiProviderKind::Gemini => {
            let api_key = api_key.ok_or_else(|| AiProviderError::MissingApiKey {
                provider: "gemini".to_string(),
            })?;
            Ok(Box::new(gemini::GeminiProvider {
                base_url: settings.base_url.clone(),
                api_key,
                model: settings.model.clone(),
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// Credential storage (write-only from the frontend's perspective)
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn set_ai_api_key(credential_ref: String, api_key: String) -> Result<(), AppErrorPayload> {
    credentials::default_store()
        .set(&credential_ref, &api_key)
        .map_err(|e| AppErrorPayload::from(&e))
}

#[tauri::command]
#[specta::specta]
pub fn delete_ai_api_key(credential_ref: String) -> Result<(), AppErrorPayload> {
    credentials::default_store()
        .delete(&credential_ref)
        .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Connection test
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct AiConnectionTestResult {
    pub success: bool,
    /// Human-readable outcome — the provider's own trivial reply on
    /// success, or an error message on failure. Never a raw API key, on
    /// either path.
    pub message: String,
}

/// Calls the configured provider with a trivial request and reports
/// success/failure — never returns a stored API key to the frontend
/// (module doc comment). Folds every failure mode (missing key, unreachable
/// endpoint, bad credentials, malformed response) into
/// `AiConnectionTestResult { success: false, .. }` rather than an `Err`, so
/// the frontend can render a plain pass/fail indicator without a thrown
/// error.
#[tauri::command]
#[specta::specta]
pub fn test_ai_connection(settings: AiProviderSettings) -> AiConnectionTestResult {
    let api_key = match resolve_api_key(&settings) {
        Ok(key) => key,
        Err(e) => {
            return AiConnectionTestResult {
                success: false,
                message: e.to_string(),
            }
        }
    };
    let provider = match build_provider(&settings, api_key) {
        Ok(p) => p,
        Err(e) => {
            return AiConnectionTestResult {
                success: false,
                message: e.to_string(),
            }
        }
    };

    let request = AiRequest {
        system_prompt: None,
        user_prompt: "Reply with only the word OK.".to_string(),
        temperature: 0.0,
        timeout_ms: settings.timeout_ms,
        max_tokens: Some(16),
    };
    match provider.complete(&request) {
        Ok(response) => AiConnectionTestResult {
            success: true,
            message: format!(
                "Connected to {}. Reply: {}",
                provider.name(),
                response.text.trim()
            ),
        },
        Err(e) => AiConnectionTestResult {
            success: false,
            message: e.to_string(),
        },
    }
}

// ---------------------------------------------------------------------------
// EditPlan pipeline: AI output -> validation -> (preview, frontend) -> apply
// ---------------------------------------------------------------------------

/// **Validate** (master prompt §18's "JSON Schema validation" stage): parses
/// `raw` (whatever text an `AIProvider::complete` returned) into a strict
/// `EditPlan`, or a specific validation error — never a partially-populated
/// plan. The frontend's Edit Plan Preview (a later pass) is built on the
/// `EditPlan` this returns.
#[tauri::command]
#[specta::specta]
pub fn validate_edit_plan(raw: String) -> Result<edit_plan::EditPlan, AppErrorPayload> {
    edit_plan::parse_and_validate(&raw).map_err(|e| AppErrorPayload::from(&e))
}

/// Pure conversion of an already-validated plan's `Remove` operations into
/// unapplied `Cut`s scoped to `source_media_id` — the same
/// "propose the cutlist, don't apply it yet" step
/// `commands::vad::build_silence_cutlist` already exposes for VAD-derived
/// cuts. `Zoom` operations are dropped here (`edit_plan` module doc comment
/// — structural-only in this pass).
#[tauri::command]
#[specta::specta]
pub fn build_cuts_from_edit_plan(source_media_id: String, plan: edit_plan::EditPlan) -> Vec<Cut> {
    edit_plan::plan_to_remove_cuts(&plan, &source_media_id)
}

/// **Apply** (master prompt §18's "User Approves → Timeline Engine" stage,
/// scoped to one clip): converts `plan`'s `Remove` operations into `Cut`s
/// against `source_media_id` and applies them to `clip_id` through the
/// exact same split/trim/delete → `Command::Batch` → undo-history path
/// `commands::timeline::apply_silence_cuts` already provides for VAD/
/// filler-word cuts — one atomic undo step, never a second mutation path.
#[tauri::command]
#[specta::specta]
pub fn apply_edit_plan_to_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    source_media_id: String,
    plan: edit_plan::EditPlan,
) -> Result<ProjectV1, AppErrorPayload> {
    let cuts = edit_plan::plan_to_remove_cuts(&plan, &source_media_id);
    crate::commands::timeline::apply_silence_cuts(state, clip_id, cuts)
}

/// Same as [`apply_edit_plan_to_clip`], but for every clip currently on
/// `track_id` (delegates to `commands::timeline::apply_silence_cuts_to_track`).
#[tauri::command]
#[specta::specta]
pub fn apply_edit_plan_to_track(
    state: State<'_, TimelineState>,
    track_id: String,
    source_media_id: String,
    plan: edit_plan::EditPlan,
) -> Result<ProjectV1, AppErrorPayload> {
    let cuts = edit_plan::plan_to_remove_cuts(&plan, &source_media_id);
    crate::commands::timeline::apply_silence_cuts_to_track(state, track_id, cuts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::spawn_one_shot;

    fn settings(provider: AiProviderKind, base_url: String) -> AiProviderSettings {
        AiProviderSettings {
            provider,
            base_url,
            model: "test-model".to_string(),
            temperature: 0.2,
            timeout_ms: 5_000,
            credential_ref: None,
        }
    }

    #[test]
    fn build_provider_requires_a_credential_for_anthropic() {
        let s = settings(
            AiProviderKind::Anthropic,
            "https://api.anthropic.com".to_string(),
        );
        assert!(matches!(
            build_provider(&s, None),
            Err(AiProviderError::MissingApiKey { .. })
        ));
    }

    #[test]
    fn build_provider_requires_a_credential_for_gemini() {
        let s = settings(
            AiProviderKind::Gemini,
            "https://generativelanguage.googleapis.com".to_string(),
        );
        assert!(matches!(
            build_provider(&s, None),
            Err(AiProviderError::MissingApiKey { .. })
        ));
    }

    #[test]
    fn build_provider_allows_no_credential_for_a_local_ollama_style_endpoint() {
        let s = settings(
            AiProviderKind::Ollama,
            "http://localhost:11434/v1".to_string(),
        );
        assert!(build_provider(&s, None).is_ok());
    }

    #[test]
    fn test_ai_connection_reports_success_against_a_working_mock_server() {
        let body = r#"{"choices": [{"index": 0, "message": {"role": "assistant", "content": "OK"}, "finish_reason": "stop"}]}"#.to_string();
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let result = test_ai_connection(settings(AiProviderKind::OpenAi, base_url));
        assert!(result.success, "message: {}", result.message);
        assert!(result.message.contains("OK"));
    }

    #[test]
    fn test_ai_connection_reports_failure_against_an_unreachable_endpoint() {
        let dead_url = crate::ai::test_http::spawn_connection_refused();
        let result = test_ai_connection(settings(AiProviderKind::OpenAi, dead_url));
        assert!(!result.success);
    }

    #[test]
    fn test_ai_connection_reports_failure_when_a_required_credential_is_missing() {
        let result = test_ai_connection(settings(
            AiProviderKind::Anthropic,
            "https://api.anthropic.com".to_string(),
        ));
        assert!(!result.success);
        assert!(result.message.to_lowercase().contains("api key"));
    }

    #[test]
    fn validate_edit_plan_command_round_trips_a_valid_plan() {
        let raw = r#"{"version": 1, "operations": [
            {"type": "remove", "start_us": 0, "end_us": 100, "reason": "x", "confidence": null}
        ]}"#
        .to_string();
        let plan = validate_edit_plan(raw).unwrap();
        assert_eq!(plan.operations.len(), 1);
    }

    #[test]
    fn validate_edit_plan_command_surfaces_a_clear_error_for_malformed_input() {
        let err = validate_edit_plan("not json".to_string()).unwrap_err();
        assert_eq!(err.code, "EDIT_PLAN_MALFORMED_JSON");
    }
}
