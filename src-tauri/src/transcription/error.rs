//! `TranscriptionError`/`ModelError` — this subsystem's slice of the
//! standardized error model (master prompt §56, `docs/project-format.md`
//! "Error model"), following the same `{code, message, details,
//! recoverable, suggested_action}` pattern as `project::error::ProjectError`/
//! `media::error::MediaError`/`vad::error::VadError`.
//!
//! Two enums, not one, matching `docs/project-format.md`'s error list
//! (`TranscriptionError`, `ModelError` are listed separately) and this
//! module's own separation of concerns: `TranscriptionError` covers the
//! Whisper inference pipeline itself (`transcription::provider`/`whisper`);
//! `ModelError` covers the Model Manager (`transcription::models`/
//! `download`) — a model can fail to download for reasons that have nothing
//! to do with transcription ever running, and vice versa.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum TranscriptionError {
    #[error("no installed Whisper model for id {model_id}; download one first")]
    ModelNotInstalled { model_id: String },

    #[error("failed to load Whisper model at {path}: {details}")]
    ModelLoadFailed { path: String, details: String },

    #[error("unsupported input sample rate {found}Hz (expected {expected}Hz)")]
    UnsupportedSampleRate { found: u32, expected: u32 },

    #[error("no audio samples to transcribe")]
    EmptyAudio,

    #[error("Whisper inference failed: {details}")]
    InferenceFailed { details: String },

    #[error("transcription was cancelled")]
    Cancelled,

    #[error("no transcription job found for job_id {job_id}")]
    JobNotFound { job_id: String },
}

impl From<&TranscriptionError> for AppErrorPayload {
    fn from(err: &TranscriptionError) -> Self {
        let message = err.to_string();
        match err {
            TranscriptionError::ModelNotInstalled { model_id } => {
                AppErrorPayload::new("TRANSCRIPTION_MODEL_NOT_INSTALLED", message)
                    .with_details(model_id.clone())
                    .recoverable(true)
                    .with_suggestion("Open Model Manager and download this model first.")
            }
            TranscriptionError::ModelLoadFailed { path, details } => {
                AppErrorPayload::new("TRANSCRIPTION_MODEL_LOAD_FAILED", message)
                    .with_details(format!("path={path}: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "The model file may be corrupt or incompatible; delete and re-download it.",
                    )
            }
            TranscriptionError::UnsupportedSampleRate { found, expected } => {
                AppErrorPayload::new("TRANSCRIPTION_UNSUPPORTED_SAMPLE_RATE", message)
                    .with_details(format!("found={found} expected={expected}"))
                    .recoverable(false)
                    .with_suggestion(
                        "This is an internal error — audio should always be extracted at the expected rate.",
                    )
            }
            TranscriptionError::EmptyAudio => AppErrorPayload::new("TRANSCRIPTION_EMPTY_AUDIO", message)
                .recoverable(true)
                .with_suggestion("This media has no audio track to transcribe."),
            TranscriptionError::InferenceFailed { details } => {
                AppErrorPayload::new("TRANSCRIPTION_INFERENCE_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Retry; if it keeps failing, try a smaller model or check available memory.")
            }
            TranscriptionError::Cancelled => AppErrorPayload::new("TRANSCRIPTION_CANCELLED", message)
                .recoverable(true)
                .with_suggestion("Transcription was cancelled; re-run it if you still want a transcript."),
            TranscriptionError::JobNotFound { job_id } => {
                AppErrorPayload::new("TRANSCRIPTION_JOB_NOT_FOUND", message)
                    .with_details(job_id.clone())
                    .recoverable(true)
                    .with_suggestion("The job may have already finished, failed, or been cancelled.")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum ModelError {
    #[error("unknown model id: {model_id}")]
    UnknownModel { model_id: String },

    #[error("model {model_id} is not installed")]
    NotInstalled { model_id: String },

    #[error("could not resolve the model storage directory: {details}")]
    StorageUnavailable { details: String },

    #[error("download failed for model {model_id}: {details}")]
    DownloadFailed { model_id: String, details: String },

    #[error("download for model {model_id} was cancelled")]
    DownloadCancelled { model_id: String },

    #[error("downloaded file for model {model_id} failed verification: {details}")]
    VerificationFailed { model_id: String, details: String },

    #[error("filesystem error for model {model_id}: {details}")]
    IoFailed { model_id: String, details: String },

    #[error("no download job found for model {model_id}")]
    JobNotFound { model_id: String },
}

impl From<&ModelError> for AppErrorPayload {
    fn from(err: &ModelError) -> Self {
        let message = err.to_string();
        match err {
            ModelError::UnknownModel { model_id } => AppErrorPayload::new("MODEL_UNKNOWN", message)
                .with_details(model_id.clone())
                .recoverable(true)
                .with_suggestion("Choose one of the models listed in the catalog."),
            ModelError::NotInstalled { model_id } => {
                AppErrorPayload::new("MODEL_NOT_INSTALLED", message)
                    .with_details(model_id.clone())
                    .recoverable(true)
                    .with_suggestion("Download the model before using or deleting it.")
            }
            ModelError::StorageUnavailable { details } => {
                AppErrorPayload::new("MODEL_STORAGE_UNAVAILABLE", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion(
                        "Check the app's local data directory is writable, then retry.",
                    )
            }
            ModelError::DownloadFailed { model_id, details } => {
                AppErrorPayload::new("MODEL_DOWNLOAD_FAILED", message)
                    .with_details(format!("model={model_id}: {details}"))
                    .recoverable(true)
                    .with_suggestion("Check your network connection, then retry the download.")
            }
            ModelError::DownloadCancelled { model_id } => {
                AppErrorPayload::new("MODEL_DOWNLOAD_CANCELLED", message)
                    .with_details(model_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The partial download was kept and can be resumed by downloading again.",
                    )
            }
            ModelError::VerificationFailed { model_id, details } => {
                AppErrorPayload::new("MODEL_VERIFICATION_FAILED", message)
                    .with_details(format!("model={model_id}: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "The download did not match the expected size; retry the download.",
                    )
            }
            ModelError::IoFailed { model_id, details } => {
                AppErrorPayload::new("MODEL_IO_FAILED", message)
                    .with_details(format!("model={model_id}: {details}"))
                    .recoverable(true)
                    .with_suggestion("Check disk space and folder permissions, then retry.")
            }
            ModelError::JobNotFound { model_id } => {
                AppErrorPayload::new("MODEL_JOB_NOT_FOUND", message)
                    .with_details(model_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The download may have already finished, failed, or been cancelled.",
                    )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transcription_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(TranscriptionError, &str)> = vec![
            (
                TranscriptionError::ModelNotInstalled {
                    model_id: "tiny".into(),
                },
                "TRANSCRIPTION_MODEL_NOT_INSTALLED",
            ),
            (
                TranscriptionError::ModelLoadFailed {
                    path: "p".into(),
                    details: "d".into(),
                },
                "TRANSCRIPTION_MODEL_LOAD_FAILED",
            ),
            (
                TranscriptionError::UnsupportedSampleRate {
                    found: 8000,
                    expected: 16000,
                },
                "TRANSCRIPTION_UNSUPPORTED_SAMPLE_RATE",
            ),
            (TranscriptionError::EmptyAudio, "TRANSCRIPTION_EMPTY_AUDIO"),
            (
                TranscriptionError::InferenceFailed {
                    details: "d".into(),
                },
                "TRANSCRIPTION_INFERENCE_FAILED",
            ),
            (TranscriptionError::Cancelled, "TRANSCRIPTION_CANCELLED"),
            (
                TranscriptionError::JobNotFound {
                    job_id: "j1".into(),
                },
                "TRANSCRIPTION_JOB_NOT_FOUND",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
            assert!(payload.suggested_action.is_some());
        }
    }

    #[test]
    fn every_model_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(ModelError, &str)> = vec![
            (
                ModelError::UnknownModel {
                    model_id: "x".into(),
                },
                "MODEL_UNKNOWN",
            ),
            (
                ModelError::NotInstalled {
                    model_id: "tiny".into(),
                },
                "MODEL_NOT_INSTALLED",
            ),
            (
                ModelError::StorageUnavailable {
                    details: "d".into(),
                },
                "MODEL_STORAGE_UNAVAILABLE",
            ),
            (
                ModelError::DownloadFailed {
                    model_id: "tiny".into(),
                    details: "d".into(),
                },
                "MODEL_DOWNLOAD_FAILED",
            ),
            (
                ModelError::DownloadCancelled {
                    model_id: "tiny".into(),
                },
                "MODEL_DOWNLOAD_CANCELLED",
            ),
            (
                ModelError::VerificationFailed {
                    model_id: "tiny".into(),
                    details: "d".into(),
                },
                "MODEL_VERIFICATION_FAILED",
            ),
            (
                ModelError::IoFailed {
                    model_id: "tiny".into(),
                    details: "d".into(),
                },
                "MODEL_IO_FAILED",
            ),
            (
                ModelError::JobNotFound {
                    model_id: "tiny".into(),
                },
                "MODEL_JOB_NOT_FOUND",
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
