//! `RenderGraph` construction (master prompt §69) and the local FFmpeg
//! render engine: `Project -> RenderGraph -> FFmpeg plan -> FFmpeg`.
//! Reimplemented from `vendor/autocut/src-tauri/src/export_mp4.rs`'s
//! concat-demuxer cutting technique, extended to full multi-track
//! compositing (effects, captions) that autocut's version does not support
//! (`docs/architecture-audit.md` §2/§4).
//!
//! Not implemented yet — this module is an intentionally empty, honest
//! placeholder. Lands in Phase 6 (`IMPLEMENTATION_PLAN.md`).
