//! Export/Import Preset to/from an arbitrary file (`STUDIO_PLAN.md` Phase
//! D17 — `promt.md` §18's own "cho phép lưu toàn bộ config thành preset"
//! implies shareability).
//!
//! The `Preset` shape itself lives entirely on the frontend
//! (`src/stores/presets.svelte.ts`): a name plus a snapshot of each real
//! settings store's own non-secret fields (AI provider/model, translation
//! genre, voice provider + role mappings, `export_preset_id`, CapCut
//! draft-root override) — never a secret. See that store's own module doc
//! comment and `ai::credentials`'s: a preset only ever carries a stable
//! `credential_ref`-shaped string indirectly (by referencing a provider kind
//! whose key, if any, already lives in Windows Credential Manager), never
//! the key itself. There is deliberately no `Preset` struct in Rust —
//! duplicating that schema here just to (de)serialize opaque JSON text this
//! module never inspects would be dead weight this phase doesn't need.
//!
//! These two commands exist for one narrow reason: there is no other way
//! for the frontend to read/write an arbitrary file at a path the user
//! picked via the native save/open dialog. `@tauri-apps/plugin-dialog`'s
//! `save()`/`open()` (already a dependency, `package.json`) return only a
//! *path*, never file contents, and this app has no `@tauri-apps/plugin-fs`
//! dependency (confirmed absent: `Cargo.toml`/`capabilities/default.json`)
//! — so, exactly like `commands::project::save_project_as`/`open_project`'s
//! own doc comment already establishes ("a thin IPC wrapper... no new
//! file-I/O logic, only the missing command surface for it"), a real
//! command is the only honest option. Everything else about this feature
//! (the actual `Preset` schema, validation, and — critically — the
//! guarantee that no secret is ever included) lives entirely on the
//! frontend, in plain sight.
//!
//! Atomic write (write -> fsync -> rename) mirrors `templates::io::
//! write_atomic`/`ProjectV1::save_atomic`'s established discipline for
//! "never leave a truncated/corrupt file behind on a crash mid-write", even
//! though a preset export is a one-shot action, not a continuously-resaved
//! file.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use crate::error::AppErrorPayload;

const IO_ERROR_CODE: &str = "PRESET_FILE_IO_FAILED";

fn io_error(message: &str, err: impl std::fmt::Display) -> AppErrorPayload {
    AppErrorPayload::new(IO_ERROR_CODE, message)
        .with_details(err.to_string())
        .recoverable(true)
}

/// Writes `json` (an already-serialized preset — the frontend's
/// responsibility to build, per this module's own doc comment) to `path`,
/// atomically: `<path>.tmp` -> fsync -> rename over `path`.
#[tauri::command]
#[specta::specta]
pub fn export_preset_to_file(json: String, path: String) -> Result<(), AppErrorPayload> {
    let path = Path::new(&path);
    let tmp_path = path.with_extension("json.tmp");

    let mut file = File::create(&tmp_path)
        .map_err(|e| io_error(&format!("could not create {}", tmp_path.display()), e))?;
    file.write_all(json.as_bytes())
        .map_err(|e| io_error("could not write preset file", e))?;
    file.sync_all()
        .map_err(|e| io_error("could not flush preset file to disk", e))?;
    drop(file);

    fs::rename(&tmp_path, path)
        .map_err(|e| io_error(&format!("could not finalize {}", path.display()), e))
}

/// Reads the raw text contents of `path` back — the frontend parses and
/// validates the JSON itself (`presets.svelte.ts`'s own import path), the
/// same "backend hands back bytes, frontend owns the schema" split
/// `commands::templates::import_template` uses for a typed template instead.
#[tauri::command]
#[specta::specta]
pub fn import_preset_from_file(path: String) -> Result<String, AppErrorPayload> {
    fs::read_to_string(&path).map_err(|e| io_error(&format!("could not read {path}"), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_then_import_round_trips_the_exact_json_text() {
        let dir = std::env::temp_dir().join(format!(
            "ave-commands-presets-io-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("my-preset.json");
        let path_str = path.to_string_lossy().to_string();

        let json = r#"{"id":"abc","name":"Spanish Crime Movie"}"#.to_string();
        export_preset_to_file(json.clone(), path_str.clone()).expect("export_preset_to_file");

        let read_back = import_preset_from_file(path_str).expect("import_preset_from_file");
        assert_eq!(read_back, json);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_leaves_no_stray_tmp_file_behind_on_success() {
        let dir = std::env::temp_dir().join(format!(
            "ave-commands-presets-io-tmp-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("preset.json");
        export_preset_to_file("{}".to_string(), path.to_string_lossy().to_string())
            .expect("export_preset_to_file");

        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_on_a_missing_file_returns_a_real_error_not_a_panic() {
        let missing = std::env::temp_dir()
            .join(format!(
                "ave-commands-presets-missing-{}",
                uuid::Uuid::new_v4()
            ))
            .join("preset.json");
        let err = import_preset_from_file(missing.to_string_lossy().to_string()).unwrap_err();
        assert_eq!(err.code, IO_ERROR_CODE);
    }
}
