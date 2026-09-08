//! `VoiceError` — this subsystem's slice of the standardized error model
//! (master prompt §56, `docs/project-format.md` "Error model"), following
//! the same `{code, message, details, recoverable, suggested_action}`
//! pattern as `ai::error::AiProviderError`/`transcription::error::
//! TranscriptionError`. Deliberately close to `AiProviderError`'s own shape
//! (`RequestFailed`/`HttpError`/`InvalidResponse`/`MissingApiKey`/
//! `CredentialNotFound`/`CredentialStoreFailed`) since a `VoiceProvider`
//! talking to a configured HTTP endpoint through a credential looked up in
//! the same `ai::credentials::CredentialStore` is structurally the same
//! kind of problem `AIProvider` already solves — plus four voice-specific
//! cases this subsystem alone needs: `OutputWriteFailed` (writing the
//! synthesized audio to disk failed), `NotImplemented` (the honestly-
//! stubbed `NtsGenAi`/`GptSoVits` provider kinds — see `voice::stub` module
//! doc comment), `StorageUnavailable` (resolving the `voice_output`
//! directory itself failed — `commands::voice::voice_output_dir`'s only
//! failure mode, matching `transcription::ModelError::StorageUnavailable`'s
//! own precedent), and `JobNotFound` (`cancel_voice_job` against an
//! already-finished or never-existed job id, matching
//! `RenderError::JobNotFound`/`TranscriptionError::JobNotFound`).

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum VoiceError {
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

    #[error("failed to write synthesized audio to {path}: {details}")]
    OutputWriteFailed { path: String, details: String },

    #[error("voice provider {provider} is not implemented: {reason}")]
    NotImplemented { provider: String, reason: String },

    #[error("could not access voice output storage: {details}")]
    StorageUnavailable { details: String },

    #[error("no voice synthesis job found for id {job_id}")]
    JobNotFound { job_id: String },
}

impl From<&VoiceError> for AppErrorPayload {
    fn from(err: &VoiceError) -> Self {
        let message = err.to_string();
        match err {
            VoiceError::RequestFailed { details, .. } => {
                AppErrorPayload::new("VOICE_PROVIDER_REQUEST_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Check the voice provider's server/API URL and your network connection, then retry.",
                    )
            }
            VoiceError::HttpError { status, body, .. } => {
                AppErrorPayload::new("VOICE_PROVIDER_HTTP_ERROR", message)
                    .with_details(format!("status={status}: {body}"))
                    .recoverable(true)
                    .with_suggestion("Check the API key and server URL, then retry.")
            }
            VoiceError::InvalidResponse { details, .. } => {
                AppErrorPayload::new("VOICE_PROVIDER_INVALID_RESPONSE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The voice provider returned an unexpected response shape; verify the server URL points at a compatible endpoint.",
                    )
            }
            VoiceError::MissingApiKey { .. } => {
                AppErrorPayload::new("VOICE_PROVIDER_MISSING_API_KEY", message)
                    .recoverable(true)
                    .with_suggestion(
                        "Enter and save an API key for this voice provider in Voice Settings.",
                    )
            }
            VoiceError::CredentialNotFound { credential_ref } => {
                AppErrorPayload::new("VOICE_CREDENTIAL_NOT_FOUND", message)
                    .with_details(credential_ref.clone())
                    .recoverable(true)
                    .with_suggestion("Save an API key for this voice provider before using it.")
            }
            VoiceError::CredentialStoreFailed { details } => {
                AppErrorPayload::new("VOICE_CREDENTIAL_STORE_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion(
                        "Windows Credential Manager could not be accessed; check Windows account/policy restrictions.",
                    )
            }
            VoiceError::OutputWriteFailed { path, details } => {
                AppErrorPayload::new("VOICE_OUTPUT_WRITE_FAILED", message)
                    .with_details(format!("path={path}: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Check disk space and folder permissions for the voice output directory, then retry.",
                    )
            }
            VoiceError::NotImplemented { reason, .. } => {
                AppErrorPayload::new("VOICE_PROVIDER_NOT_IMPLEMENTED", message)
                    .with_details(reason.clone())
                    .recoverable(false)
                    .with_suggestion("Choose Custom API, or wait for this provider to be implemented.")
            }
            VoiceError::StorageUnavailable { details } => {
                AppErrorPayload::new("VOICE_STORAGE_UNAVAILABLE", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion(
                        "This app could not resolve its local data directory for voice output; check disk/OS permissions.",
                    )
            }
            VoiceError::JobNotFound { job_id } => {
                AppErrorPayload::new("VOICE_JOB_NOT_FOUND", message)
                    .with_details(job_id.clone())
                    .recoverable(false)
                    .with_suggestion("This voice synthesis job has already finished or never existed.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_voice_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(VoiceError, &str)> = vec![
            (
                VoiceError::RequestFailed {
                    provider: "custom-api".into(),
                    details: "d".into(),
                },
                "VOICE_PROVIDER_REQUEST_FAILED",
            ),
            (
                VoiceError::HttpError {
                    provider: "custom-api".into(),
                    status: 500,
                    body: "b".into(),
                },
                "VOICE_PROVIDER_HTTP_ERROR",
            ),
            (
                VoiceError::InvalidResponse {
                    provider: "custom-api".into(),
                    details: "d".into(),
                },
                "VOICE_PROVIDER_INVALID_RESPONSE",
            ),
            (
                VoiceError::MissingApiKey {
                    provider: "custom-api".into(),
                },
                "VOICE_PROVIDER_MISSING_API_KEY",
            ),
            (
                VoiceError::CredentialNotFound {
                    credential_ref: "r1".into(),
                },
                "VOICE_CREDENTIAL_NOT_FOUND",
            ),
            (
                VoiceError::CredentialStoreFailed {
                    details: "d".into(),
                },
                "VOICE_CREDENTIAL_STORE_FAILED",
            ),
            (
                VoiceError::OutputWriteFailed {
                    path: "p".into(),
                    details: "d".into(),
                },
                "VOICE_OUTPUT_WRITE_FAILED",
            ),
            (
                VoiceError::NotImplemented {
                    provider: "NTSGenAI".into(),
                    reason: "r".into(),
                },
                "VOICE_PROVIDER_NOT_IMPLEMENTED",
            ),
            (
                VoiceError::StorageUnavailable {
                    details: "d".into(),
                },
                "VOICE_STORAGE_UNAVAILABLE",
            ),
            (
                VoiceError::JobNotFound {
                    job_id: "j1".into(),
                },
                "VOICE_JOB_NOT_FOUND",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
            assert!(payload.suggested_action.is_some());
        }
    }
}
