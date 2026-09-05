//! Anthropic `/v1/messages` adapter (master prompt §17).
//!
//! A genuinely different wire shape from the OpenAI-compatible adapter
//! (`ai::openai_compat` module doc comment) — separate `system` field
//! instead of a `system` role message, mandatory `max_tokens`, an
//! `x-api-key`/`anthropic-version` header pair instead of `Authorization:
//! Bearer`, and a `content: [{type, text}]` array response instead of
//! `choices[].message.content` — so this is its own adapter, not a
//! parameterization of `OpenAiCompatProvider`.
//!
//! Request/response shapes below match Anthropic's own documented Messages
//! API (`POST /v1/messages`) as of this writing.

use serde::Deserialize;
use serde_json::json;

use super::error::AiProviderError;
use super::provider::{AIProvider, AiRequest, AiResponse};

const PROVIDER_NAME: &str = "anthropic";
const DEFAULT_MAX_TOKENS: u32 = 1024;
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    /// e.g. `https://api.anthropic.com` (no trailing slash expected).
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl AnthropicProvider {
    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

    fn request_body(&self, request: &AiRequest) -> serde_json::Value {
        let mut body = json!({
            "model": self.model,
            "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "messages": [
                {"role": "user", "content": request.user_prompt}
            ],
            "temperature": request.temperature,
        });
        if let Some(system) = &request.system_prompt {
            body["system"] = json!(system);
        }
        body
    }
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

/// Anthropic's documented error envelope: `{"type": "error", "error":
/// {"type": ..., "message": ...}}`.
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

    let parsed: MessagesResponse =
        serde_json::from_str(body).map_err(|e| AiProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: e.to_string(),
        })?;
    let text = parsed
        .content
        .into_iter()
        .find(|block| block.kind == "text")
        .and_then(|block| block.text)
        .ok_or_else(|| AiProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: "response had no text content block".to_string(),
        })?;
    Ok(AiResponse { text })
}

impl AIProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiProviderError> {
        let req = ureq::post(&self.messages_url())
            .set("Content-Type", "application/json")
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
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

    fn provider(base_url: String) -> AnthropicProvider {
        AnthropicProvider {
            base_url,
            api_key: "sk-ant-test-key".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
        }
    }

    #[test]
    fn request_body_matches_the_documented_messages_shape() {
        let provider = provider("http://example.invalid".to_string());
        let body = provider.request_body(&test_request("Say OK."));
        assert_eq!(body["model"], "claude-3-5-sonnet-20241022");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["system"], "You are a helpful assistant.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Say OK.");
        // `temperature` is `f32`; serde_json widens it to `f64` exactly as
        // the bit pattern represents, not as the nearest decimal — compare
        // against the same widening rather than a literal `0.2` (`f64`).
        assert_eq!(body["temperature"], json!(0.2_f32));
    }

    #[test]
    fn request_body_omits_system_field_when_absent() {
        let provider = provider("http://example.invalid".to_string());
        let mut request = test_request("hi");
        request.system_prompt = None;
        let body = provider.request_body(&request);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn messages_url_appends_the_documented_path() {
        let provider = provider("https://api.anthropic.com".to_string());
        assert_eq!(
            provider.messages_url(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn parses_a_real_documented_success_response() {
        let body = r#"{
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-20241022",
            "content": [{"type": "text", "text": "OK."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 10, "output_tokens": 2}
        }"#;
        let response = parse_response(200, body).unwrap();
        assert_eq!(response.text, "OK.");
    }

    #[test]
    fn parses_a_real_documented_error_response() {
        let body = r#"{"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}"#;
        let err = parse_response(401, body).unwrap_err();
        assert!(matches!(
            err,
            AiProviderError::HttpError { status: 401, .. }
        ));
        assert!(err.to_string().contains("invalid x-api-key"));
    }

    #[test]
    fn an_empty_content_array_is_an_invalid_response() {
        let body = r#"{"id": "x", "type": "message", "role": "assistant", "model": "m", "content": [], "stop_reason": "end_turn"}"#;
        assert!(matches!(
            parse_response(200, body).unwrap_err(),
            AiProviderError::InvalidResponse { .. }
        ));
    }

    #[test]
    fn real_http_round_trip_against_a_mock_server_succeeds() {
        let body = r#"{
            "id": "msg_abc123",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-20241022",
            "content": [{"type": "text", "text": "Hello from the mock."}],
            "stop_reason": "end_turn"
        }"#
        .to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let provider = provider(base_url);

        let response = provider.complete(&test_request("Say hi.")).unwrap();
        assert_eq!(response.text, "Hello from the mock.");

        let captured = rx.recv().expect("server captured a request");
        assert_eq!(captured.method, "POST");
        assert_eq!(captured.path, "/v1/messages");
        assert_eq!(captured.header("x-api-key"), Some("sk-ant-test-key"));
        assert_eq!(
            captured.header("anthropic-version"),
            Some(ANTHROPIC_VERSION)
        );
        let sent: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
        assert_eq!(sent["messages"][0]["content"], "Say hi.");
    }

    #[test]
    fn real_http_round_trip_reports_a_non_2xx_status_as_an_http_error() {
        let body =
            r#"{"type": "error", "error": {"type": "authentication_error", "message": "bad key"}}"#
                .to_string();
        let (base_url, _rx) = spawn_one_shot("HTTP/1.1 401 Unauthorized", body);
        let provider = provider(base_url);

        let err = provider.complete(&test_request("hi")).unwrap_err();
        assert!(matches!(
            err,
            AiProviderError::HttpError { status: 401, .. }
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
