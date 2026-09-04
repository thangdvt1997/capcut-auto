//! `RenderError` — this subsystem's slice of the standardized error model
//! (master prompt §56), following the same
//! `{code, message, details, recoverable, suggested_action}` pattern as
//! `MediaError`/`TimelineError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum RenderError {
    #[error(
        "clip {clip_id} references media {media_id}, which is not in the project's media list"
    )]
    MissingMedia { clip_id: String, media_id: String },

    #[error("the timeline has no visible video/audio content to render")]
    EmptyTimeline,

    #[error("unknown render preset id: {preset_id}")]
    UnknownPreset { preset_id: String },

    #[error("invalid render settings: {details}")]
    InvalidSettings { details: String },

    #[error("could not locate the {tool} binary: {details}")]
    BinaryNotFound { tool: String, details: String },

    #[error("render job {job_id} not found")]
    JobNotFound { job_id: String },

    #[error("render failed: {details}")]
    RenderFailed { details: String },

    #[error("render job was cancelled")]
    Cancelled,
}

impl From<&RenderError> for AppErrorPayload {
    fn from(err: &RenderError) -> Self {
        let message = err.to_string();
        match err {
            RenderError::MissingMedia { .. } => AppErrorPayload::new("RENDER_MISSING_MEDIA", message)
                .recoverable(true)
                .with_suggestion("Relink or remove the clip referencing missing media, then retry."),
            RenderError::EmptyTimeline => AppErrorPayload::new("RENDER_EMPTY_TIMELINE", message)
                .recoverable(true)
                .with_suggestion("Add at least one visible video/audio clip to the timeline before rendering."),
            RenderError::UnknownPreset { preset_id } => {
                AppErrorPayload::new("RENDER_UNKNOWN_PRESET", message)
                    .with_details(preset_id.clone())
                    .recoverable(true)
                    .with_suggestion("Choose one of the presets returned by list_render_presets, or omit preset_id for a fully custom render.")
            }
            RenderError::InvalidSettings { details } => {
                AppErrorPayload::new("RENDER_INVALID_SETTINGS", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Check resolution/fps/bitrate/CRF values are positive and codec/container are compatible.")
            }
            RenderError::BinaryNotFound { tool, details } => {
                AppErrorPayload::new("RENDER_BINARY_NOT_FOUND", message)
                    .with_details(format!("tool={tool}: {details}"))
                    .recoverable(false)
                    .with_suggestion("Reinstall the app; the ffmpeg sidecar is missing.")
            }
            RenderError::JobNotFound { job_id } => AppErrorPayload::new("RENDER_JOB_NOT_FOUND", message)
                .with_details(job_id.clone())
                .recoverable(true)
                .with_suggestion("The job may have already finished or been cancelled."),
            RenderError::RenderFailed { details } => AppErrorPayload::new("RENDER_FAILED", message)
                .with_details(details.clone())
                .recoverable(true)
                .with_suggestion("Check the output path is writable and the source media still exists, then retry."),
            RenderError::Cancelled => AppErrorPayload::new("RENDER_CANCELLED", message)
                .recoverable(true)
                .with_suggestion("Start a new render job if the output is still needed."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(RenderError, &str)> = vec![
            (
                RenderError::MissingMedia {
                    clip_id: "c1".into(),
                    media_id: "m1".into(),
                },
                "RENDER_MISSING_MEDIA",
            ),
            (RenderError::EmptyTimeline, "RENDER_EMPTY_TIMELINE"),
            (
                RenderError::UnknownPreset {
                    preset_id: "x".into(),
                },
                "RENDER_UNKNOWN_PRESET",
            ),
            (RenderError::Cancelled, "RENDER_CANCELLED"),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
