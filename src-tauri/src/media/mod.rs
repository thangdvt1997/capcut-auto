//! Media engine: ffprobe-backed metadata extraction (`probe`), format
//! classification and folder scanning for import (`import`), thumbnail
//! generation (`thumbnail`), proxy media generation (`proxy`), real
//! non-AI scene-change detection (`scene`, Phase 10 follow-up — highlight
//! detection's visual signal), and this subsystem's error type (`error`).
//!
//! Reimplemented from `vendor/autocut/src-tauri/src/probe.rs`'s design
//! (direct ffprobe JSON parsing, no `pymediainfo` dependency — see
//! `docs/architecture-audit.md` §4), rewritten to the i64-microsecond
//! timebase (`docs/project-format.md`).

pub mod error;
pub mod import;
pub mod probe;
pub mod proxy;
pub mod scene;
pub mod thumbnail;
