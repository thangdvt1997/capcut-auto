//! Rust core crate root. Module layout mirrors `docs/architecture.md`'s
//! component diagram and `docs/architecture-audit.md` §9's proposed tree.
//!
//! Most modules below are intentionally near-empty in Phase 2 (see each
//! module's doc comment for which phase implements it) — this is an honest
//! scaffold, not a stub pretending to work, per master prompt §75.
//! `project` is the exception: `docs/project-format.md`'s `ProjectV1`
//! schema is implemented for real here, since Phase 2's task explicitly
//! calls for the schema to land as real Rust structs.

// `capcut::script::ScriptMaterial::export_json`'s single `serde_json::json!`
// call covers every one of `draft_content.json`'s ~50 `materials` keys in
// one literal (matching the real CapCut draft schema key-for-key, per that
// module's doc comment) — past `serde_json`'s default `json_internal!`
// recursion limit for a single macro invocation. Raising the crate's
// recursion limit is the standard fix for a wide (not deep/infinite) macro
// expansion like this one, per `serde_json`'s own docs.
#![recursion_limit = "256"]

pub mod ai;
pub mod audio;
pub mod batch;
pub mod broll;
pub mod capcut;
pub mod captions;
pub mod commands;
pub mod db;
pub mod error;
pub mod fcpxml;
pub mod ffmpeg;
pub mod fs_safety;
pub mod highlights;
pub mod jobs;
pub mod logging;
pub mod media;
pub mod project;
pub mod reframe;
pub mod render;
pub mod shorts;
pub mod templates;
pub mod timeline;
pub mod transcription;
pub mod update;
pub mod vad;
pub mod zoom;

/// Builds the shared `tauri-specta` command/type registry. Used both by the
/// real running app (`run`, below) and by the standalone bindings exporter
/// (`src/bin/export_bindings.rs`) so `src/types/bindings.ts` can be
/// regenerated without launching a GUI window — useful in CI and on a
/// headless dev box.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        commands::diagnostics::get_shell_info,
        commands::capcut::detect_capcut_installations,
        commands::capcut::detect_capcut_registry_hints,
        commands::project::new_project,
        commands::media::ffmpeg_diagnostics,
        commands::media::probe_media_file,
        commands::media::import_media_paths,
        commands::media::import_media_folder,
        commands::media::generate_media_proxy,
        commands::media::compute_media_waveform,
        commands::media::generate_thumbnail_strip,
        commands::media::search_media_library,
        commands::media::list_media_library,
        commands::media::remove_media_from_library,
        commands::media::suggest_media_tags,
        commands::media::merge_media_tags,
        commands::timeline::load_timeline_project,
        commands::timeline::get_timeline_project,
        commands::timeline::split_clip,
        commands::timeline::trim_clip_start,
        commands::timeline::trim_clip_end,
        commands::timeline::move_clip,
        commands::timeline::delete_clip,
        commands::timeline::delete_clips,
        commands::timeline::duplicate_clip,
        commands::timeline::set_track_locked,
        commands::timeline::set_track_hidden,
        commands::timeline::set_track_muted,
        commands::timeline::set_track_solo,
        commands::timeline::effective_track_mute_state,
        commands::timeline::undo_timeline,
        commands::timeline::redo_timeline,
        commands::timeline::copy_clips,
        commands::timeline::paste_clips,
        commands::timeline::snap_to_candidates,
        commands::timeline::apply_silence_cuts,
        commands::timeline::apply_silence_cuts_to_track,
        commands::timeline::create_sync_group_manual,
        commands::timeline::create_sync_group_by_timecode,
        commands::captions::list_caption_templates,
        commands::captions::set_caption_styles,
        commands::captions::generate_captions,
        commands::captions::split_caption,
        commands::captions::merge_captions,
        commands::captions::retime_caption,
        commands::captions::find_replace_captions,
        commands::captions::bulk_set_caption_style,
        commands::vad::score_media_silence,
        commands::vad::segment_media_silence,
        commands::vad::build_silence_cutlist,
        commands::transcription::detect_filler_words,
        commands::transcription::list_installed_models,
        commands::transcription::list_available_models,
        commands::transcription::download_model,
        commands::transcription::cancel_model_download,
        commands::transcription::delete_model,
        commands::transcription::transcribe_media,
        commands::transcription::cancel_transcription,
        commands::render::list_render_presets,
        commands::render::detect_hardware_encoders,
        commands::render::start_render_job,
        commands::render::cancel_render_job,
        commands::ai::set_ai_api_key,
        commands::ai::delete_ai_api_key,
        commands::ai::test_ai_connection,
        commands::ai::validate_edit_plan,
        commands::ai::build_cuts_from_edit_plan,
        commands::ai::apply_edit_plan_to_clip,
        commands::ai::apply_edit_plan_to_track,
        commands::ai::generate_edit_plan_from_nl_command,
        commands::ai::analyze_smart_edit,
        commands::ai::build_cuts_from_smart_edit_recommendations,
        commands::ai::apply_smart_edit_recommendations_to_clip,
        commands::ai::apply_smart_edit_recommendations_to_track,
        commands::highlights::detect_media_scene_changes,
        commands::highlights::detect_highlights,
        commands::reframe::auto_reframe_media,
        commands::scenes::detect_media_scenes,
        commands::scenes::split_clip_at_scenes,
        commands::scenes::remove_scenes_from_clip,
        commands::scenes::remove_scenes_from_track,
        commands::scenes::generate_highlights_from_scenes,
        commands::zoom::generate_zoom_triggers,
        commands::zoom::generate_zoom_keyframes,
        commands::zoom::apply_auto_zoom_to_clip,
        commands::broll::search_local_broll,
        commands::broll::suggest_broll_from_transcript,
        commands::broll::suggest_and_search_broll,
        commands::templates::list_templates,
        commands::templates::save_as_template,
        commands::templates::import_template,
        commands::templates::export_template,
        commands::templates::delete_custom_template,
        commands::shorts::generate_shorts,
        commands::batch::start_batch,
        commands::batch::list_batch_jobs,
        commands::batch::pause_batch_job,
        commands::batch::resume_batch_job,
        commands::batch::cancel_batch_job,
        commands::batch::retry_batch_job,
        commands::update::check_for_update,
        commands::update::install_available_update,
        commands::diagnostics::get_system_information,
        commands::diagnostics::get_logs_folder_path,
        commands::diagnostics::open_logs_folder,
        commands::diagnostics::get_last_session_status,
        fcpxml::export::export_fcpxml,
        capcut::export::export_project_to_capcut_draft,
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
        // Phase 12 auto-update architecture (master prompt §62). Config
        // (`endpoints`/`pubkey`/`windows.installMode`) comes from
        // `tauri.conf.json`'s `plugins.updater` — still a documented
        // human-fill-in placeholder (empty `endpoints`, a placeholder
        // `pubkey` string) until a real update-manifest host and signing
        // keypair exist; see that file's own `_comment_*` keys and
        // `commands::update` module doc comment for exactly what to fill in
        // and how. Paired with `tauri_plugin_process` immediately below,
        // which backs the restart-after-install action
        // `commands::update::install_available_update` falls back to on
        // platforms where the updater's own `install()` doesn't already
        // exit the process itself (Windows does, per that crate's docs).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(specta_builder.invoke_handler())
        .setup(move |app| {
            specta_builder.mount_events(app);
            // Phase 12 crash handling/logging (master prompt §54/§55/§86),
            // run first, before any other startup step below: a panic
            // during, say, media-library init should still be captured by
            // the panic hook, and "was the last exit clean" has to be
            // decided before anything in *this* session could write to the
            // marker `check_and_mark_session_start` itself manages. Real
            // logging-init failure (unwritable app-data dir, etc.) is
            // intentionally non-fatal — printed to stderr and skipped —
            // since crash-logging infrastructure failing should never be
            // *why* the whole app refuses to start. See `crate::logging`
            // module doc comment for the full design.
            match crate::commands::diagnostics::logs_dir(app.handle()) {
                Ok(log_dir) => match crate::logging::init_logging(&log_dir) {
                    Ok(guard) => {
                        crate::logging::keep_alive_for_process_lifetime(guard);
                        crate::logging::install_panic_hook();
                        tracing::info!(
                            "AI Video Editor starting up (log dir: {})",
                            log_dir.display()
                        );
                    }
                    Err(e) => eprintln!(
                        "warning: failed to initialize file logging at {}: {e}",
                        log_dir.display()
                    ),
                },
                Err(e) => eprintln!("warning: failed to resolve logs directory: {e:?}"),
            }
            let previous_exit_was_clean = tauri::Manager::path(app)
                .app_local_data_dir()
                .ok()
                .and_then(|dir| crate::logging::check_and_mark_session_start(&dir).ok())
                // Best-effort: if the app-data dir can't even be resolved,
                // assume clean rather than block startup on a recovery flag.
                .unwrap_or(true);
            tauri::Manager::manage(
                app,
                crate::logging::SessionStatus {
                    previous_exit_was_clean,
                    recovered_project_path: None,
                },
            );
            // Opens/creates the media library SQLite database (master
            // prompt §35) as managed state, used by every
            // `commands::media::*` command. A failure here is a real
            // startup problem (unwritable app-data dir, corrupt db file)
            // worth failing loudly on rather than limping along with no
            // media library.
            commands::media::init_media_library(app.handle())
                .expect("failed to initialize media library database");
            // The live timeline session (current project + undo history +
            // clipboard), managed the same way as `MediaLibrary` above.
            // Starts empty; `commands::timeline::load_timeline_project`
            // populates it once a project is opened/created.
            tauri::Manager::manage(app, crate::timeline::session::TimelineState::default());
            // VAD chunk-score cache (master prompt §13 / Phase 5), keyed by
            // media id — see `crate::vad::cache` module doc comment for why
            // the expensive scoring phase is cached here rather than
            // recomputed on every parameter change.
            tauri::Manager::manage(app, crate::vad::VadCache::default());
            // Live render jobs (job_id -> cancellation flag), managed the
            // same way as `MediaLibrary`/`VadCache` above — see
            // `commands::render::RenderJobs` doc comment.
            tauri::Manager::manage(app, crate::commands::render::RenderJobs::default());
            // Live model downloads and transcription jobs (Phase 7, master
            // prompt §14/§60), same `id -> Arc<AtomicBool>` cancellation-map
            // pattern as `RenderJobs` above — see
            // `commands::transcription::{ModelDownloadJobs, TranscriptionJobs}`
            // doc comments.
            tauri::Manager::manage(
                app,
                crate::commands::transcription::ModelDownloadJobs::default(),
            );
            tauri::Manager::manage(
                app,
                crate::commands::transcription::TranscriptionJobs::default(),
            );
            // Live batch jobs (Phase 11, master prompt §42/§43): one
            // `BatchJobManager` for the whole app, tracking every in-flight
            // batch job by id — see `batch::manager` module doc comment.
            tauri::Manager::manage(app, crate::batch::BatchJobManager::default());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running AI Video Editor")
        // `.build()` + a `run` callback (rather than the simpler
        // `.run(tauri::generate_context!())` every other phase used) is the
        // only way to observe `RunEvent::Exit` — the one lifecycle event
        // that corresponds to an actual graceful shutdown, as opposed to a
        // crash/panic/force-kill. That's what lets `mark_clean_exit` (master
        // prompt §86 unclean-exit recovery marker — see `crate::logging`
        // module doc comment) only ever fire on a real clean exit.
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                // Last-resort orphan-process sweep (master prompt §45): force
                // -kill any ffmpeg/ffprobe child still tracked at this point.
                // Cooperative `AtomicBool` cancellation (every render/proxy/
                // batch job already threads one through) only works if its
                // worker thread gets scheduled again before the process
                // actually exits, which is not guaranteed here — see
                // `ffmpeg::command`'s registry module doc comment for the
                // full reasoning. This call is synchronous and runs before
                // the process actually terminates, unlike a flag flip.
                crate::ffmpeg::command::kill_all_tracked_children();
                if let Ok(dir) = tauri::Manager::path(app_handle).app_local_data_dir() {
                    crate::logging::mark_clean_exit(&dir);
                }
            }
        });
}
