//! Caption generation and style catalog (Phase 8, master prompt §26/§27).
//!
//! Deliberately separate from `timeline`: everything here is a pure, stateless
//! function of its inputs — `generate::generate_captions_from_transcript`
//! turns a transcript into `Vec<Caption>`, `styles::all_caption_templates`
//! returns the built-in style catalog — neither touches a `ProjectV1` or
//! produces a `timeline::command::Command`. Caption *correction* operations
//! (split/merge/retime/find-replace/bulk-style), which DO need to mutate an
//! existing project's caption list through the undo-aware command machinery,
//! live in `timeline::captions` instead, alongside `timeline::ops`/`silence`/
//! `sync` — the same module-boundary logic that file already documents.
//!
//! The Tauri command surface fronting both halves lives in
//! `crate::commands::captions`.

pub mod generate;
pub mod styles;
