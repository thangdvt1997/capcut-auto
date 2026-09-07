//! `CustomApiVoiceProvider` — the "Custom API" entry in `promt.md` §9's
//! provider list: a user-hosted TTS server, not any single named third-party
//! vendor. Since there is no one real spec to match (unlike `ai::
//! openai_compat`, which matches OpenAI's own documented Chat Completions
//! API), this module *is* the contract: it documents exactly what request
//! this codebase sends and exactly what response shape it expects back, so a
//! user standing up a compatible server has something concrete to implement
//! against. Structurally real end-to-end (request-building, response-
//! parsing, error handling) — see `voice::provider` module doc comment's
//! "what is real / what is not" section for the honest caveat that no live
//! server implementing this contract has ever been run against it in this
//! environment; every test below talks to a real local mock HTTP server
//! (`ai::test_http`), never a live network call.
//!
//! ## The documented wire contract
//!
//! - **Synthesize**: `POST {base_url}/synthesize`, JSON body `{"text":
//!   <str>, "voice_id": <str>, "settings": <VoiceSynthesisSettings, exactly
//!   as that type's own `Serialize` impl produces>}`. A 2xx response body is
//!   JSON: `{"audio_base64": <str>, "duration_us": <int, optional>}` —
//!   `audio_base64` is standard (RFC 4648) base64-encoded raw audio bytes,
//!   decoded here and written verbatim to the caller's `output_path`
//!   (`voice::provider` module doc comment: this trait never invents a
//!   location, and never re-encodes/transcodes what a provider sends).
//!   `duration_us` is optional because not every real TTS server bothers
//!   reporting one; when absent, `VoiceSynthesisOutput::duration_us` is
//!   honestly `None` (never estimated from file size — this codebase has no
//!   audio-container parser to do that safely across arbitrary output
//!   formats a Custom API server might choose).
//! - **List voices**: `GET {base_url}/voices`, response JSON
//!   `{"voices": [{"voice_id": <str>, "name": <str>, "gender": <str,
//!   optional>, "language": <str, optional>, "preview_url": <str,
//!   optional>}]}`. `gender` is matched case-insensitively against
//!   `"male"`/`"female"`; any other non-empty value (e.g. a server's own
//!   `"narrator"` label) maps to [`VoiceGender::Other`] rather than being
//!   rejected — an unrecognized-but-present label is real information ("not
//!   strictly male/female"), not malformed input. A wholly absent `gender`
//!   field maps to `None` (this provider genuinely doesn't know).
//! - **Auth**: an `Authorization: Bearer <key>` header is sent only when
//!   `api_key` is `Some` — the same "keyless local server still works"
//!   discipline `ai::openai_compat::OpenAiCompatProvider` already
//!   established for a local Ollama instance, since `promt.md` §9 lists API
//!   Key as an optional field, not a mandatory one, for a self-hosted
//!   Custom API endpoint.
//!
//! Both endpoints are read from `base_url` with any trailing slash trimmed
//! first (`ai::openai_compat::OpenAiCompatProvider::chat_completions_url`'s
//! own precedent), so `"http://host:port"` and `"http://host:port/"` both
//! resolve to the same real URL.

use std::path::Path;

use base64::Engine;
use serde::Deserialize;
use serde_json::json;

use super::error::VoiceError;
use super::provider::{
    VoiceGender, VoiceInfo, VoiceProvider, VoiceSynthesisOutput, VoiceSynthesisSettings,
};

const PROVIDER_NAME: &str = "custom-api";

pub struct CustomApiVoiceProvider {
    /// e.g. `http://localhost:8080` or any self-hosted server implementing
    /// this module's documented contract. No trailing slash expected —
    /// [`Self::synthesize_url`]/[`Self::voices_url`] append their own path
    /// directly.
    pub base_url: String,
    /// `None` for a keyless local/self-hosted server (module doc comment).
    pub api_key: Option<String>,
}

impl CustomApiVoiceProvider {
    fn synthesize_url(&self) -> String {
        format!("{}/synthesize", self.base_url.trim_end_matches('/'))
    }

    fn voices_url(&self) -> String {
        format!("{}/voices", self.base_url.trim_end_matches('/'))
    }

    fn request_body(
        &self,
        text: &str,
        voice_id: &str,
        settings: &VoiceSynthesisSettings,
    ) -> serde_json::Value {
        json!({
            "text": text,
            "voice_id": voice_id,
            "settings": settings,
        })
    }

    fn auth_header(&self) -> Option<String> {
        self.api_key.as_ref().map(|key| format!("Bearer {key}"))
    }
}

#[derive(Debug, Deserialize)]
struct SynthesizeResponseWire {
    audio_base64: String,
    duration_us: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct VoicesResponseWire {
    voices: Vec<VoiceInfoWire>,
}

#[derive(Debug, Deserialize)]
struct VoiceInfoWire {
    voice_id: String,
    name: String,
    gender: Option<String>,
    language: Option<String>,
    preview_url: Option<String>,
}

fn map_gender(raw: Option<String>) -> Option<VoiceGender> {
    raw.map(|g| match g.to_ascii_lowercase().as_str() {
        "male" => VoiceGender::Male,
        "female" => VoiceGender::Female,
        _ => VoiceGender::Other,
    })
}

fn parse_synthesize(status: u16, body: &str) -> Result<SynthesizeResponseWire, VoiceError> {
    if !(200..300).contains(&status) {
        return Err(VoiceError::HttpError {
            provider: PROVIDER_NAME.to_string(),
            status,
            body: body.to_string(),
        });
    }
    serde_json::from_str(body).map_err(|e| VoiceError::InvalidResponse {
        provider: PROVIDER_NAME.to_string(),
        details: e.to_string(),
    })
}

fn parse_voices(status: u16, body: &str) -> Result<Vec<VoiceInfo>, VoiceError> {
    if !(200..300).contains(&status) {
        return Err(VoiceError::HttpError {
            provider: PROVIDER_NAME.to_string(),
            status,
            body: body.to_string(),
        });
    }
    let parsed: VoicesResponseWire =
        serde_json::from_str(body).map_err(|e| VoiceError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: e.to_string(),
        })?;
    Ok(parsed
        .voices
        .into_iter()
        .map(|v| VoiceInfo {
            voice_id: v.voice_id,
            name: v.name,
            gender: map_gender(v.gender),
            language: v.language,
            preview_url: v.preview_url,
        })
        .collect())
}

impl VoiceProvider for CustomApiVoiceProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn synthesize(
        &self,
        text: &str,
        voice_id: &str,
        settings: &VoiceSynthesisSettings,
        output_path: &Path,
    ) -> Result<VoiceSynthesisOutput, VoiceError> {
        let mut req = ureq::post(&self.synthesize_url()).set("Content-Type", "application/json");
        if let Some(auth) = self.auth_header() {
            req = req.set("Authorization", &auth);
        }

        let (status, body) = match req.send_json(self.request_body(text, voice_id, settings)) {
            Ok(response) => {
                let status = response.status();
                let text = response
                    .into_string()
                    .map_err(|e| VoiceError::InvalidResponse {
                        provider: PROVIDER_NAME.to_string(),
                        details: e.to_string(),
                    })?;
                (status, text)
            }
            Err(ureq::Error::Status(status, response)) => {
                (status, response.into_string().unwrap_or_default())
            }
            Err(ureq::Error::Transport(t)) => {
                return Err(VoiceError::RequestFailed {
                    provider: PROVIDER_NAME.to_string(),
                    details: t.to_string(),
                })
            }
        };

        let parsed = parse_synthesize(status, &body)?;
        let audio_bytes = base64::engine::general_purpose::STANDARD
            .decode(parsed.audio_base64.as_bytes())
            .map_err(|e| VoiceError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: format!("`audio_base64` field was not valid base64: {e}"),
            })?;
        std::fs::write(output_path, &audio_bytes).map_err(|e| VoiceError::OutputWriteFailed {
            path: output_path.to_string_lossy().to_string(),
            details: e.to_string(),
        })?;

        Ok(VoiceSynthesisOutput {
            audio_path: output_path.to_string_lossy().to_string(),
            duration_us: parsed.duration_us,
        })
    }

    fn list_voices(&self) -> Result<Vec<VoiceInfo>, VoiceError> {
        let mut req = ureq::get(&self.voices_url());
        if let Some(auth) = self.auth_header() {
            req = req.set("Authorization", &auth);
        }

        let (status, body) = match req.call() {
            Ok(response) => {
                let status = response.status();
                let text = response
                    .into_string()
                    .map_err(|e| VoiceError::InvalidResponse {
                        provider: PROVIDER_NAME.to_string(),
                        details: e.to_string(),
                    })?;
                (status, text)
            }
            Err(ureq::Error::Status(status, response)) => {
                (status, response.into_string().unwrap_or_default())
            }
            Err(ureq::Error::Transport(t)) => {
                return Err(VoiceError::RequestFailed {
                    provider: PROVIDER_NAME.to_string(),
                    details: t.to_string(),
                })
            }
        };

        parse_voices(status, &body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::{spawn_connection_refused, spawn_one_shot};

    fn provider(base_url: String) -> CustomApiVoiceProvider {
        CustomApiVoiceProvider {
            base_url,
            api_key: Some("sk-test-key".to_string()),
        }
    }

    fn test_settings() -> VoiceSynthesisSettings {
        VoiceSynthesisSettings {
            speed: 1.2,
            pitch: 0.9,
            volume: 1.0,
            emotion: Some("happy".to_string()),
            language: Some("en".to_string()),
        }
    }

    fn temp_output_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ave-voice-custom-api-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    // -- request_body / URL building --------------------------------------

    #[test]
    fn request_body_matches_the_documented_synthesize_shape() {
        let p = provider("http://example.invalid".to_string());
        let body = p.request_body("Hello there.", "voice-1", &test_settings());
        assert_eq!(body["text"], "Hello there.");
        assert_eq!(body["voice_id"], "voice-1");
        assert_eq!(body["settings"]["speed"], json!(1.2_f32));
        assert_eq!(body["settings"]["pitch"], json!(0.9_f32));
        assert_eq!(body["settings"]["volume"], json!(1.0_f32));
        assert_eq!(body["settings"]["emotion"], "happy");
        assert_eq!(body["settings"]["language"], "en");
    }

    #[test]
    fn request_body_sends_null_for_unset_optional_settings() {
        let p = provider("http://example.invalid".to_string());
        let mut settings = test_settings();
        settings.emotion = None;
        settings.language = None;
        let body = p.request_body("hi", "voice-1", &settings);
        assert!(body["settings"]["emotion"].is_null());
        assert!(body["settings"]["language"].is_null());
    }

    #[test]
    fn synthesize_url_and_voices_url_append_the_documented_paths() {
        let no_trailing_slash = provider("http://localhost:8080".to_string());
        assert_eq!(
            no_trailing_slash.synthesize_url(),
            "http://localhost:8080/synthesize"
        );
        assert_eq!(
            no_trailing_slash.voices_url(),
            "http://localhost:8080/voices"
        );
        // Trailing slash on base_url must not produce a double slash.
        let with_trailing_slash = provider("http://localhost:8080/".to_string());
        assert_eq!(
            with_trailing_slash.synthesize_url(),
            "http://localhost:8080/synthesize"
        );
        assert_eq!(
            with_trailing_slash.voices_url(),
            "http://localhost:8080/voices"
        );
    }

    // -- response parsing (no HTTP involved) -------------------------------

    #[test]
    fn parse_voices_round_trips_a_well_formed_mock_response() {
        let body = r#"{"voices": [
            {"voice_id": "v1", "name": "Voice One", "gender": "female", "language": "en", "preview_url": "http://x/v1.mp3"},
            {"voice_id": "v2", "name": "Voice Two", "gender": "Male", "language": null, "preview_url": null},
            {"voice_id": "v3", "name": "Narrator", "gender": "narrator", "language": null, "preview_url": null},
            {"voice_id": "v4", "name": "No Gender Given", "gender": null, "language": null, "preview_url": null}
        ]}"#;
        let voices = parse_voices(200, body).unwrap();
        assert_eq!(voices.len(), 4);
        assert_eq!(voices[0].voice_id, "v1");
        assert_eq!(voices[0].gender, Some(VoiceGender::Female));
        assert_eq!(voices[1].gender, Some(VoiceGender::Male));
        // An unrecognized-but-present label ("narrator") is real information,
        // not malformed input (module doc comment) — maps to `Other`, not a
        // parse failure.
        assert_eq!(voices[2].gender, Some(VoiceGender::Other));
        // A wholly absent field is honestly `None` (module doc comment).
        assert_eq!(voices[3].gender, None);
    }

    #[test]
    fn parse_voices_rejects_malformed_json() {
        let err = parse_voices(200, "not json at all").unwrap_err();
        assert!(matches!(err, VoiceError::InvalidResponse { .. }));
    }

    #[test]
    fn parse_voices_reports_a_non_2xx_status_as_an_http_error() {
        let err = parse_voices(500, "internal error").unwrap_err();
        assert!(matches!(err, VoiceError::HttpError { status: 500, .. }));
    }

    #[test]
    fn parse_synthesize_rejects_malformed_json() {
        let err = parse_synthesize(200, "not json").unwrap_err();
        assert!(matches!(err, VoiceError::InvalidResponse { .. }));
    }

    #[test]
    fn parse_synthesize_reports_a_non_2xx_status_as_an_http_error() {
        let err = parse_synthesize(401, "unauthorized").unwrap_err();
        assert!(matches!(err, VoiceError::HttpError { status: 401, .. }));
    }

    // -- real HTTP round trips against a local mock server -----------------

    #[test]
    fn real_http_round_trip_synthesize_writes_decoded_audio_bytes_to_output_path() {
        let raw_audio = b"FAKE-WAV-BYTES-FOR-TEST-1234";
        let encoded = base64::engine::general_purpose::STANDARD.encode(raw_audio);
        let body = json!({"audio_base64": encoded, "duration_us": 2_500_000_i64}).to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let p = provider(base_url);
        let output_path = temp_output_path("synth-ok");

        let result = p
            .synthesize("Hello.", "voice-1", &test_settings(), &output_path)
            .expect("mock server responds with a well-formed synth response");

        assert_eq!(result.audio_path, output_path.to_string_lossy());
        assert_eq!(result.duration_us, Some(2_500_000));
        let written = std::fs::read(&output_path).expect("output file was written");
        assert_eq!(written, raw_audio);
        let _ = std::fs::remove_file(&output_path);

        let captured = rx.recv().expect("mock server captured a request");
        assert_eq!(captured.method, "POST");
        assert_eq!(captured.path, "/synthesize");
        assert_eq!(captured.header("authorization"), Some("Bearer sk-test-key"));
        let sent: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
        assert_eq!(sent["text"], "Hello.");
        assert_eq!(sent["voice_id"], "voice-1");
    }

    #[test]
    fn real_http_round_trip_synthesize_reports_a_non_2xx_status_as_an_http_error() {
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 500 Internal Server Error", "oops".to_string());
        let p = provider(base_url);
        let output_path = temp_output_path("synth-500");

        let err = p
            .synthesize("hi", "voice-1", &test_settings(), &output_path)
            .unwrap_err();
        assert!(matches!(err, VoiceError::HttpError { status: 500, .. }));
        assert!(!output_path.exists());
    }

    #[test]
    fn real_http_round_trip_synthesize_rejects_invalid_base64_as_an_invalid_response() {
        let body = json!({"audio_base64": "not-valid-base64!!", "duration_us": null}).to_string();
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let p = provider(base_url);
        let output_path = temp_output_path("synth-badb64");

        let err = p
            .synthesize("hi", "voice-1", &test_settings(), &output_path)
            .unwrap_err();
        assert!(matches!(err, VoiceError::InvalidResponse { .. }));
        assert!(!output_path.exists());
    }

    #[test]
    fn an_unreachable_synthesize_endpoint_reports_request_failed() {
        let dead_url = spawn_connection_refused();
        let p = provider(dead_url);
        let output_path = temp_output_path("synth-unreachable");
        let err = p
            .synthesize("hi", "voice-1", &test_settings(), &output_path)
            .unwrap_err();
        assert!(matches!(err, VoiceError::RequestFailed { .. }));
    }

    #[test]
    fn real_http_round_trip_list_voices_succeeds_and_captures_the_request() {
        let body = r#"{"voices": [{"voice_id": "v1", "name": "Voice One", "gender": "female", "language": "en", "preview_url": null}]}"#.to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let p = provider(base_url);

        let voices = p.list_voices().expect("mock server responds");
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].voice_id, "v1");
        assert_eq!(voices[0].gender, Some(VoiceGender::Female));

        let captured = rx.recv().expect("mock server captured a request");
        assert_eq!(captured.method, "GET");
        assert_eq!(captured.path, "/voices");
        assert_eq!(captured.header("authorization"), Some("Bearer sk-test-key"));
    }

    #[test]
    fn real_http_round_trip_list_voices_reports_a_non_2xx_status_as_an_http_error() {
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 401 Unauthorized", "nope".to_string());
        let p = provider(base_url);
        let err = p.list_voices().unwrap_err();
        assert!(matches!(err, VoiceError::HttpError { status: 401, .. }));
    }

    #[test]
    fn an_unreachable_voices_endpoint_reports_request_failed() {
        let dead_url = spawn_connection_refused();
        let p = provider(dead_url);
        let err = p.list_voices().unwrap_err();
        assert!(matches!(err, VoiceError::RequestFailed { .. }));
    }

    #[test]
    fn no_api_key_omits_the_authorization_header_for_a_keyless_local_server() {
        let body = r#"{"voices": []}"#.to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let p = CustomApiVoiceProvider {
            base_url,
            api_key: None,
        };
        p.list_voices().unwrap();
        let captured = rx.recv().unwrap();
        assert_eq!(captured.header("authorization"), None);
    }
}
