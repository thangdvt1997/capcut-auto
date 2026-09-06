//! `HistoryError` — the History subsystem's slice of the standardized
//! `{code, message, details, recoverable, suggested_action}` error model
//! (master prompt §56), same shape/pattern as `BatchError`/`TemplateError`.

use serde::Serialize;
use specta::Type;
use thiserror::Error;

use crate::error::AppErrorPayload;

#[derive(Debug, Clone, Serialize, Type, Error)]
#[serde(tag = "variant")]
pub enum HistoryError {
    #[error("history entry {id} not found")]
    NotFound { id: String },

    #[error("history database error: {details}")]
    DatabaseError { details: String },
}

impl From<&HistoryError> for AppErrorPayload {
    fn from(err: &HistoryError) -> Self {
        let message = err.to_string();
        match err {
            HistoryError::NotFound { id } => AppErrorPayload::new("HISTORY_NOT_FOUND", message)
                .with_details(id.clone())
                .recoverable(true)
                .with_suggestion("Check the history entry id; it may have been deleted."),
            HistoryError::DatabaseError { details } => {
                AppErrorPayload::new("HISTORY_DATABASE_ERROR", message)
                    .with_details(details.clone())
                    .recoverable(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_maps_to_a_stable_code() {
        let cases: Vec<(HistoryError, &str)> = vec![
            (
                HistoryError::NotFound { id: "x".into() },
                "HISTORY_NOT_FOUND",
            ),
            (
                HistoryError::DatabaseError {
                    details: "x".into(),
                },
                "HISTORY_DATABASE_ERROR",
            ),
        ];
        for (err, expected_code) in cases {
            let payload = AppErrorPayload::from(&err);
            assert_eq!(payload.code, expected_code);
            assert!(!payload.message.is_empty());
        }
    }
}
