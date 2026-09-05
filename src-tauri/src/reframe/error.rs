//! `ReframeError` — this subsystem's slice of the standardized error model
//! (master prompt §56, `docs/project-format.md` "Error model"), following the
//! same `{code, message, details, recoverable, suggested_action}` pattern
//! `MediaError`/`RenderError` already established.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum ReframeError {
    #[error("could not locate the {tool} binary: {details}")]
    BinaryNotFound { tool: String, details: String },

    #[error("probing {path} failed: {details}")]
    ProbeFailed { path: String, details: String },

    #[error("{path} has no video stream to track")]
    NoVideoStream { path: String },

    #[error("frame sampling failed for {path}: {details}")]
    SamplingFailed { path: String, details: String },

    #[error("invalid target aspect ratio {width}x{height}: width and height must both be greater than zero")]
    InvalidTargetAspect { width: u32, height: u32 },

    #[error("source dimensions are zero or unknown for {path}")]
    InvalidSourceDimensions { path: String },
}

impl From<&ReframeError> for AppErrorPayload {
    fn from(err: &ReframeError) -> Self {
        let message = err.to_string();
        match err {
            ReframeError::BinaryNotFound { tool, details } => {
                AppErrorPayload::new("REFRAME_BINARY_NOT_FOUND", message)
                    .with_details(format!("tool={tool}: {details}"))
                    .recoverable(false)
                    .with_suggestion("Reinstall the app; the ffmpeg/ffprobe sidecar is missing.")
            }
            ReframeError::ProbeFailed { details, .. } => {
                AppErrorPayload::new("REFRAME_PROBE_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("The file may be corrupt or use an unsupported codec.")
            }
            ReframeError::NoVideoStream { path } => {
                AppErrorPayload::new("REFRAME_NO_VIDEO_STREAM", message)
                    .with_details(path.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Auto-reframe requires a video stream; choose a different source file.",
                    )
            }
            ReframeError::SamplingFailed { details, .. } => {
                AppErrorPayload::new("REFRAME_SAMPLING_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Retry; if this persists the source file may be corrupt.")
            }
            ReframeError::InvalidTargetAspect { .. } => {
                AppErrorPayload::new("REFRAME_INVALID_TARGET_ASPECT", message)
                    .recoverable(true)
                    .with_suggestion("Choose a target width/height greater than zero.")
            }
            ReframeError::InvalidSourceDimensions { path } => {
                AppErrorPayload::new("REFRAME_INVALID_SOURCE_DIMENSIONS", message)
                    .with_details(path.clone())
                    .recoverable(true)
                    .with_suggestion("The source file's dimensions could not be determined.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(ReframeError, &str)> = vec![
            (
                ReframeError::BinaryNotFound {
                    tool: "ffmpeg".into(),
                    details: "not found".into(),
                },
                "REFRAME_BINARY_NOT_FOUND",
            ),
            (
                ReframeError::ProbeFailed {
                    path: "a.mp4".into(),
                    details: "boom".into(),
                },
                "REFRAME_PROBE_FAILED",
            ),
            (
                ReframeError::NoVideoStream {
                    path: "a.mp3".into(),
                },
                "REFRAME_NO_VIDEO_STREAM",
            ),
            (
                ReframeError::SamplingFailed {
                    path: "a.mp4".into(),
                    details: "boom".into(),
                },
                "REFRAME_SAMPLING_FAILED",
            ),
            (
                ReframeError::InvalidTargetAspect {
                    width: 0,
                    height: 16,
                },
                "REFRAME_INVALID_TARGET_ASPECT",
            ),
            (
                ReframeError::InvalidSourceDimensions {
                    path: "a.mp4".into(),
                },
                "REFRAME_INVALID_SOURCE_DIMENSIONS",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
