//! Google Gemini `generateContent` adapter (master prompt §17).
//!
//! Another genuinely different wire shape from both other adapters: the
//! model name is part of the URL path (not the request body), the API key
//! is a `key` query parameter (not a header at all), prompts are
//! `contents: [{role, parts: [{text}]}]` with a separate top-level
//! `systemInstruction`, and generation parameters (temperature, max output
//! tokens) live nested under `generationConfig`.
//!
//! Request/response shapes below match Google's own documented Gemini API
//! (`POST /v1beta/models/{model}:generateContent`) as of this writing.

use serde::Deserialize;
use serde_json::json;

use super::error::AiProviderError;
use super::provider::{AIProvider, AiRequest, AiResponse};

const PROVIDER_NAME: &str = "gemini";
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1024;

pub struct GeminiProvider {
    /// e.g. `https://generativelanguage.googleapis.com` (no trailing slash
    /// expected).
    pub base_url: String,
    pub api_key: String,
    /// e.g. `gemini-1.5-flash`, `gemini-1.5-pro`.
    pub model: String,
}

impl GeminiProvider {
    fn generate_content_url(&self) -> String {
        format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            self.model,
            self.api_key
        )
    }

    fn request_body(&self, request: &AiRequest) -> serde_json::Value {
        let mut body = json!({
            "contents": [
                {"role": "user", "parts": [{"text": request.user_prompt}]}
            ],
            "generationConfig": {
                "temperature": request.temperature,
                "maxOutputTokens": request.max_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            }
        });
        if let Some(system) = &request.system_prompt {
            body["systemInstruction"] = json!({"parts": [{"text": system}]});
        }
        body
    }
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: CandidateContent,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
struct Part {
    text: Option<String>,
}

/// Gemini's documented error envelope: `{"error": {"code": ..., "message":
/// ..., "status": ...}}`.
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: String,
}

fn parse_response(status: u16, body: &str) -> Result<AiResponse, AiProviderError> {
    if !(200..300).contains(&status) {
        let message = serde_json::from_str::<ErrorEnvelope>(body)
            .map(|e| e.error.message)
            .unwrap_or_else(|_| body.to_string());
        return Err(AiProviderError::HttpError {
            provider: PROVIDER_NAME.to_string(),
            status,
            body: message,
        });
    }

    let parsed: GenerateContentResponse =
        serde_json::from_str(body).map_err(|e| AiProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: e.to_string(),
        })?;
    let text = parsed
        .candidates
        .into_iter()
        .next()
        .and_then(|c| c.content.parts.into_iter().find_map(|p| p.text))
        .ok_or_else(|| AiProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: "response had no candidates with text parts".to_string(),
        })?;
    Ok(AiResponse { text })
}

impl AIProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiProviderError> {
        let req = ureq::post(&self.generate_content_url())
            .set("Content-Type", "application/json")
            .timeout(request.timeout());

        let body = self.request_body(request);
        match req.send_json(body) {
            Ok(response) => {
                let status = response.status();
                let text =
                    response
                        .into_string()
                        .map_err(|e| AiProviderError::InvalidResponse {
                            provider: PROVIDER_NAME.to_string(),
                            details: e.to_string(),
                        })?;
                parse_response(status, &text)
            }
            Err(ureq::Error::Status(status, response)) => {
                let text = response.into_string().unwrap_or_default();
                parse_response(status, &text)
            }
            Err(ureq::Error::Transport(t)) => Err(AiProviderError::RequestFailed {
                provider: PROVIDER_NAME.to_string(),
                details: t.to_string(),
            }),
        }
    }
}

#[cfg(test)]
fn test_request(user_prompt: &str) -> AiRequest {
    AiRequest {
        system_prompt: Some("You are a helpful assistant.".to_string()),
        user_prompt: user_prompt.to_string(),
        temperature: 0.2,
        timeout_ms: 5_000,
        max_tokens: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::{spawn_connection_refused, spawn_one_shot};

    fn provider(base_url: String) -> GeminiProvider {
        GeminiProvider {
            base_url,
            api_key: "AIzaTestKey".to_string(),
            model: "gemini-1.5-flash".to_string(),
        }
    }

    #[test]
    fn request_body_matches_the_documented_generate_content_shape() {
        let provider = provider("http://example.invalid".to_string());
        let body = provider.request_body(&test_request("Say OK."));
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "Say OK.");
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "You are a helpful assistant."
        );
        // Compare against the same f32->f64 widening `temperature` goes
        // through (see `anthropic`'s equivalent test comment).
        assert_eq!(body["generationConfig"]["temperature"], json!(0.2_f32));
        assert_eq!(
            body["generationConfig"]["maxOutputTokens"],
            DEFAULT_MAX_OUTPUT_TOKENS
        );
    }

    #[test]
    fn request_body_omits_system_instruction_when_absent() {
        let provider = provider("http://example.invalid".to_string());
        let mut request = test_request("hi");
        request.system_prompt = None;
        let body = provider.request_body(&request);
        assert!(body.get("systemInstruction").is_none());
    }

    #[test]
    fn generate_content_url_puts_model_in_the_path_and_key_in_the_query() {
        let provider = provider("https://generativelanguage.googleapis.com".to_string());
        assert_eq!(
            provider.generate_content_url(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent?key=AIzaTestKey"
        );
    }

    #[test]
    fn parses_a_real_documented_success_response() {
        let body = r#"{
            "candidates": [
                {
                    "content": {"parts": [{"text": "OK."}], "role": "model"},
                    "finishReason": "STOP",
                    "index": 0,
                    "safetyRatings": []
                }
            ],
            "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 2, "totalTokenCount": 12}
        }"#;
        let response = parse_response(200, body).unwrap();
        assert_eq!(response.text, "OK.");
    }

    #[test]
    fn parses_a_real_documented_error_response() {
        let body = r#"{"error": {"code": 400, "message": "API key not valid.", "status": "INVALID_ARGUMENT"}}"#;
        let err = parse_response(400, body).unwrap_err();
        assert!(matches!(
            err,
            AiProviderError::HttpError { status: 400, .. }
        ));
        assert!(err.to_string().contains("API key not valid"));
    }

    #[test]
    fn an_empty_candidates_array_is_an_invalid_response() {
        let body = r#"{"candidates": []}"#;
        assert!(matches!(
            parse_response(200, body).unwrap_err(),
            AiProviderError::InvalidResponse { .. }
        ));
    }

    #[test]
    fn real_http_round_trip_against_a_mock_server_succeeds() {
        let body = r#"{
            "candidates": [
                {"content": {"parts": [{"text": "Hello from the mock."}], "role": "model"}, "finishReason": "STOP", "index": 0}
            ]
        }"#
        .to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let provider = provider(base_url);

        let response = provider.complete(&test_request("Say hi.")).unwrap();
        assert_eq!(response.text, "Hello from the mock.");

        let captured = rx.recv().expect("server captured a request");
        assert_eq!(captured.method, "POST");
        assert!(captured
            .path
            .starts_with("/v1beta/models/gemini-1.5-flash:generateContent"));
        assert!(captured.path.contains("key=AIzaTestKey"));
        let sent: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
        assert_eq!(sent["contents"][0]["parts"][0]["text"], "Say hi.");
    }

    #[test]
    fn real_http_round_trip_reports_a_non_2xx_status_as_an_http_error() {
        let body = r#"{"error": {"code": 400, "message": "API key not valid.", "status": "INVALID_ARGUMENT"}}"#
            .to_string();
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 400 Bad Request", body);
        let provider = provider(base_url);

        let err = provider.complete(&test_request("hi")).unwrap_err();
        assert!(matches!(
            err,
            AiProviderError::HttpError { status: 400, .. }
        ));
    }

    #[test]
    fn an_unreachable_endpoint_reports_request_failed() {
        let dead_url = spawn_connection_refused();
        let provider = provider(dead_url);
        let err = provider.complete(&test_request("hi")).unwrap_err();
        assert!(matches!(err, AiProviderError::RequestFailed { .. }));
    }
}
