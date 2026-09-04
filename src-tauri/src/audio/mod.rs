//! PCM audio extraction and waveform (peak-per-bin) generation.
//! Reimplemented from `vendor/autocut/src-tauri/src/audio.rs` and
//! `waveform.rs` (incremental i16 read to bound memory — see
//! `docs/architecture-audit.md` §2), rewritten to the i64-microsecond
//! timebase where relevant.
//!
//! Not implemented yet — this module is an intentionally empty, honest
//! placeholder. Lands in Phase 3 (`IMPLEMENTATION_PLAN.md`).
