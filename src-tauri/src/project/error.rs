//! `ProjectError` — this schema module's slice of the standardized error
//! model (master prompt §56, `docs/project-format.md` "Error model"). The
//! other seven error enums (`MediaError`, `FfmpegError`,
//! `TranscriptionError`, `AiProviderError`, `CapCutError`, `RenderError`,
//! `ModelError`) are NOT implemented here — each lands with its own
//! subsystem in a later phase. Implementing all eight now, before the
//! subsystems that would actually throw them exist, would be exactly the
//! "looks done but isn't" scaffolding master prompt §75 warns against.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum ProjectError {
    #[error(
        "project schema version {found} is newer than this app supports (max {max_supported})"
    )]
    SchemaVersionTooNew { found: u32, max_supported: u32 },

    #[error("project.json is not valid JSON: {details}")]
    CorruptJson { details: String },

    #[error("migration from version {from} to {to} failed: {details}")]
    MigrationFailed { from: u32, to: u32, details: String },

    #[error("failed to atomically write project.json: {details}")]
    AtomicWriteFailed { details: String },

    #[error("a recovery snapshot was found for this project: {snapshot_path}")]
    RecoverySnapshotFound { snapshot_path: String },
}

/// The `{code, message, details, recoverable, suggested_action}` envelope
/// every error subsystem sends to the frontend (master prompt §56). This is
/// the reference implementation for `ProjectError`; other subsystems should
/// follow the same shape when their own error enums land.
#[derive(Debug, Clone, Serialize, Type)]
pub struct AppErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
    pub recoverable: bool,
    pub suggested_action: Option<String>,
}

impl From<&ProjectError> for AppErrorPayload {
    fn from(err: &ProjectError) -> Self {
        let message = err.to_string();
        match err {
            ProjectError::SchemaVersionTooNew {
                found,
                max_supported,
            } => AppErrorPayload {
                code: "PROJECT_SCHEMA_VERSION_TOO_NEW".into(),
                message,
                details: Some(format!("found={found} max_supported={max_supported}")),
                recoverable: false,
                suggested_action: Some(
                    "Update the app to a version that supports this project file.".into(),
                ),
            },
            ProjectError::CorruptJson { details } => AppErrorPayload {
                code: "PROJECT_CORRUPT_JSON".into(),
                message,
                details: Some(details.clone()),
                recoverable: true,
                suggested_action: Some(
                    "Restore from the most recent autosave/recovery snapshot.".into(),
                ),
            },
            ProjectError::MigrationFailed { from, to, details } => AppErrorPayload {
                code: "PROJECT_MIGRATION_FAILED".into(),
                message,
                details: Some(format!("from={from} to={to}: {details}")),
                recoverable: false,
                suggested_action: Some(
                    "Keep a backup of the original file and report this as a bug.".into(),
                ),
            },
            ProjectError::AtomicWriteFailed { details } => AppErrorPayload {
                code: "PROJECT_ATOMIC_WRITE_FAILED".into(),
                message,
                details: Some(details.clone()),
                recoverable: true,
                suggested_action: Some(
                    "Check disk space and folder permissions, then retry saving.".into(),
                ),
            },
            ProjectError::RecoverySnapshotFound { snapshot_path } => AppErrorPayload {
                code: "PROJECT_RECOVERY_SNAPSHOT_FOUND".into(),
                message,
                details: Some(snapshot_path.clone()),
                recoverable: true,
                suggested_action: Some(
                    "Choose whether to restore the recovery snapshot or discard it.".into(),
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AppErrorPayload` isn't wired into any command yet (no Phase 2 code
    /// path actually produces a `ProjectError` over IPC) — this test is
    /// what exercises the `From` mapping today, standing in for that future
    /// call site so every variant's `code`/`recoverable` pairing is
    /// verified now rather than whenever it first gets used for real.
    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(ProjectError, &str, bool)> = vec![
            (
                ProjectError::SchemaVersionTooNew {
                    found: 2,
                    max_supported: 1,
                },
                "PROJECT_SCHEMA_VERSION_TOO_NEW",
                false,
            ),
            (
                ProjectError::CorruptJson {
                    details: "eof".into(),
                },
                "PROJECT_CORRUPT_JSON",
                true,
            ),
            (
                ProjectError::MigrationFailed {
                    from: 1,
                    to: 2,
                    details: "no path".into(),
                },
                "PROJECT_MIGRATION_FAILED",
                false,
            ),
            (
                ProjectError::AtomicWriteFailed {
                    details: "disk full".into(),
                },
                "PROJECT_ATOMIC_WRITE_FAILED",
                true,
            ),
            (
                ProjectError::RecoverySnapshotFound {
                    snapshot_path: "/tmp/x.bak".into(),
                },
                "PROJECT_RECOVERY_SNAPSHOT_FOUND",
                true,
            ),
        ];

        for (err, expected_code, expected_recoverable) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert_eq!(payload.recoverable, expected_recoverable);
            assert!(!payload.message.is_empty());
            assert!(payload.suggested_action.is_some());
        }
    }
}
