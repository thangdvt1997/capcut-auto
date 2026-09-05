//! `AiProviderError`/`EditPlanError` — this subsystem's slice of the
//! standardized error model (master prompt §56, `docs/project-format.md`
//! "Error model"), following the same `{code, message, details,
//! recoverable, suggested_action}` pattern as `transcription::error`'s own
//! two-enums-per-subsystem split.
//!
//! Two enums, not one, for the same reason `transcription::error` keeps
//! `TranscriptionError`/`ModelError` separate: `AiProviderError` covers
//! talking to an LLM backend (network, auth, malformed HTTP response) —
//! nothing here knows what an `EditPlan` is. `EditPlanError` covers the
//! completely separate concern of validating whatever text a provider
//! handed back against the strict `EditPlan` schema (`ai::edit_plan`) — a
//! provider call can succeed perfectly and still return text that fails
//! `EditPlanError` validation (the model hallucinated invalid JSON), and a
//! validation failure has nothing to do with whether the network call
//! itself worked.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum AiProviderError {
    #[error("{provider} request failed: {details}")]
    RequestFailed { provider: String, details: String },

    #[error("{provider} returned HTTP {status}: {body}")]
    HttpError {
        provider: String,
        status: u16,
        body: String,
    },

    #[error("could not parse {provider} response: {details}")]
    InvalidResponse { provider: String, details: String },

    #[error("{provider} requires an API key but none is configured")]
    MissingApiKey { provider: String },

    #[error("no stored credential for ref {credential_ref}")]
    CredentialNotFound { credential_ref: String },

    #[error("failed to access secure credential storage: {details}")]
    CredentialStoreFailed { details: String },
}

impl From<&AiProviderError> for AppErrorPayload {
    fn from(err: &AiProviderError) -> Self {
        let message = err.to_string();
        match err {
            AiProviderError::RequestFailed { details, .. } => {
                AppErrorPayload::new("AI_PROVIDER_REQUEST_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Check the provider's base URL and your network connection, then retry.",
                    )
            }
            AiProviderError::HttpError { status, body, .. } => {
                AppErrorPayload::new("AI_PROVIDER_HTTP_ERROR", message)
                    .with_details(format!("status={status}: {body}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Check the API key, model name, and account quota/billing, then retry.",
                    )
            }
            AiProviderError::InvalidResponse { details, .. } => {
                AppErrorPayload::new("AI_PROVIDER_INVALID_RESPONSE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The provider returned an unexpected response shape; verify the base URL points at a compatible API.",
                    )
            }
            AiProviderError::MissingApiKey { .. } => {
                AppErrorPayload::new("AI_PROVIDER_MISSING_API_KEY", message)
                    .recoverable(true)
                    .with_suggestion("Enter and save an API key for this provider in AI Settings.")
            }
            AiProviderError::CredentialNotFound { credential_ref } => {
                AppErrorPayload::new("AI_CREDENTIAL_NOT_FOUND", message)
                    .with_details(credential_ref.clone())
                    .recoverable(true)
                    .with_suggestion("Save an API key for this provider before using it.")
            }
            AiProviderError::CredentialStoreFailed { details } => {
                AppErrorPayload::new("AI_CREDENTIAL_STORE_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion(
                        "Windows Credential Manager could not be accessed; check Windows account/policy restrictions.",
                    )
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum EditPlanError {
    #[error("could not parse AI output as JSON: {details}")]
    MalformedJson { details: String },

    #[error("unsupported EditPlan schema version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("operation {index} is invalid: {details}")]
    InvalidOperation { index: usize, details: String },
}

impl From<&EditPlanError> for AppErrorPayload {
    fn from(err: &EditPlanError) -> Self {
        let message = err.to_string();
        match err {
            EditPlanError::MalformedJson { details } => {
                AppErrorPayload::new("EDIT_PLAN_MALFORMED_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The AI response was not valid EditPlan JSON; ask it to try again.",
                    )
            }
            EditPlanError::UnsupportedVersion { version } => {
                AppErrorPayload::new("EDIT_PLAN_UNSUPPORTED_VERSION", message)
                    .with_details(version.to_string())
                    .recoverable(false)
                    .with_suggestion("This app only understands EditPlan version 1.")
            }
            EditPlanError::InvalidOperation { index, details } => {
                AppErrorPayload::new("EDIT_PLAN_INVALID_OPERATION", message)
                    .with_details(format!("operation[{index}]: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject this EditPlan and ask the AI to produce a corrected one.",
                    )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ai_provider_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(AiProviderError, &str)> = vec![
            (
                AiProviderError::RequestFailed {
                    provider: "openai".into(),
                    details: "d".into(),
                },
                "AI_PROVIDER_REQUEST_FAILED",
            ),
            (
                AiProviderError::HttpError {
                    provider: "openai".into(),
                    status: 401,
                    body: "b".into(),
                },
                "AI_PROVIDER_HTTP_ERROR",
            ),
            (
                AiProviderError::InvalidResponse {
                    provider: "openai".into(),
                    details: "d".into(),
                },
                "AI_PROVIDER_INVALID_RESPONSE",
            ),
            (
                AiProviderError::MissingApiKey {
                    provider: "openai".into(),
                },
                "AI_PROVIDER_MISSING_API_KEY",
            ),
            (
                AiProviderError::CredentialNotFound {
                    credential_ref: "r1".into(),
                },
                "AI_CREDENTIAL_NOT_FOUND",
            ),
            (
                AiProviderError::CredentialStoreFailed {
                    details: "d".into(),
                },
                "AI_CREDENTIAL_STORE_FAILED",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn every_edit_plan_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(EditPlanError, &str)> = vec![
            (
                EditPlanError::MalformedJson {
                    details: "d".into(),
                },
                "EDIT_PLAN_MALFORMED_JSON",
            ),
            (
                EditPlanError::UnsupportedVersion { version: 2 },
                "EDIT_PLAN_UNSUPPORTED_VERSION",
            ),
            (
                EditPlanError::InvalidOperation {
                    index: 0,
                    details: "d".into(),
                },
                "EDIT_PLAN_INVALID_OPERATION",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
