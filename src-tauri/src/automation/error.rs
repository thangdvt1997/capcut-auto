//! `AutomationError` — the Smart Automation subsystem's slice of the
//! standardized `{code, message, details, recoverable, suggested_action}`
//! error model (master prompt §56), same shape/pattern as
//! `AssetError`/`TemplateError`/`HistoryError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum AutomationError {
    #[error("unknown automation rule id: {rule_id}")]
    UnknownRule { rule_id: String },

    /// `automation::new_rule`/`update_automation_rule` validate that a
    /// `WatchFolder` trigger's `path` really exists as a directory before
    /// ever registering/updating the rule — a silently-broken watch path
    /// would otherwise only surface much later, as a rule that simply never
    /// fires, with no obvious reason why. Mirrors
    /// `AssetError::FileNotFound`'s exact rationale.
    #[error("watch-folder path does not exist or is not a directory: {path}")]
    FolderNotFound { path: String },

    /// Starting the real `notify` filesystem watch itself failed (e.g. a
    /// permissions error, or a platform-specific watch-limit) even though
    /// the path exists — distinct from `FolderNotFound` so a caller/UI can
    /// tell "bad input" apart from "a real OS-level watch failure".
    #[error("could not start watching {path}: {details}")]
    WatchFailed { path: String, details: String },

    #[error("could not resolve the automation rules storage directory: {details}")]
    StorageUnavailable { details: String },

    #[error("automation rules filesystem error: {details}")]
    IoFailed { details: String },

    #[error("automation rule file is not valid JSON: {details}")]
    CorruptJson { details: String },

    /// Path traversal prevention (master prompt §53), same rationale as
    /// `AssetError::UnsafeAssetId`/`TemplateError::UnsafeTemplateId` — every
    /// legitimately-created rule gets a fresh `rule_<uuid>` id, but this is
    /// validated defensively anyway before ever being joined onto the
    /// `automation_rules/` storage directory.
    #[error("automation rule id {rule_id} is not a safe path component")]
    UnsafeRuleId { rule_id: String },
}

impl From<&AutomationError> for AppErrorPayload {
    fn from(err: &AutomationError) -> Self {
        let message = err.to_string();
        match err {
            AutomationError::UnknownRule { rule_id } => {
                AppErrorPayload::new("AUTOMATION_UNKNOWN_RULE", message)
                    .with_details(rule_id.clone())
                    .recoverable(true)
                    .with_suggestion("Check the automation rule id and try again.")
            }
            AutomationError::FolderNotFound { path } => {
                AppErrorPayload::new("AUTOMATION_FOLDER_NOT_FOUND", message)
                    .with_details(path.clone())
                    .recoverable(true)
                    .with_suggestion("Choose a folder that exists on disk and try again.")
            }
            AutomationError::WatchFailed { path, details } => {
                AppErrorPayload::new("AUTOMATION_WATCH_FAILED", message)
                    .with_details(format!("{path}: {details}"))
                    .recoverable(true)
                    .with_suggestion("Check folder permissions and try enabling this rule again.")
            }
            AutomationError::StorageUnavailable { details } => {
                AppErrorPayload::new("AUTOMATION_STORAGE_UNAVAILABLE", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion("Restart the app; if this persists, check disk permissions.")
            }
            AutomationError::IoFailed { details } => {
                AppErrorPayload::new("AUTOMATION_IO_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Check disk space and folder permissions, then retry.")
            }
            AutomationError::CorruptJson { details } => {
                AppErrorPayload::new("AUTOMATION_CORRUPT_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "This automation rule's stored data is corrupt; remove and re-add it.",
                    )
            }
            AutomationError::UnsafeRuleId { rule_id } => {
                AppErrorPayload::new("AUTOMATION_UNSAFE_RULE_ID", message)
                    .with_details(rule_id.clone())
                    .recoverable(true)
                    .with_suggestion("This rule's id is invalid; re-create it and try again.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(AutomationError, &str)> = vec![
            (
                AutomationError::UnknownRule {
                    rule_id: "x".into(),
                },
                "AUTOMATION_UNKNOWN_RULE",
            ),
            (
                AutomationError::FolderNotFound { path: "x".into() },
                "AUTOMATION_FOLDER_NOT_FOUND",
            ),
            (
                AutomationError::WatchFailed {
                    path: "x".into(),
                    details: "y".into(),
                },
                "AUTOMATION_WATCH_FAILED",
            ),
            (
                AutomationError::StorageUnavailable {
                    details: "x".into(),
                },
                "AUTOMATION_STORAGE_UNAVAILABLE",
            ),
            (
                AutomationError::IoFailed {
                    details: "x".into(),
                },
                "AUTOMATION_IO_FAILED",
            ),
            (
                AutomationError::CorruptJson {
                    details: "x".into(),
                },
                "AUTOMATION_CORRUPT_JSON",
            ),
            (
                AutomationError::UnsafeRuleId {
                    rule_id: "../../etc/passwd".into(),
                },
                "AUTOMATION_UNSAFE_RULE_ID",
            ),
        ];
        for (err, expected_code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert!(!payload.message.is_empty());
        }
    }
}
