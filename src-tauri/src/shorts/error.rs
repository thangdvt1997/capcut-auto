//! `ShortsError` — the Long-Video-to-Shorts pipeline's slice of the
//! standardized error model (master prompt §56), covering only what's
//! genuinely specific to this pipeline (transcript-dependency, settings
//! validation). Every other real failure mode (ffmpeg/probe failures,
//! subject tracking, highlight detection) reuses its own subsystem's
//! existing error type unchanged — exactly like `ai::edit_plan::EditPlanError`
//! doesn't duplicate `AiProviderError`, `highlights::HighlightError` doesn't
//! duplicate `MediaError`/`VadError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum ShortsError {
    /// This pipeline's own transcription-dependency design decision
    /// (`commands::shorts` module doc comment): `generate_shorts` requires a
    /// caller-supplied, non-empty transcript (produced beforehand via the
    /// existing granular `transcribe_media` job) rather than kicking off a
    /// background transcription job itself.
    #[error("generate_shorts requires a non-empty transcript; transcribe the media first")]
    TranscriptRequired,

    #[error("clip_count must be at least 1, got {clip_count}")]
    InvalidClipCount { clip_count: u32 },

    #[error("source media at {path} has invalid/zero dimensions or duration")]
    InvalidSourceMedia { path: String },
}

impl From<&ShortsError> for AppErrorPayload {
    fn from(err: &ShortsError) -> Self {
        let message = err.to_string();
        match err {
            ShortsError::TranscriptRequired => AppErrorPayload::new("SHORTS_TRANSCRIPT_REQUIRED", message)
                .recoverable(true)
                .with_suggestion(
                    "Run transcribe_media for this media first, then pass the resulting transcript entries to generate_shorts.",
                ),
            ShortsError::InvalidClipCount { clip_count } => {
                AppErrorPayload::new("SHORTS_INVALID_CLIP_COUNT", message)
                    .with_details(format!("clip_count={clip_count}"))
                    .recoverable(true)
                    .with_suggestion("Pass a clip_count of at least 1.")
            }
            ShortsError::InvalidSourceMedia { path } => {
                AppErrorPayload::new("SHORTS_INVALID_SOURCE_MEDIA", message)
                    .with_details(path.clone())
                    .recoverable(true)
                    .with_suggestion("Check the media file is a valid, readable video.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(ShortsError, &str)> = vec![
            (
                ShortsError::TranscriptRequired,
                "SHORTS_TRANSCRIPT_REQUIRED",
            ),
            (
                ShortsError::InvalidClipCount { clip_count: 0 },
                "SHORTS_INVALID_CLIP_COUNT",
            ),
            (
                ShortsError::InvalidSourceMedia {
                    path: "x.mp4".into(),
                },
                "SHORTS_INVALID_SOURCE_MEDIA",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
