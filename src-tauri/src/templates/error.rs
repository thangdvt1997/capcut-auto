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

    /// Path traversal prevention (master prompt §53): a custom template's
    /// `id` is joined directly onto this app's own `templates/` directory
    /// (`io::template_file_path`) to build its on-disk filename. Every
    /// legitimately-*created* template gets a fresh `custom_<uuid>` id
    /// (`templates::save_as_template_from_project`), but an *imported*
    /// template's `id` comes straight from whatever JSON file the user
    /// chose to import (`commands::templates::import_template`) — a
    /// crafted/corrupted file could carry `"id": "../../../../whatever"`,
    /// so this is rejected before it ever reaches a filesystem write/delete.
    /// See `crate::fs_safety::is_safe_path_component`.
    #[error("template id {template_id} is not a safe path component")]
    UnsafeTemplateId { template_id: String },

    /// Upgrade spec §17's "template reference asset bằng ID thay vì
    /// hard-code path" requirement: `Template::intro`/`outro`/`watermark`/
    /// `background_music` each carry an asset id, validated at save/update
    /// time against the caller-supplied set of ids that actually exist in
    /// the Asset Library (`assets::io::list_assets`) — never silently
    /// accepted as an opaque string, same discipline as
    /// `UnknownCaptionStyle`/`UnknownExportPreset` above.
    #[error("unknown asset id: {asset_id}")]
    UnknownAsset { asset_id: String },

    /// Upgrade spec §20 versioning: a built-in template (`is_built_in:
    /// true`) is never editable in place — same "immutable built-in" rule
    /// `CannotDeleteBuiltIn` already enforces for deletion, mirrored here
    /// for the new update/versioning path
    /// (`commands::templates::update_custom_template`).
    #[error("template {template_id} is a built-in template and cannot be edited")]
    CannotEditBuiltIn { template_id: String },

    /// Upgrade spec §20: `job` records pin `template_id` + `template_version`
    /// so it can resolve the exact content it was run with even after the
    /// template is edited further (`commands::templates::get_template_version`).
    /// This is the "no such version exists" case — either the version
    /// number was never real, or (for a custom template) its history was
    /// never recorded that far back.
    #[error("template {template_id} has no version {version}")]
    UnknownTemplateVersion { template_id: String, version: u32 },
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
            TemplateError::UnsafeTemplateId { template_id } => {
                AppErrorPayload::new("TEMPLATE_UNSAFE_TEMPLATE_ID", message)
                    .with_details(template_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "This template's id is invalid; re-export/re-create it and try again.",
                    )
            }
            TemplateError::UnknownAsset { asset_id } => {
                AppErrorPayload::new("TEMPLATE_UNKNOWN_ASSET", message)
                    .with_details(asset_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Add this asset to the Asset Library first, or choose a different one.",
                    )
            }
            TemplateError::CannotEditBuiltIn { template_id } => {
                AppErrorPayload::new("TEMPLATE_CANNOT_EDIT_BUILT_IN", message)
                    .with_details(template_id.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Built-in templates cannot be edited; save your changes as a new custom template instead.",
                    )
            }
            TemplateError::UnknownTemplateVersion {
                template_id,
                version,
            } => AppErrorPayload::new("TEMPLATE_UNKNOWN_VERSION", message)
                .with_details(format!("{template_id} v{version}"))
                .recoverable(true)
                .with_suggestion("Check the version number; it may have been pruned or never existed."),
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
            (
                TemplateError::UnsafeTemplateId {
                    template_id: "../../etc/passwd".into(),
                },
                "TEMPLATE_UNSAFE_TEMPLATE_ID",
            ),
            (
                TemplateError::UnknownAsset {
                    asset_id: "x".into(),
                },
                "TEMPLATE_UNKNOWN_ASSET",
            ),
            (
                TemplateError::CannotEditBuiltIn {
                    template_id: "x".into(),
                },
                "TEMPLATE_CANNOT_EDIT_BUILT_IN",
            ),
            (
                TemplateError::UnknownTemplateVersion {
                    template_id: "x".into(),
                    version: 3,
                },
                "TEMPLATE_UNKNOWN_VERSION",
            ),
        ];
        for (err, expected_code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert!(!payload.message.is_empty());
        }
    }
}
