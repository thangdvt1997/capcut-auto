//! OpenAI-compatible `/v1/chat/completions` adapter (master prompt §17).
//!
//! Serves **three** of master prompt §17's five listed adapters — OpenAI
//! itself, Ollama, and a "custom OpenAI-compatible endpoint" — with one
//! struct, not three near-duplicate ones: all three speak the identical
//! wire protocol (`POST {base_url}/chat/completions` with an OpenAI-shaped
//! `{model, messages, temperature}` request body and `{choices: [{message:
//! {content}}]}` response body), differing only in *which* `base_url` is
//! configured and whether an API key is actually required (Ollama's local
//! server needs none; this adapter sends the `Authorization` header only
//! when `api_key` is `Some`, so a keyless local Ollama call still works).
//! `AiProviderKind::{OpenAi, Ollama, CustomOpenAiCompatible}` in
//! `commands::ai` all construct this same `OpenAiCompatProvider`.
//!
//! Request/response shapes below match OpenAI's own documented Chat
//! Completions API (`POST /v1/chat/completions`) as of this writing.

use serde::Deserialize;
use serde_json::json;

use super::error::AiProviderError;
use super::provider::{AIProvider, AiRequest, AiResponse};

const PROVIDER_NAME: &str = "openai-compatible";
const DEFAULT_MAX_TOKENS: u32 = 1024;

pub struct OpenAiCompatProvider {
    /// e.g. `https://api.openai.com/v1`, `http://localhost:11434/v1`
    /// (Ollama's own OpenAI-compatibility endpoint), or any custom
    /// self-hosted OpenAI-compatible server. No trailing slash expected —
    /// `chat_completions_url` appends `/chat/completions` directly.
    pub base_url: String,
    /// `None` for a provider that needs no auth (e.g. a local Ollama
    /// instance with no auth configured).
    pub api_key: Option<String>,
    pub model: String,
}

impl OpenAiCompatProvider {
    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn request_body(&self, request: &AiRequest) -> serde_json::Value {
        let mut messages = Vec::new();
        if let Some(system) = &request.system_prompt {
            messages.push(json!({"role": "system", "content": system}));
        }
        messages.push(json!({"role": "user", "content": request.user_prompt}));

        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "temperature": request.temperature,
        });
        body["max_tokens"] = json!(request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS));
        body
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: String,
}

/// OpenAI's documented error envelope: `{"error": {"message": ..., "type":
/// ..., "param": ..., "code": ...}}`.
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

    let parsed: ChatCompletionResponse =
        serde_json::from_str(body).map_err(|e| AiProviderError::InvalidResponse {
            provider: PROVIDER_NAME.to_string(),
            details: e.to_string(),
        })?;
    let first =
        parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AiProviderError::InvalidResponse {
                provider: PROVIDER_NAME.to_string(),
                details: "response had an empty `choices` array".to_string(),
            })?;
    Ok(AiResponse {
        text: first.message.content,
    })
}

impl AIProvider for OpenAiCompatProvider {
    fn name(&self) -> &'static str {
        PROVIDER_NAME
    }

    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiProviderError> {
        let mut req = ureq::post(&self.chat_completions_url())
            .set("Content-Type", "application/json")
            .timeout(request.timeout());
        if let Some(key) = &self.api_key {
            req = req.set("Authorization", &format!("Bearer {key}"));
        }

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

    fn provider(base_url: String) -> OpenAiCompatProvider {
        OpenAiCompatProvider {
            base_url,
            api_key: Some("sk-test-key".to_string()),
            model: "gpt-4o-mini".to_string(),
        }
    }

    #[test]
    fn request_body_matches_the_documented_chat_completions_shape() {
        let provider = provider("http://example.invalid/v1".to_string());
        let body = provider.request_body(&test_request("Say OK."));
        assert_eq!(body["model"], "gpt-4o-mini");
        // Compare against the same f32->f64 widening `temperature` goes
        // through (see `anthropic`'s equivalent test comment).
        assert_eq!(body["temperature"], json!(0.2_f32));
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(
            body["messages"][0]["content"],
            "You are a helpful assistant."
        );
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "Say OK.");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn request_body_omits_the_system_message_when_absent() {
        let provider = provider("http://example.invalid/v1".to_string());
        let mut request = test_request("hi");
        request.system_prompt = None;
        let body = provider.request_body(&request);
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn chat_completions_url_appends_the_documented_path() {
        let no_trailing_slash = provider("https://api.openai.com/v1".to_string());
        assert_eq!(
            no_trailing_slash.chat_completions_url(),
            "https://api.openai.com/v1/chat/completions"
        );
        // Trailing slash on base_url must not produce a double slash.
        let with_trailing_slash = provider("https://api.openai.com/v1/".to_string());
        assert_eq!(
            with_trailing_slash.chat_completions_url(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn parses_a_real_documented_success_response() {
        let body = r#"{
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "created": 1699000000,
            "model": "gpt-4o-mini",
            "choices": [
                {"index": 0, "message": {"role": "assistant", "content": "OK."}, "finish_reason": "stop"}
            ],
            "usage": {"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12}
        }"#;
        let response = parse_response(200, body).unwrap();
        assert_eq!(response.text, "OK.");
    }

    #[test]
    fn parses_a_real_documented_error_response() {
        let body = r#"{"error": {"message": "Incorrect API key provided.", "type": "invalid_request_error", "param": null, "code": "invalid_api_key"}}"#;
        let err = parse_response(401, body).unwrap_err();
        assert!(matches!(
            err,
            AiProviderError::HttpError { status: 401, .. }
        ));
        assert!(err.to_string().contains("Incorrect API key"));
    }

    #[test]
    fn an_empty_choices_array_is_an_invalid_response() {
        let body = r#"{"id": "x", "object": "chat.completion", "choices": []}"#;
        assert!(matches!(
            parse_response(200, body).unwrap_err(),
            AiProviderError::InvalidResponse { .. }
        ));
    }

    #[test]
    fn real_http_round_trip_against_a_mock_server_succeeds() {
        let body = r#"{
            "id": "chatcmpl-abc123",
            "object": "chat.completion",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "Hello from the mock."}, "finish_reason": "stop"}]
        }"#
        .to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let provider = provider(base_url);

        let response = provider.complete(&test_request("Say hi.")).unwrap();
        assert_eq!(response.text, "Hello from the mock.");

        let captured = rx.recv().expect("server captured a request");
        assert_eq!(captured.method, "POST");
        assert_eq!(captured.path, "/chat/completions");
        assert_eq!(captured.header("authorization"), Some("Bearer sk-test-key"));
        let sent: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
        assert_eq!(sent["messages"][1]["content"], "Say hi.");
    }

    #[test]
    fn real_http_round_trip_reports_a_non_2xx_status_as_an_http_error() {
        let body = r#"{"error": {"message": "invalid api key", "type": "invalid_request_error"}}"#
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

    #[test]
    fn no_api_key_omits_the_authorization_header_for_a_local_ollama_style_call() {
        let body = r#"{"choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}]}"#.to_string();
        let (base_url, rx) = spawn_one_shot("HTTP/1.1 200 OK", body);
        let provider = OpenAiCompatProvider {
            base_url,
            api_key: None,
            model: "llama3".to_string(),
        };

        provider.complete(&test_request("hi")).unwrap();
        let captured = rx.recv().unwrap();
        assert_eq!(captured.header("authorization"), None);
    }
}
