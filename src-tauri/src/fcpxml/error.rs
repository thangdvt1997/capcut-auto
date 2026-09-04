//! `FcpxmlError` — this subsystem's slice of the standardized error model
//! (master prompt §56), following the same
//! `{code, message, details, recoverable, suggested_action}` pattern already
//! established by `project::error::ProjectError`, `media::error::MediaError`
//! and `timeline::error::TimelineError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum FcpxmlError {
    /// Mirrors autocut's `export_fcpxml::render`'s `ensure_exportable`
    /// check: an FCPXML with an empty `<spine>` is well-formed XML that
    /// imports as an empty timeline in the NLE, so there is no later point
    /// at which anything would notice. Structural, not a courtesy check.
    #[error("timeline has no exportable video/audio/image clips: {details}")]
    EmptyTimeline { details: String },

    #[error("failed to write FCPXML file to {path}: {details}")]
    WriteFailed { path: String, details: String },
}

impl From<&FcpxmlError> for AppErrorPayload {
    fn from(err: &FcpxmlError) -> Self {
        let message = err.to_string();
        match err {
            FcpxmlError::EmptyTimeline { details } => {
                AppErrorPayload::new("FCPXML_EMPTY_TIMELINE", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Add at least one enabled clip on a visible video, audio, image, or \
                         overlay track before exporting.",
                    )
            }
            FcpxmlError::WriteFailed { path, details } => {
                AppErrorPayload::new("FCPXML_WRITE_FAILED", message)
                    .with_details(format!("path={path}: {details}"))
                    .recoverable(true)
                    .with_suggestion("Check disk space and folder permissions, then retry export.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(FcpxmlError, &str)> = vec![
            (
                FcpxmlError::EmptyTimeline {
                    details: "no tracks".into(),
                },
                "FCPXML_EMPTY_TIMELINE",
            ),
            (
                FcpxmlError::WriteFailed {
                    path: "out.fcpxml".into(),
                    details: "disk full".into(),
                },
                "FCPXML_WRITE_FAILED",
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
