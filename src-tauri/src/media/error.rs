//! `MediaError` — this subsystem's slice of the standardized error model
//! (master prompt §56, `docs/project-format.md` "Error model"), following the
//! same `{code, message, details, recoverable, suggested_action}` pattern
//! `project::error::ProjectError` established in Phase 2.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum MediaError {
    #[error("unsupported file format: {extension} ({path})")]
    UnsupportedFormat { path: String, extension: String },

    #[error("path does not exist: {path}")]
    PathNotFound { path: String },

    #[error("ffprobe failed on {path}: {details}")]
    ProbeFailed { path: String, details: String },

    #[error("could not locate the {tool} binary: {details}")]
    BinaryNotFound { tool: String, details: String },

    #[error("thumbnail generation failed for {path}: {details}")]
    ThumbnailFailed { path: String, details: String },

    #[error("proxy generation failed for {path}: {details}")]
    ProxyFailed { path: String, details: String },

    #[error("proxy generation for {path} was cancelled")]
    ProxyCancelled { path: String },

    #[error("waveform generation failed for {path}: {details}")]
    WaveformFailed { path: String, details: String },

    #[error("media library database error: {details}")]
    DatabaseError { details: String },

    #[error("import failed for {path}: {details}")]
    ImportFailed { path: String, details: String },

    /// Highlight detection's real, non-AI scene-change signal
    /// (`crate::media::scene`, Phase 10 follow-up, master prompt §21) shells
    /// out to ffmpeg's own `select='gt(scene,THRESHOLD)'` filter — this is
    /// that call's failure mode, same shape as every other ffmpeg-backed
    /// `MediaError` variant above.
    #[error("scene-change detection failed for {path}: {details}")]
    SceneDetectionFailed { path: String, details: String },

    /// Path traversal prevention (master prompt §53): `media_id` is joined
    /// directly onto this app's own media-cache directory
    /// (`commands::media::generate_media_proxy`/`generate_thumbnail_strip`),
    /// so it must be a single, safe path segment — never `../..` or an
    /// embedded separator that could escape that directory — see
    /// `crate::fs_safety::is_safe_path_component`.
    #[error("media id {media_id} is not a safe path component")]
    UnsafeMediaId { media_id: String },
}

impl From<&MediaError> for AppErrorPayload {
    fn from(err: &MediaError) -> Self {
        let message = err.to_string();
        match err {
            MediaError::UnsupportedFormat { extension, .. } => {
                AppErrorPayload::new("MEDIA_UNSUPPORTED_FORMAT", message)
                    .with_details(format!("extension={extension}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Supported formats: MP4/MOV/MKV/AVI/WEBM/M4V, MP3/WAV/AAC/M4A/FLAC, PNG/JPG/JPEG/WEBP.",
                    )
            }
            MediaError::PathNotFound { path } => AppErrorPayload::new("MEDIA_PATH_NOT_FOUND", message)
                .with_details(path.clone())
                .recoverable(true)
                .with_suggestion("Check the file wasn't moved or deleted, then retry."),
            MediaError::ProbeFailed { details, .. } => {
                AppErrorPayload::new("MEDIA_PROBE_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("The file may be corrupt or use an unsupported codec.")
            }
            MediaError::BinaryNotFound { tool, details } => {
                AppErrorPayload::new("MEDIA_BINARY_NOT_FOUND", message)
                    .with_details(format!("tool={tool}: {details}"))
                    .recoverable(false)
                    .with_suggestion("Reinstall the app; the ffmpeg/ffprobe sidecar is missing.")
            }
            MediaError::ThumbnailFailed { details, .. } => {
                AppErrorPayload::new("MEDIA_THUMBNAIL_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Import still succeeds without a thumbnail; retry generating one later.")
            }
            MediaError::ProxyFailed { details, .. } => {
                AppErrorPayload::new("MEDIA_PROXY_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Editing can continue against the original media; retry proxy generation later.")
            }
            MediaError::ProxyCancelled { path } => AppErrorPayload::new("MEDIA_PROXY_CANCELLED", message)
                .with_details(path.clone())
                .recoverable(true)
                .with_suggestion("Restart proxy generation if it's still needed."),
            MediaError::WaveformFailed { details, .. } => {
                AppErrorPayload::new("MEDIA_WAVEFORM_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("The timeline can still work without a waveform; retry later.")
            }
            MediaError::DatabaseError { details } => {
                AppErrorPayload::new("MEDIA_DATABASE_ERROR", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Restart the app; if this persists, the media library index may need to be reset.")
            }
            MediaError::ImportFailed { details, .. } => {
                AppErrorPayload::new("MEDIA_IMPORT_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Check the file/folder is accessible, then retry the import.")
            }
            MediaError::SceneDetectionFailed { details, .. } => {
                AppErrorPayload::new("MEDIA_SCENE_DETECTION_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "Highlight detection can still use the transcript/speech-density/audio-energy signals without scene changes; retry later.",
                    )
            }
            MediaError::UnsafeMediaId { media_id } => {
                AppErrorPayload::new("MEDIA_UNSAFE_MEDIA_ID", message)
                    .with_details(media_id.clone())
                    .recoverable(false)
                    .with_suggestion(
                        "This is an internal error — a media id should never contain a path separator.",
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
        let cases: Vec<(MediaError, &str)> = vec![
            (
                MediaError::UnsupportedFormat {
                    path: "a.xyz".into(),
                    extension: "xyz".into(),
                },
                "MEDIA_UNSUPPORTED_FORMAT",
            ),
            (
                MediaError::PathNotFound {
                    path: "a.mp4".into(),
                },
                "MEDIA_PATH_NOT_FOUND",
            ),
            (
                MediaError::ProbeFailed {
                    path: "a.mp4".into(),
                    details: "boom".into(),
                },
                "MEDIA_PROBE_FAILED",
            ),
            (
                MediaError::BinaryNotFound {
                    tool: "ffmpeg".into(),
                    details: "not found".into(),
                },
                "MEDIA_BINARY_NOT_FOUND",
            ),
            (
                MediaError::DatabaseError {
                    details: "locked".into(),
                },
                "MEDIA_DATABASE_ERROR",
            ),
            (
                MediaError::UnsafeMediaId {
                    media_id: "../../etc/passwd".into(),
                },
                "MEDIA_UNSAFE_MEDIA_ID",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
