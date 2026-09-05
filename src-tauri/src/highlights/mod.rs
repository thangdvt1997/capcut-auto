//! Highlight detection (Phase 10 follow-up, master prompt §21): a real
//! `Highlight` type (`types`) — `{id, start, end, score, title, reason}` —
//! replacing `project::types::AiState.highlights`'s previous opaque
//! `Vec<serde_json::Value>` placeholder, plus real, independently-testable
//! local signals (`signals` — speech density via `vad::provider`'s existing
//! Silero VAD scoring, audio energy via `audio::pcm`'s existing PCM
//! extraction) and a genuinely real, non-AI scene-change detector
//! (`crate::media::scene`, FFmpeg's documented
//! `select='gt(scene,THRESHOLD)'` filter). `semantic` is the one piece that
//! needs an `AIProvider` call at all (asking the model to propose its own
//! candidate list with title/reason); `combine` blends the two into the
//! final scored list. `commands::highlights` is the Tauri command surface
//! tying every piece together.
//!
//! Kept as five small files rather than one, mirroring `ai`'s own
//! `provider`/`edit_plan`/`error` split: the local-signal half
//! (`signals`, and `media::scene` alongside it) must stay usable and
//! testable with zero AI provider configured at all (this pass's brief), so
//! it cannot live in the same module as anything that calls
//! `AIProvider::complete` (`semantic`).

pub mod combine;
pub mod error;
pub mod semantic;
pub mod signals;
pub mod types;

pub use types::Highlight;
