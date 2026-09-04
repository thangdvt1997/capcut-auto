// Prevents an additional console window on Windows in release builds.
// Pattern from vendor/autocut/src-tauri/src/main.rs (reuse permitted, see
// docs/upstream.md) — master prompt §45 "Windows process management".
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ai_video_editor_lib::run()
}
