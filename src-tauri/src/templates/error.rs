//! `TemplateError` — the Templates subsystem's slice of the standardized
//! `{code, message, details, recoverable, suggested_action}` error model
//! (master prompt §56), same shape/pattern as `transcription::ModelError`/
//! `project::ProjectError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum TemplateError {
    #[error("unknown template id: {template_id}")]
    UnknownTemplate { template_id: String },

    #[error("unknown caption style id: {style_id}")]
    UnknownCaptionStyle { style_id: String },

    #[error("unknown export preset id: {preset_id}")]
    UnknownExportPreset { preset_id: String },

    #[error("could not resolve the templates storage directory: {details}")]
    StorageUnavailable { details: String },

    #[error("templates filesystem error: {details}")]
    IoFailed { details: String },

    #[error("template file is not valid JSON: {details}")]
    CorruptJson { details: String },

    #[error("template {template_id} is a built-in template and cannot be deleted")]
    CannotDeleteBuiltIn { template_id: String },
}

impl From<&TemplateError> for AppErrorPayload {
    fn from(err: &TemplateError) -> Self {
        let message = err.to_string();
        match err {
            TemplateError::UnknownTemplate { template_id } => {
                AppErrorPayload::new("TEMPLATE_UNKNOWN", message)
                    .with_details(template_id.clone())
                    .recoverable(true)
                    .with_suggestion("Check the template id and try again.")
            }
            TemplateError::UnknownCaptionStyle { style_id } => AppErrorPayload::new(
                "TEMPLATE_UNKNOWN_CAPTION_STYLE",
                message,
            )
            .with_details(style_id.clone())
            .recoverable(true)
            .with_suggestion(
                "Choose a caption style id that exists in this project or the built-in catalog.",
            ),
            TemplateError::UnknownExportPreset { preset_id } => {
                AppErrorPayload::new("TEMPLATE_UNKNOWN_EXPORT_PRESET", message)
                    .with_details(preset_id.clone())
                    .recoverable(true)
                    .with_suggestion("Choose one of render::presets::all_presets()'s ids.")
            }
            TemplateError::StorageUnavailable { details } => {
                AppErrorPayload::new("TEMPLATE_STORAGE_UNAVAILABLE", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion("Restart the app; if this persists, check disk permissions.")
            }
            TemplateError::IoFailed { details } => {
                AppErrorPayload::new("TEMPLATE_IO_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Check disk space and folder permissions, then retry.")
            }
            TemplateError::CorruptJson { details } => {
                AppErrorPayload::new("TEMPLATE_CORRUPT_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a different template file to import.")
            }
            TemplateError::CannotDeleteBuiltIn { template_id } => {
                AppErrorPayload::new("TEMPLATE_CANNOT_DELETE_BUILT_IN", message)
                    .with_details(template_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Built-in templates cannot be deleted; delete a custom template instead.",
                    )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(TemplateError, &str)> = vec![
            (
                TemplateError::UnknownTemplate {
                    template_id: "x".into(),
                },
                "TEMPLATE_UNKNOWN",
            ),
            (
                TemplateError::UnknownCaptionStyle {
                    style_id: "x".into(),
                },
                "TEMPLATE_UNKNOWN_CAPTION_STYLE",
            ),
            (
                TemplateError::UnknownExportPreset {
                    preset_id: "x".into(),
                },
                "TEMPLATE_UNKNOWN_EXPORT_PRESET",
            ),
            (
                TemplateError::StorageUnavailable {
                    details: "x".into(),
                },
                "TEMPLATE_STORAGE_UNAVAILABLE",
            ),
            (
                TemplateError::IoFailed {
                    details: "x".into(),
                },
                "TEMPLATE_IO_FAILED",
            ),
            (
                TemplateError::CorruptJson {
                    details: "x".into(),
                },
                "TEMPLATE_CORRUPT_JSON",
            ),
            (
                TemplateError::CannotDeleteBuiltIn {
                    template_id: "x".into(),
                },
                "TEMPLATE_CANNOT_DELETE_BUILT_IN",
            ),
        ];
        for (err, expected_code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert!(!payload.message.is_empty());
        }
    }
}
