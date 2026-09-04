//! Standalone TypeScript bindings exporter. Regenerates
//! `src/types/bindings.ts` from the Rust command/type definitions without
//! launching the Tauri window — the app never opens a GUI here, so this
//! runs fine on a headless build box.
//!
//! Usage (from `src-tauri/`): `cargo run --bin export_bindings`

fn main() {
    ai_video_editor_lib::export_bindings().expect("failed to export TypeScript bindings");
}
