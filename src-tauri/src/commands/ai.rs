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
use crate::ai::{anthropic, credentials, edit_plan, gemini, openai_compat, smart_edit};
use crate::error::AppErrorPayload;
use crate::project::{Cut, ProjectV1, TranscriptEntry};
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

/// `pub(crate)`, not `pub`: `commands::highlights::detect_highlights` (Phase
/// 10 follow-up, highlight detection's optional semantic-importance signal)
/// reuses this exact key-resolution logic rather than duplicating it — never
/// exposed outside this crate's own command layer.
pub(crate) fn resolve_api_key(
    settings: &AiProviderSettings,
) -> Result<Option<String>, AiProviderError> {
    match &settings.credential_ref {
        Some(credential_ref) => credentials::default_store().get(credential_ref).map(Some),
        None => Ok(None),
    }
}

/// `pub(crate)` for the same reason as [`resolve_api_key`] above —
/// `commands::highlights::detect_highlights` builds a provider through this
/// exact function rather than a second, parallel construction path.
pub(crate) fn build_provider(
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

// ---------------------------------------------------------------------------
// Natural-language AI command box (master prompt §20)
// ---------------------------------------------------------------------------

/// **Natural language → AI Provider → EditPlan → Schema validation**
/// (master prompt §20's pipeline, up through validation — Preview/Apply are
/// the existing [`build_cuts_from_edit_plan`]/[`apply_edit_plan_to_clip`]/
/// [`apply_edit_plan_to_track`] commands above, reused unchanged): builds a
/// real grounding prompt from `nl_command` + `transcript` + `total_duration_us`
/// (`ai::nl_command::build_edit_plan_prompt`), calls the configured
/// provider, and validates the response through the exact same
/// `edit_plan::parse_and_validate` [`validate_edit_plan`] already uses —
/// never a second, parallel validation path. Never a partially-populated
/// plan: any malformed/invalid response is a clear `AppErrorPayload`, not a
/// best-effort guess.
///
/// See `ai::nl_command` module doc comment for exactly which of master
/// prompt §20's own example commands this can express end-to-end today
/// (pure removal/timing edits) versus which need operation kinds this pass's
/// `EditPlan` schema doesn't have yet.
#[tauri::command]
#[specta::specta]
pub fn generate_edit_plan_from_nl_command(
    settings: AiProviderSettings,
    nl_command: String,
    transcript: Vec<TranscriptEntry>,
    total_duration_us: i64,
) -> Result<edit_plan::EditPlan, AppErrorPayload> {
    let api_key = resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider = build_provider(&settings, api_key).map_err(|e| AppErrorPayload::from(&e))?;
    let prompt =
        crate::ai::nl_command::build_edit_plan_prompt(&nl_command, &transcript, total_duration_us);
    let request = prompt.into_request(settings.temperature, settings.timeout_ms, Some(2048));
    let response = provider
        .complete(&request)
        .map_err(|e| AppErrorPayload::from(&e))?;
    edit_plan::parse_and_validate(&response.text).map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Smart Edit / AI semantic editing (master prompt §19)
// ---------------------------------------------------------------------------

/// **Analyze**: builds a Smart Edit prompt from `entries` (the caller-
/// supplied transcript — same "caller passes the transcript in directly"
/// convention `commands::transcription::detect_filler_words` already uses,
/// rather than this command reaching into project state itself), calls the
/// configured provider, and validates the response into a strict
/// `Vec<SmartEditRecommendation>` — or a clear error, never a partially
/// populated result (`ai::smart_edit` module doc comment).
///
/// This is a *proposal* the frontend shows the user for review (a later
/// pass) — nothing here mutates the timeline. See
/// [`apply_smart_edit_recommendations_to_clip`]/
/// [`apply_smart_edit_recommendations_to_track`] for the separate, explicit
/// apply step.
#[tauri::command]
#[specta::specta]
pub fn analyze_smart_edit(
    settings: AiProviderSettings,
    entries: Vec<TranscriptEntry>,
) -> Result<Vec<smart_edit::SmartEditRecommendation>, AppErrorPayload> {
    let api_key = resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider = build_provider(&settings, api_key).map_err(|e| AppErrorPayload::from(&e))?;
    let request =
        smart_edit::build_smart_edit_request(&entries, settings.temperature, settings.timeout_ms);
    let response = provider
        .complete(&request)
        .map_err(|e| AppErrorPayload::from(&e))?;
    smart_edit::parse_and_validate(&response.text).map_err(|e| AppErrorPayload::from(&e))
}

/// Pure conversion of a caller-selected (and possibly action-overridden)
/// subset of recommendations into unapplied `Cut`s scoped to
/// `source_media_id` — the same "propose the cutlist, don't apply it yet"
/// step [`build_cuts_from_edit_plan`] already exposes for `EditPlan`-derived
/// cuts.
#[tauri::command]
#[specta::specta]
pub fn build_cuts_from_smart_edit_recommendations(
    source_media_id: String,
    recommendations: Vec<smart_edit::SmartEditRecommendation>,
) -> Vec<Cut> {
    smart_edit::recommendations_to_cuts(&recommendations, &source_media_id)
}

/// **Apply** (scoped to one clip): converts `recommendations`' `Remove`/
/// `Shorten` actions into `Cut`s against `source_media_id` and applies them
/// to `clip_id` through the exact same `commands::timeline::apply_silence_cuts`
/// path VAD/filler-word/`EditPlan` cuts already use — one atomic undo step,
/// never a second mutation path. `Keep`/`Highlight` recommendations in
/// `recommendations` simply produce no `Cut` (module doc comment).
#[tauri::command]
#[specta::specta]
pub fn apply_smart_edit_recommendations_to_clip(
    state: State<'_, TimelineState>,
    clip_id: String,
    source_media_id: String,
    recommendations: Vec<smart_edit::SmartEditRecommendation>,
) -> Result<ProjectV1, AppErrorPayload> {
    let cuts = smart_edit::recommendations_to_cuts(&recommendations, &source_media_id);
    crate::commands::timeline::apply_silence_cuts(state, clip_id, cuts)
}

/// Same as [`apply_smart_edit_recommendations_to_clip`], but for every clip
/// currently on `track_id` (delegates to
/// `commands::timeline::apply_silence_cuts_to_track`).
#[tauri::command]
#[specta::specta]
pub fn apply_smart_edit_recommendations_to_track(
    state: State<'_, TimelineState>,
    track_id: String,
    source_media_id: String,
    recommendations: Vec<smart_edit::SmartEditRecommendation>,
) -> Result<ProjectV1, AppErrorPayload> {
    let cuts = smart_edit::recommendations_to_cuts(&recommendations, &source_media_id);
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

    // -- Smart Edit (master prompt §19) --------------------------------------

    fn transcript_entry(id: &str, text: &str, start_us: i64, end_us: i64) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: "m1".to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
            words: Vec::new(),
            is_filler: false,
        }
    }

    /// Wraps `content` (whatever raw text the Smart Edit pipeline should
    /// receive as the provider's response) in a minimal OpenAI-compatible
    /// chat-completion body, the same shape
    /// `test_ai_connection_reports_success_against_a_working_mock_server`
    /// above already exercises for `OpenAiCompatProvider`.
    fn chat_completion_body(content: &str) -> String {
        serde_json::json!({
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }]
        })
        .to_string()
    }

    #[test]
    fn analyze_smart_edit_round_trips_a_well_formed_mock_response() {
        let smart_edit_json = r#"{"version":1,"recommendations":[{"id":"r1","start_us":0,"end_us":1000000,"transcript":"um so anyway","category":"filler_word","reason":"filler word detected","confidence":0.8,"suggested_action":{"type":"remove"}}]}"#;
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(smart_edit_json));

        let entries = vec![transcript_entry("e1", "um so anyway", 0, 1_000_000)];
        let recs = analyze_smart_edit(settings(AiProviderKind::OpenAi, base_url), entries)
            .expect("well-formed response should parse and validate");

        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "r1");
        assert_eq!(recs[0].transcript, "um so anyway");
        assert!(matches!(
            recs[0].category,
            smart_edit::SmartEditCategory::FillerWord
        ));
        assert!(matches!(
            recs[0].suggested_action,
            smart_edit::SmartEditAction::Remove
        ));
    }

    #[test]
    fn analyze_smart_edit_surfaces_a_clear_error_for_a_malformed_mock_response() {
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body("not json at all"));

        let entries = vec![transcript_entry("e1", "hello", 0, 1_000_000)];
        let err =
            analyze_smart_edit(settings(AiProviderKind::OpenAi, base_url), entries).unwrap_err();
        assert_eq!(err.code, "SMART_EDIT_MALFORMED_JSON");
    }

    #[test]
    fn analyze_smart_edit_surfaces_a_clear_error_for_an_invalid_recommendation() {
        // Well-formed JSON, but confidence is out of range — the provider
        // call itself succeeds; validation is what must reject this.
        let smart_edit_json = r#"{"version":1,"recommendations":[{"id":"r1","start_us":0,"end_us":100,"transcript":"x","category":"filler_word","reason":"x","confidence":5.0,"suggested_action":{"type":"keep"}}]}"#;
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(smart_edit_json));

        let entries = vec![transcript_entry("e1", "x", 0, 100)];
        let err =
            analyze_smart_edit(settings(AiProviderKind::OpenAi, base_url), entries).unwrap_err();
        assert_eq!(err.code, "SMART_EDIT_INVALID_RECOMMENDATION");
    }

    #[test]
    fn build_cuts_from_smart_edit_recommendations_command_delegates_to_the_pure_conversion() {
        let recs = vec![smart_edit::SmartEditRecommendation {
            id: "r1".to_string(),
            start_us: 0,
            end_us: 100,
            transcript: "x".to_string(),
            category: smart_edit::SmartEditCategory::Repetition,
            reason: "x".to_string(),
            confidence: 0.9,
            suggested_action: smart_edit::SmartEditAction::Remove,
        }];
        let cuts = build_cuts_from_smart_edit_recommendations("m1".to_string(), recs);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].source_media_id, "m1");
    }

    // -- Natural-language AI command box (master prompt §20) ----------------

    #[test]
    fn generate_edit_plan_from_nl_command_round_trips_a_well_formed_mock_response() {
        let plan_json = r#"{"version": 1, "operations": [
            {"type": "remove", "start_us": 0, "end_us": 2000000, "reason": "long pause", "confidence": 0.9}
        ]}"#;
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(plan_json));

        let transcript = vec![transcript_entry("e1", "um so anyway", 0, 2_000_000)];
        let plan = generate_edit_plan_from_nl_command(
            settings(AiProviderKind::OpenAi, base_url),
            "Remove filler words.".to_string(),
            transcript,
            10_000_000,
        )
        .expect("well-formed response should parse and validate");

        assert_eq!(plan.operations.len(), 1);
        assert!(matches!(
            plan.operations[0],
            edit_plan::EditOperation::Remove { .. }
        ));

        // The mock server actually received a real HTTP request carrying the
        // constructed prompt (this pass's "real HTTP-call-shape tested
        // against the mock server" requirement) — not a stubbed call.
        let captured = rx.recv().expect("mock server captured a request");
        assert_eq!(captured.method, "POST");
        assert!(captured.body.contains("Remove filler words."));
        assert!(captured.body.contains("um so anyway"));
    }

    #[test]
    fn generate_edit_plan_from_nl_command_surfaces_a_clear_error_for_a_malformed_mock_response() {
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body("not json at all"));

        let err = generate_edit_plan_from_nl_command(
            settings(AiProviderKind::OpenAi, base_url),
            "Remove filler words.".to_string(),
            vec![],
            10_000_000,
        )
        .unwrap_err();
        assert_eq!(err.code, "EDIT_PLAN_MALFORMED_JSON");
    }

    #[test]
    fn generate_edit_plan_from_nl_command_surfaces_a_clear_error_when_unreachable() {
        let dead_url = crate::ai::test_http::spawn_connection_refused();
        let err = generate_edit_plan_from_nl_command(
            settings(AiProviderKind::OpenAi, dead_url),
            "Remove filler words.".to_string(),
            vec![],
            10_000_000,
        )
        .unwrap_err();
        assert_eq!(err.code, "AI_PROVIDER_REQUEST_FAILED");
    }
}
