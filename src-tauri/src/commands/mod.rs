//! Tauri command surface (`#[tauri::command]` functions), kept thin per
//! master prompt §66 ("no giant god files") — each command here just
//! delegates to the matching domain module under `src-tauri/src/`. Command
//! modules mirror the domain module they front: `commands::project` fronts
//! `crate::project`, etc.

pub mod diagnostics;
pub mod media;
pub mod project;
