//! Long-Video-to-Shorts pipeline (master prompt §22, `IMPLEMENTATION_PLAN.md`
//! Phase 11's final two backend bullets): **orchestration and candidate
//! ranking** over subsystems that already exist and are already fully
//! tested — transcription (Phase 7), highlight detection (Phase 10 follow-
//! up), `SubjectTracker`-based auto-reframe (this phase), caption generation
//! (Phase 8), and auto-zoom (this phase). This module does not build any new
//! detection/generation engine; it composes the existing ones.
//!
//! Pipeline stages (master prompt §22, this module's own split):
//!
//! ```text
//! Transcription -> Highlight Detection -> Candidate Ranking -> Clip
//! Extraction -> Reframe -> Captions -> Optional Zoom -> Export
//! ```
//!
//! - **Transcription**: NOT run by this module — see
//!   `commands::shorts` module doc comment for the full "why", the short
//!   version being: `transcribe_media` is an async, job-id-returning,
//!   event-emitting background job (Phase 7), not a synchronous call this
//!   pipeline could block on safely. The caller supplies an already-produced
//!   transcript; an empty one is a clear, specific error
//!   (`ShortsError::TranscriptRequired`), never a silent skip.
//! - **Highlight Detection**: reuses `commands::highlights::run_detection`
//!   directly (widened to `pub(crate)` for this reuse) — the exact same real
//!   pipeline (VAD speech density, PCM audio energy, real ffmpeg scene-change
//!   detection, optional AI semantic blending) `detect_highlights` already
//!   runs, not a second copy.
//! - **Candidate Ranking**: `ranking::select_top_non_overlapping` (a real
//!   non-overlapping top-K selection, not naive top-N-by-score) +
//!   `ranking::adjust_span_to_duration` (expand/contract each selected span
//!   to the requested target duration, clamped to the source media's own
//!   bounds).
//! - **Clip Extraction / Reframe / Captions / Optional Zoom**: composed onto
//!   one real `ProjectV1` per candidate by `build::build_short_project` —
//!   see that module's doc comment for the exact reframe-via-scale
//!   composition and keyframe-coexistence approach.
//! - **Export**: deliberately NOT run automatically by this pipeline — see
//!   `commands::shorts` module doc comment for the full reasoning
//!   ("each generated short should remain editable").

pub mod build;
pub mod captions;
pub mod error;
pub mod ranking;
pub mod settings;

pub use build::{build_short_project, ShortSourceContext};
pub use error::ShortsError;
pub use ranking::{adjust_span_to_duration, select_top_non_overlapping};
pub use settings::{DurationSetting, ShortsAspect, ShortsSettings};
