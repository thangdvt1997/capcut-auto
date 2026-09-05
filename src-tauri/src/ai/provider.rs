//! `AIProvider` trait (master prompt §17) — model/vendor-independent, same
//! design shape as `vad::provider::VadProvider`/`transcription::provider::
//! TranscriptionProvider`: no vendor-specific type leaks into the trait
//! signature, so a concrete adapter (`ai::openai_compat`, `ai::anthropic`,
//! `ai::gemini`, and any future one) can be swapped in without touching any
//! call site.
//!
//! This trait only ever talks to an LLM and hands back its raw text — it has
//! no notion of `EditPlan` (that parsing/validation step is `ai::edit_plan`,
//! a separate, later stage: master prompt §18's "AI → JSON Schema
//! validation → ... " pipeline is deliberately two independent modules, not
//! one, so a caller that just wants a chat completion for something other
//! than an EditPlan — a future "explain this cut" feature, say — can use
//! `AIProvider` without any EditPlan machinery involved).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::error::AiProviderError;

/// Provider-agnostic request. Deliberately minimal — exactly the fields
/// master prompt §17 calls out as configurable per call (system/user prompt
/// content, temperature, timeout) plus `max_tokens`, which several real
/// provider APIs (Anthropic's `/v1/messages` in particular) require as a
/// mandatory field; adapters that don't need it (OpenAI-compatible) simply
/// omit it from the wire request when `None`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AiRequest {
    /// System/instruction prompt. `None` omits it entirely rather than
    /// sending an empty string — some providers (Gemini) treat an empty
    /// system instruction differently from an absent one.
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub temperature: f32,
    /// Milliseconds — carried as a plain integer (not `std::time::Duration`,
    /// which specta cannot export) across the Tauri/specta boundary;
    /// converted to a real `Duration` only at the HTTP-client boundary
    /// inside each adapter.
    pub timeout_ms: u64,
    pub max_tokens: Option<u32>,
}

impl AiRequest {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

/// Provider-agnostic response: the raw text/JSON the provider returned.
/// Parsing that into a validated `EditPlan` is a separate, later step
/// (`ai::edit_plan::parse_and_validate`) — this type carries no opinion
/// about what the text contains.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AiResponse {
    pub text: String,
}

/// LLM backend. Deliberately minimal and free of any vendor-specific type
/// (module doc comment) — `complete` is the only method every adapter must
/// implement.
pub trait AIProvider: Send + Sync {
    /// Human-readable provider name, used only in error messages
    /// (`AiProviderError::RequestFailed { provider, .. }` etc.) — never
    /// parsed by callers.
    fn name(&self) -> &'static str;

    /// Sends `request` to the configured backend and returns its raw text
    /// response. Real, synchronous HTTP (this crate has no async runtime
    /// dependency elsewhere for HTTP — see `transcription::download` module
    /// doc comment) — callers running this from a Tauri command should do
    /// so on a background thread via `tauri::async_runtime::spawn_blocking`,
    /// the same pattern every other long-running call in this crate uses.
    fn complete(&self, request: &AiRequest) -> Result<AiResponse, AiProviderError>;
}
