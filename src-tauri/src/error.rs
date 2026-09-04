//! Shared `{code, message, details, recoverable, suggested_action}` error
//! envelope (master prompt §56). Originally defined inline in
//! `project::error` (Phase 2, the only subsystem that existed yet); moved
//! here in Phase 3 now that `media`/`ffmpeg`/`audio`/`db` need the same
//! shape for their own error enums (`MediaError`, `FfmpegError`). Re-exported
//! from `project::error` so Phase 2 call sites keep compiling unchanged.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Serialize, Type)]
pub struct AppErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
    pub recoverable: bool,
    pub suggested_action: Option<String>,
}

impl AppErrorPayload {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            recoverable: false,
            suggested_action: None,
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggested_action = Some(suggestion.into());
        self
    }
}
