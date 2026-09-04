//! Media probing (ffprobe wrapper -> `MediaInfo`), thumbnail generation,
//! proxy media generation, media import (drag & drop, file picker).
//! Reimplemented from `vendor/autocut/src-tauri/src/probe.rs`'s design
//! (direct ffprobe JSON parsing, no `pymediainfo` dependency — see
//! `docs/architecture-audit.md` §4), rewritten to the i64-microsecond
//! timebase.
//!
//! Not implemented yet — this module is an intentionally empty, honest
//! placeholder. Lands in Phase 3 (`IMPLEMENTATION_PLAN.md`).
