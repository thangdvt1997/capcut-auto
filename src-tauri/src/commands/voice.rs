//! Voice/TTS Tauri command surface (`promt.md` §9). Thin per master prompt
//! §66 — all real logic lives in `crate::voice::{provider, custom_api,
//! stub}`, following the exact same shape `commands::ai` already established
//! for `crate::ai::*` (`resolve_api_key`/`build_provider`/a connection-test
//! command that folds every failure into a `{success, message}` result
//! rather than a thrown error).
//!
//! ## Deliberately deferred: no `synthesize_speech` command this pass
//!
//! `voice::provider` module doc comment already flags this: the real caller
//! for `VoiceProvider::synthesize` is a future `synthesize_speech` command
//! that resolves a real `output_path` under `app_local_data_dir().join(
//! "voice_output")` (the same pattern `commands::media`'s `media_cache_dir`/
//! `commands::assets::assets_dir` already use) and runs the call on a
//! background thread via `tauri::async_runtime::spawn_blocking`. That
//! plumbing — an `AppHandle`-aware path resolver, a background job/
//! cancellation story matching `commands::render`'s `RenderJobs`/
//! `commands::transcription`'s `TranscriptionJobs`, and a real "Voice
//! Mapping" (Speaker A/B/Narrator → voice) frontend surface to actually
//! drive it from (`promt.md` §9) — is a materially bigger, separate task
//! than "does this abstraction and its one real adapter work," which is this
//! pass's actual scope (`voice::provider` module doc comment). Building only
//! `test_voice_connection`/`list_voices` here keeps every command in this
//! file genuinely testable without a live TTS service, exactly the same
//! discipline `commands::ai::test_ai_connection` already set for the AI
//! layer: a connection/listing check is honestly verifiable against a mock
//! server; a real synthesis pipeline wired to nothing but mock data would
//! not be.
//!
//! ## Commands here
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

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ai::credentials;
use crate::error::AppErrorPayload;
use crate::voice::provider::{VoiceInfo, VoiceProvider};
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
}
