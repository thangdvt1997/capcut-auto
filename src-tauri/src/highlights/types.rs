//! The real `Highlight` type (Phase 10 follow-up, master prompt §21),
//! replacing `project::types::AiState.highlights`'s previous opaque
//! `Vec<serde_json::Value>` placeholder now that real detection logic exists
//! to produce one (`crate::highlights`) — additive schema change,
//! `docs/project-format.md` updated to match.

use serde::{Deserialize, Serialize};
use specta::Type;

/// One detected highlight candidate/segment: `{start, end, score, title,
/// reason}` (master prompt §21's exact return shape).
///
/// `score` is `0.0..=100.0` — matching master prompt §21's own UI mockup
/// ("Score: 92"), a deliberate difference from the `0.0..=1.0` convention
/// `vad::provider::SpeechSegment::confidence`/`ai::edit_plan::EditOperation
/// ::Remove.confidence` use elsewhere in this crate, so a highlight score
/// reads the same way the master prompt's own example shows it rather than
/// silently rescaling a UI-facing number a second time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Highlight {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub score: f32,
    pub title: String,
    pub reason: String,
}
