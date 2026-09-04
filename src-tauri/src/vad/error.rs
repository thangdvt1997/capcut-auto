//! `VadError` — this subsystem's slice of the standardized error model
//! (master prompt §56), following the same `{code, message, details,
//! recoverable, suggested_action}` pattern as `project::error::ProjectError`,
//! `media::error::MediaError`, and `timeline::error::TimelineError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum VadError {
    #[error("failed to initialize the VAD model: {details}")]
    ModelInitFailed { details: String },

    #[error("VAD scoring was cancelled")]
    Cancelled,

    #[error("no cached VAD scores for media {media_id}; call analyze first")]
    NotScored { media_id: String },
}

impl From<&VadError> for AppErrorPayload {
    fn from(err: &VadError) -> Self {
        let message = err.to_string();
        match err {
            VadError::ModelInitFailed { details } => {
                AppErrorPayload::new("VAD_MODEL_INIT_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(false)
                    .with_suggestion("Reinstall the app; the VAD model may be missing or corrupt.")
            }
            VadError::Cancelled => AppErrorPayload::new("VAD_CANCELLED", message)
                .recoverable(true)
                .with_suggestion("Analysis was cancelled; re-run it if you still want results."),
            VadError::NotScored { media_id } => AppErrorPayload::new("VAD_NOT_SCORED", message)
                .with_details(media_id.clone())
                .recoverable(true)
                .with_suggestion(
                    "Run silence analysis on this media once before adjusting its parameters.",
                ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(VadError, &str)> = vec![
            (
                VadError::ModelInitFailed {
                    details: "x".into(),
                },
                "VAD_MODEL_INIT_FAILED",
            ),
            (VadError::Cancelled, "VAD_CANCELLED"),
            (
                VadError::NotScored {
                    media_id: "m1".into(),
                },
                "VAD_NOT_SCORED",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
