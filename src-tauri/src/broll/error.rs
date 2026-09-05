//! `BRollError`/`BRollSuggestError` — B-roll's slice of the standardized
//! error model (master prompt §56), following the exact same split
//! `highlights::error` established for its own local-signal-vs-AI-response
//! feature: `BRollError` covers [`super::provider::LocalLibraryBRollProvider`]'s
//! real local search failing (a `crate::media::error::MediaError` from the
//! underlying `db::search_media` call), while `BRollSuggestError` covers the
//! completely separate concern of validating whatever text an `AIProvider`
//! handed back against the strict `BRollSuggestion` schema
//! (`super::suggest`) — a provider call can succeed perfectly and still
//! return text that fails `BRollSuggestError` validation.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;
use crate::media::error::MediaError;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum BRollError {
    #[error("local B-roll search failed: {details}")]
    SearchFailed { details: String },
}

impl From<MediaError> for BRollError {
    fn from(err: MediaError) -> Self {
        BRollError::SearchFailed {
            details: err.to_string(),
        }
    }
}

impl From<&BRollError> for AppErrorPayload {
    fn from(err: &BRollError) -> Self {
        let message = err.to_string();
        match err {
            BRollError::SearchFailed { details } => {
                AppErrorPayload::new("BROLL_SEARCH_FAILED", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion("Check the local media library database and try again.")
            }
        }
    }
}

/// [`super::suggest`]'s validation-stage error — same two-stage split as
/// `ai::error::SmartEditError`/`highlights::error::HighlightError` (module
/// doc comment).
#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum BRollSuggestError {
    #[error("could not parse AI output as JSON: {details}")]
    MalformedJson { details: String },

    #[error("unsupported BRollSuggestion schema version: {version}")]
    UnsupportedVersion { version: u32 },

    #[error("suggestion {index} is invalid: {details}")]
    InvalidSuggestion { index: usize, details: String },
}

impl From<&BRollSuggestError> for AppErrorPayload {
    fn from(err: &BRollSuggestError) -> Self {
        let message = err.to_string();
        match err {
            BRollSuggestError::MalformedJson { details } => AppErrorPayload::new(
                "BROLL_SUGGEST_MALFORMED_JSON",
                message,
            )
            .with_details(details.clone())
            .recoverable(true)
            .with_suggestion(
                "The AI response was not valid B-roll suggestion JSON; ask it to try again.",
            ),
            BRollSuggestError::UnsupportedVersion { version } => {
                AppErrorPayload::new("BROLL_SUGGEST_UNSUPPORTED_VERSION", message)
                    .with_details(version.to_string())
                    .recoverable(false)
                    .with_suggestion(
                        "This app only understands B-roll suggestion schema version 1.",
                    )
            }
            BRollSuggestError::InvalidSuggestion { index, details } => {
                AppErrorPayload::new("BROLL_SUGGEST_INVALID_SUGGESTION", message)
                    .with_details(format!("suggestions[{index}]: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject this suggestion set and ask the AI to produce a corrected one.",
                    )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_broll_error_variant_maps_to_a_stable_code() {
        let err = BRollError::SearchFailed {
            details: "d".into(),
        };
        let payload = AppErrorPayload::from(&err);
        assert_eq!(payload.code, "BROLL_SEARCH_FAILED");
        assert!(!payload.message.is_empty());
    }

    #[test]
    fn every_broll_suggest_error_variant_maps_to_a_stable_code() {
        let cases: Vec<(BRollSuggestError, &str)> = vec![
            (
                BRollSuggestError::MalformedJson {
                    details: "d".into(),
                },
                "BROLL_SUGGEST_MALFORMED_JSON",
            ),
            (
                BRollSuggestError::UnsupportedVersion { version: 2 },
                "BROLL_SUGGEST_UNSUPPORTED_VERSION",
            ),
            (
                BRollSuggestError::InvalidSuggestion {
                    index: 0,
                    details: "d".into(),
                },
                "BROLL_SUGGEST_INVALID_SUGGESTION",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
