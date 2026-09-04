//! `VadProvider` trait and Silero-VAD implementation (master prompt §13),
//! reimplemented from `vendor/autocut/src-tauri/src/vad.rs`'s two-phase
//! score/segment design (score once, cache; cheap re-segmentation on
//! parameter change — `docs/architecture-audit.md` §2/§3), using the same
//! `voice_activity_detector` crate, rewritten to i64-microsecond timebase.
//!
//! Not implemented yet — this module is an intentionally empty, honest
//! placeholder. Lands in Phase 5 (`IMPLEMENTATION_PLAN.md`).
