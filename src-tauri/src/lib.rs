//! Rust core crate root. Module layout mirrors `docs/architecture.md`'s
//! component diagram and `docs/architecture-audit.md` §9's proposed tree.
//!
//! Most modules below are intentionally near-empty in Phase 2 (see each
//! module's doc comment for which phase implements it) — this is an honest
//! scaffold, not a stub pretending to work, per master prompt §75.
//! `project` is the exception: `docs/project-format.md`'s `ProjectV1`
//! schema is implemented for real here, since Phase 2's task explicitly
//! calls for the schema to land as real Rust structs.

pub mod ai;
pub mod audio;
pub mod capcut;
pub mod commands;
pub mod db;
pub mod fcpxml;
pub mod ffmpeg;
pub mod jobs;
pub mod media;
pub mod project;
pub mod render;
pub mod timeline;
pub mod vad;

/// Builds the shared `tauri-specta` command/type registry. Used both by the
/// real running app (`run`, below) and by the standalone bindings exporter
/// (`src/bin/export_bindings.rs`) so `src/types/bindings.ts` can be
/// regenerated without launching a GUI window — useful in CI and on a
/// headless dev box.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        commands::diagnostics::get_shell_info,
        commands::project::new_project,
    ])
}

/// specta forbids exporting `i64`/`u64` as a TypeScript `number` by default
/// (JS numbers can't exactly represent the full 64-bit range) and requires
/// an explicit opt-in. Every `_us` field in `project::types` is `i64`
/// microseconds (master prompt §67); `Number` is safe here in practice —
/// `Number.MAX_SAFE_INTEGER` microseconds is about 285 years of timeline,
/// far beyond any real project — so we opt in rather than serialize
/// durations as strings, which would just push a parse-back burden onto
/// every frontend consumer for no real precision benefit.
fn typescript_config() -> specta_typescript::Typescript {
    specta_typescript::Typescript::default().bigint(specta_typescript::BigIntExportBehavior::Number)
}

/// Regenerates `src/types/bindings.ts` from the current command/type
/// definitions. See `specta_builder` doc comment for why this exists as a
/// standalone entry point instead of only running inside `run()`.
pub fn export_bindings() -> Result<(), specta_typescript::ExportError> {
    specta_builder().export(typescript_config(), "../src/types/bindings.ts")
}

/// Excluded from the lib's own test build on purpose. `generate_context!` is
/// a proc macro that validates `tauri.conf.json` at compile time, including
/// that `frontendDist` exists — so leaving it in would make `cargo test`
/// refuse to compile until someone had run a frontend build. None of the
/// unit tests touch the Tauri runtime. The binary target still compiles the
/// lib without cfg(test), so the real app is unaffected. (Pattern from
/// vendor/autocut/src-tauri/src/lib.rs, reuse permitted per docs/upstream.md.)
#[cfg(not(test))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta_builder = specta_builder();

    #[cfg(debug_assertions)]
    specta_builder
        .export(typescript_config(), "../src/types/bindings.ts")
        .expect("failed to export TypeScript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(specta_builder.invoke_handler())
        .setup(move |app| {
            specta_builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running AI Video Editor");
}
