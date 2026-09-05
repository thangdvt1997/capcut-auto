//! `BatchError` — this subsystem's slice of the standardized error model
//! (master prompt §56), same `{code, message, details, recoverable,
//! suggested_action}` pattern as `RenderError`/`TemplateError`/`VadError`.
//! Every underlying stage error (`MediaError`/`VadError`/`TranscriptionError`/
//! `RenderError`/`TemplateError`/`TimelineError`) is folded into
//! `StageFailed { stage, details }` at the call site (its own `Display`
//! output preserved in `details`) rather than this enum growing a variant
//! per foreign error type — the stage name plus the original message is
//! enough to act on, and `AppErrorPayload::details` is exactly where that
//! kind of passthrough text belongs.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum BatchError {
    #[error("media file not found: {path}")]
    MediaNotFound { path: String },

    #[error("media file has neither video nor audio content: {path}")]
    UnsupportedMedia { path: String },

    #[error("captions were requested but no transcription_model_id was given")]
    TranscriptionModelRequired,

    #[error("transcription model {model_id} is not installed")]
    TranscriptionModelNotInstalled { model_id: String },

    #[error("unknown transcription model id: {model_id}")]
    UnknownTranscriptionModel { model_id: String },

    #[error("unknown template id: {template_id}")]
    UnknownTemplate { template_id: String },

    #[error("no export_preset_id was given, and no template was selected to fall back to")]
    ExportPresetRequired,

    #[error("job was cancelled")]
    Cancelled,

    #[error("{stage} failed: {details}")]
    StageFailed { stage: String, details: String },

    #[error("batch job {job_id} not found")]
    JobNotFound { job_id: String },

    #[error("batch {batch_id} not found")]
    BatchNotFound { batch_id: String },

    #[error("job {job_id} is not in a Failed state and cannot be retried")]
    NotRetryable { job_id: String },
}

impl From<&BatchError> for AppErrorPayload {
    fn from(err: &BatchError) -> Self {
        let message = err.to_string();
        match err {
            BatchError::MediaNotFound { path } => {
                AppErrorPayload::new("BATCH_MEDIA_NOT_FOUND", message)
                    .with_details(path.clone())
                    .recoverable(true)
                    .with_suggestion("Check the file path and try again.")
            }
            BatchError::UnsupportedMedia { path } => {
                AppErrorPayload::new("BATCH_UNSUPPORTED_MEDIA", message)
                    .with_details(path.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a media file with video or audio content.")
            }
            BatchError::TranscriptionModelRequired => {
                AppErrorPayload::new("BATCH_TRANSCRIPTION_MODEL_REQUIRED", message)
                    .recoverable(true)
                    .with_suggestion(
                        "Set transcription_model_id in the batch config, or disable captions.",
                    )
            }
            BatchError::TranscriptionModelNotInstalled { model_id } => {
                AppErrorPayload::new("BATCH_TRANSCRIPTION_MODEL_NOT_INSTALLED", message)
                    .with_details(model_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Download this model in the Model Manager before starting the batch.",
                    )
            }
            BatchError::UnknownTranscriptionModel { model_id } => {
                AppErrorPayload::new("BATCH_UNKNOWN_TRANSCRIPTION_MODEL", message)
                    .with_details(model_id.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a known transcription model id.")
            }
            BatchError::UnknownTemplate { template_id } => {
                AppErrorPayload::new("BATCH_UNKNOWN_TEMPLATE", message)
                    .with_details(template_id.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a known template id, or omit template_id.")
            }
            BatchError::ExportPresetRequired => {
                AppErrorPayload::new("BATCH_EXPORT_PRESET_REQUIRED", message)
                    .recoverable(true)
                    .with_suggestion(
                        "Set export_preset_id in the batch config, or select a template.",
                    )
            }
            BatchError::Cancelled => AppErrorPayload::new("BATCH_CANCELLED", message)
                .recoverable(true)
                .with_suggestion("Retry the job if this was unintended."),
            BatchError::StageFailed { stage, details } => {
                AppErrorPayload::new("BATCH_STAGE_FAILED", message)
                    .with_details(format!("{stage}: {details}"))
                    .recoverable(true)
                    .with_suggestion("Check the error details and retry the job.")
            }
            BatchError::JobNotFound { job_id } => {
                AppErrorPayload::new("BATCH_JOB_NOT_FOUND", message)
                    .with_details(job_id.clone())
                    .recoverable(true)
                    .with_suggestion("Check the job id; it may have already finished.")
            }
            BatchError::BatchNotFound { batch_id } => {
                AppErrorPayload::new("BATCH_NOT_FOUND", message)
                    .with_details(batch_id.clone())
                    .recoverable(true)
                    .with_suggestion("Check the batch id.")
            }
            BatchError::NotRetryable { job_id } => {
                AppErrorPayload::new("BATCH_JOB_NOT_RETRYABLE", message)
                    .with_details(job_id.clone())
                    .recoverable(true)
                    .with_suggestion("Only a Failed job can be retried.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(BatchError, &str)> = vec![
            (
                BatchError::MediaNotFound { path: "x".into() },
                "BATCH_MEDIA_NOT_FOUND",
            ),
            (
                BatchError::UnsupportedMedia { path: "x".into() },
                "BATCH_UNSUPPORTED_MEDIA",
            ),
            (
                BatchError::TranscriptionModelRequired,
                "BATCH_TRANSCRIPTION_MODEL_REQUIRED",
            ),
            (
                BatchError::TranscriptionModelNotInstalled {
                    model_id: "x".into(),
                },
                "BATCH_TRANSCRIPTION_MODEL_NOT_INSTALLED",
            ),
            (
                BatchError::UnknownTranscriptionModel {
                    model_id: "x".into(),
                },
                "BATCH_UNKNOWN_TRANSCRIPTION_MODEL",
            ),
            (
                BatchError::UnknownTemplate {
                    template_id: "x".into(),
                },
                "BATCH_UNKNOWN_TEMPLATE",
            ),
            (
                BatchError::ExportPresetRequired,
                "BATCH_EXPORT_PRESET_REQUIRED",
            ),
            (BatchError::Cancelled, "BATCH_CANCELLED"),
            (
                BatchError::StageFailed {
                    stage: "Rendering".into(),
                    details: "x".into(),
                },
                "BATCH_STAGE_FAILED",
            ),
            (
                BatchError::JobNotFound { job_id: "x".into() },
                "BATCH_JOB_NOT_FOUND",
            ),
            (
                BatchError::BatchNotFound {
                    batch_id: "x".into(),
                },
                "BATCH_NOT_FOUND",
            ),
            (
                BatchError::NotRetryable { job_id: "x".into() },
                "BATCH_JOB_NOT_RETRYABLE",
            ),
        ];
        for (err, expected_code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert!(!payload.message.is_empty());
        }
    }
}
