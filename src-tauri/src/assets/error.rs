//! `AssetError` — the Asset Library subsystem's slice of the standardized
//! `{code, message, details, recoverable, suggested_action}` error model
//! (master prompt §56), same shape/pattern as `templates::error::TemplateError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum AssetError {
    #[error("unknown asset id: {asset_id}")]
    UnknownAsset { asset_id: String },

    /// Upgrade spec §17: `add_asset`/`update_asset` validate that
    /// `file_path` really points at an existing file before ever
    /// registering/updating the reference — a silently-broken reference
    /// would only surface much later, at template-apply/export time.
    #[error("no file exists at path: {file_path}")]
    FileNotFound { file_path: String },

    #[error("could not resolve the assets storage directory: {details}")]
    StorageUnavailable { details: String },

    #[error("assets filesystem error: {details}")]
    IoFailed { details: String },

    #[error("asset file is not valid JSON: {details}")]
    CorruptJson { details: String },

    /// Path traversal prevention (master prompt §53), same rationale as
    /// `TemplateError::UnsafeTemplateId` — every legitimately-created asset
    /// gets a fresh `asset_<uuid>` id, but this is validated defensively
    /// anyway before ever being joined onto the `assets/` storage
    /// directory. See `crate::fs_safety::is_safe_path_component`.
    #[error("asset id {asset_id} is not a safe path component")]
    UnsafeAssetId { asset_id: String },
}

impl From<&AssetError> for AppErrorPayload {
    fn from(err: &AssetError) -> Self {
        let message = err.to_string();
        match err {
            AssetError::UnknownAsset { asset_id } => AppErrorPayload::new("ASSET_UNKNOWN", message)
                .with_details(asset_id.clone())
                .recoverable(true)
                .with_suggestion("Check the asset id and try again."),
            AssetError::FileNotFound { file_path } => {
                AppErrorPayload::new("ASSET_FILE_NOT_FOUND", message)
                    .with_details(file_path.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a file that exists on disk and try again.")
            }
            AssetError::StorageUnavailable { details } => {
                AppErrorPayload::new("ASSET_STORAGE_UNAVAILABLE", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion("Restart the app; if this persists, check disk permissions.")
            }
            AssetError::IoFailed { details } => AppErrorPayload::new("ASSET_IO_FAILED", message)
                .with_details(details.clone())
                .recoverable(true)
                .with_suggestion("Check disk space and folder permissions, then retry."),
            AssetError::CorruptJson { details } => {
                AppErrorPayload::new("ASSET_CORRUPT_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "This asset's stored metadata is corrupt; remove and re-add it.",
                    )
            }
            AssetError::UnsafeAssetId { asset_id } => {
                AppErrorPayload::new("ASSET_UNSAFE_ASSET_ID", message)
                    .with_details(asset_id.clone())
                    .recoverable(true)
                    .with_suggestion("This asset's id is invalid; re-add it and try again.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(AssetError, &str)> = vec![
            (
                AssetError::UnknownAsset {
                    asset_id: "x".into(),
                },
                "ASSET_UNKNOWN",
            ),
            (
                AssetError::FileNotFound {
                    file_path: "x".into(),
                },
                "ASSET_FILE_NOT_FOUND",
            ),
            (
                AssetError::StorageUnavailable {
                    details: "x".into(),
                },
                "ASSET_STORAGE_UNAVAILABLE",
            ),
            (
                AssetError::IoFailed {
                    details: "x".into(),
                },
                "ASSET_IO_FAILED",
            ),
            (
                AssetError::CorruptJson {
                    details: "x".into(),
                },
                "ASSET_CORRUPT_JSON",
            ),
            (
                AssetError::UnsafeAssetId {
                    asset_id: "../../etc/passwd".into(),
                },
                "ASSET_UNSAFE_ASSET_ID",
            ),
        ];
        for (err, expected_code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert!(!payload.message.is_empty());
        }
    }
}
