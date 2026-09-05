//! `HighlightError` — highlight detection's slice of the standardized error
//! model (master prompt §56, `docs/project-format.md` "Error model"),
//! following the exact same `{code, message, details, recoverable,
//! suggested_action}` pattern `ai::edit_plan::EditPlanError` established for
//! "an AI call succeeded, but the text it returned didn't validate".
//!
//! Deliberately covers only `highlights::semantic`'s parsing/validation of
//! AI-proposed highlight candidates — the real, local-signal half
//! (`highlights::signals`, `media::scene`) has no AI response to validate
//! and reuses `MediaError`/`VadError` for its own (ffmpeg/VAD) failure
//! modes, exactly like `ai::edit_plan::EditPlanError` doesn't duplicate
//! `AiProviderError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum HighlightError {
    #[error("could not parse AI highlight candidates as JSON: {details}")]
    MalformedJson { details: String },

    #[error("highlight candidate {index} is invalid: {details}")]
    InvalidCandidate { index: usize, details: String },
}

impl From<&HighlightError> for AppErrorPayload {
    fn from(err: &HighlightError) -> Self {
        let message = err.to_string();
        match err {
            HighlightError::MalformedJson { details } => {
                AppErrorPayload::new("HIGHLIGHT_MALFORMED_JSON", message)
                    .with_details(details.clone())
                    .recoverable(true)
                    .with_suggestion(
                        "The AI response was not a valid highlight-candidate JSON array; ask it to try again.",
                    )
            }
            HighlightError::InvalidCandidate { index, details } => {
                AppErrorPayload::new("HIGHLIGHT_INVALID_CANDIDATE", message)
                    .with_details(format!("candidate[{index}]: {details}"))
                    .recoverable(true)
                    .with_suggestion(
                        "Reject these highlight candidates and ask the AI to produce corrected ones.",
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
        let cases: Vec<(HighlightError, &str)> = vec![
            (
                HighlightError::MalformedJson {
                    details: "d".into(),
                },
                "HIGHLIGHT_MALFORMED_JSON",
            ),
            (
                HighlightError::InvalidCandidate {
                    index: 0,
                    details: "d".into(),
                },
                "HIGHLIGHT_INVALID_CANDIDATE",
            ),
        ];
        for (err, code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }
}
