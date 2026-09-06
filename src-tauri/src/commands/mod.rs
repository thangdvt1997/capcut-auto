//! Tauri command surface (`#[tauri::command]` functions), kept thin per
//! master prompt §66 ("no giant god files") — each command here just
//! delegates to the matching domain module under `src-tauri/src/`. Command
//! modules mirror the domain module they front: `commands::project` fronts
//! `crate::project`, etc.

pub mod ai;
pub mod assets;
pub mod auto_template;
pub mod batch;
pub mod broll;
pub mod capcut;
pub mod captions;
pub mod diagnostics;
pub mod highlights;
pub mod history;
pub mod media;
pub mod project;
pub mod reframe;
pub mod render;
pub mod scenes;
pub mod shorts;
pub mod templates;
pub mod timeline;
pub mod transcription;
pub mod update;
pub mod vad;
pub mod zoom;
