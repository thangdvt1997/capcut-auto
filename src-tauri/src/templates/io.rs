//! Import/Export Template (master prompt §36) and custom-template storage
//! under this app's own `templates/` app-data directory (§36's literal
//! "Directory: templates/" instruction — see
//! `commands::templates::templates_dir` for the exact
//! `$APPLOCALDATA`-equivalent resolution, mirroring
//! `commands::transcription::models_dir`'s pattern for the Model Manager).
//!
//! Saving/deleting a custom template on disk reuses `project::io::save_atomic`'s
//! exact temp-file-then-rename-then-fsync discipline (never a bare
//! overwrite) rather than inventing a second convention for the same
//! problem.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::error::TemplateError;
use super::Template;

/// One custom template's on-disk filename inside the `templates/` directory:
/// `<id>.json`. Rejects a `template_id` that isn't a safe single path
/// component (master prompt §53 path traversal prevention —
/// `TemplateError::UnsafeTemplateId` doc comment) before ever joining it
/// onto `dir`.
fn template_file_path(dir: &Path, template_id: &str) -> Result<PathBuf, TemplateError> {
    if !crate::fs_safety::is_safe_path_component(template_id) {
        return Err(TemplateError::UnsafeTemplateId {
            template_id: template_id.to_string(),
        });
    }
    Ok(dir.join(format!("{template_id}.json")))
}

/// Serialize -> write to `<path>.tmp` -> fsync -> rename over `path`. Same
/// atomic-write discipline as `project::io::save_atomic`, reused for
/// whichever path a caller wants a `Template` written to (a `templates/`
/// slot or an arbitrary user-chosen export file).
fn write_atomic(path: &Path, template: &Template) -> Result<(), TemplateError> {
    let json = serde_json::to_vec_pretty(template).map_err(|e| TemplateError::IoFailed {
        details: format!("serialize failed: {e}"),
    })?;

    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp_path).map_err(|e| TemplateError::IoFailed {
            details: format!("could not create {}: {e}", tmp_path.display()),
        })?;
        file.write_all(&json).map_err(|e| TemplateError::IoFailed {
            details: format!("write failed: {e}"),
        })?;
        file.sync_all().map_err(|e| TemplateError::IoFailed {
            details: format!("fsync failed: {e}"),
        })?;
    }

    fs::rename(&tmp_path, path).map_err(|e| TemplateError::IoFailed {
        details: format!(
            "rename {} -> {} failed: {e}",
            tmp_path.display(),
            path.display()
        ),
    })
}

fn read_template(path: &Path) -> Result<Template, TemplateError> {
    let bytes = fs::read(path).map_err(|e| TemplateError::IoFailed {
        details: format!("could not read {}: {e}", path.display()),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| TemplateError::CorruptJson {
        details: format!("{}: {e}", path.display()),
    })
}

/// Atomically writes `template` into the `templates/` directory as
/// `<id>.json`, creating the directory if it doesn't exist yet. Returns the
/// path written.
pub fn save_custom_template(dir: &Path, template: &Template) -> Result<PathBuf, TemplateError> {
    fs::create_dir_all(dir).map_err(|e| TemplateError::IoFailed {
        details: format!("could not create {}: {e}", dir.display()),
    })?;
    let path = template_file_path(dir, &template.id)?;
    write_atomic(&path, template)?;
    Ok(path)
}

/// Lists every custom template saved under `dir` (an empty `Vec`, not an
/// error, if the directory doesn't exist yet — "no custom templates saved",
/// same "absence means default/empty" convention `ProjectV1`'s additive
/// overlay maps use). Sorted by name for a stable listing order.
pub fn list_custom_templates(dir: &Path) -> Result<Vec<Template>, TemplateError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir).map_err(|e| TemplateError::IoFailed {
        details: format!("could not read {}: {e}", dir.display()),
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| TemplateError::IoFailed {
            details: e.to_string(),
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue; // ignore stray non-.json files (e.g. a leftover .tmp)
        }
        out.push(read_template(&path)?);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Removes a custom template's file from `dir`. Errors with
/// `TemplateError::UnknownTemplate` if no such file exists — the caller
/// (`commands::templates::delete_custom_template`) is responsible for
/// separately rejecting an attempt to delete a *built-in* template's id
/// before ever reaching here.
pub fn delete_custom_template(dir: &Path, template_id: &str) -> Result<(), TemplateError> {
    let path = template_file_path(dir, template_id)?;
    if !path.exists() {
        return Err(TemplateError::UnknownTemplate {
            template_id: template_id.to_string(),
        });
    }
    fs::remove_file(&path).map_err(|e| TemplateError::IoFailed {
        details: format!("could not remove {}: {e}", path.display()),
    })
}

/// Export Template (§36): write `template` to an arbitrary caller-chosen
/// file path (a user-picked export location, not necessarily inside
/// `templates/`) — same atomic-write discipline as everything else here.
pub fn export_template_to_path(template: &Template, path: &Path) -> Result<(), TemplateError> {
    write_atomic(path, template)
}

// -- Upgrade spec §20: version history --------------------------------------
//
// A single `<id>.history.json` file per custom template, holding every
// *previous* full `Template` snapshot as a JSON array (the current version
// itself always lives at `<id>.json`, never duplicated into its own
// history). Chosen over a `templates_dir/<id>/v<N>.json` subdirectory
// scheme for the same reason `templates_dir` itself is one flat directory
// of `<id>.json` files rather than nested per-template folders: fewer
// directories to create/list/clean up, and a template's entire past is one
// read away rather than a directory listing plus N reads. A custom
// template that has never been updated has no history file at all — same
// "absence means empty" convention `list_custom_templates` already uses for
// a missing `templates_dir` itself.

fn history_file_path(dir: &Path, template_id: &str) -> Result<PathBuf, TemplateError> {
    if !crate::fs_safety::is_safe_path_component(template_id) {
        return Err(TemplateError::UnsafeTemplateId {
            template_id: template_id.to_string(),
        });
    }
    Ok(dir.join(format!("{template_id}.history.json")))
}

fn write_history_atomic(path: &Path, history: &[Template]) -> Result<(), TemplateError> {
    let json = serde_json::to_vec_pretty(history).map_err(|e| TemplateError::IoFailed {
        details: format!("serialize history failed: {e}"),
    })?;
    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp_path).map_err(|e| TemplateError::IoFailed {
            details: format!("could not create {}: {e}", tmp_path.display()),
        })?;
        file.write_all(&json).map_err(|e| TemplateError::IoFailed {
            details: format!("write failed: {e}"),
        })?;
        file.sync_all().map_err(|e| TemplateError::IoFailed {
            details: format!("fsync failed: {e}"),
        })?;
    }
    fs::rename(&tmp_path, path).map_err(|e| TemplateError::IoFailed {
        details: format!(
            "rename {} -> {} failed: {e}",
            tmp_path.display(),
            path.display()
        ),
    })
}

/// Every previous version of `template_id` recorded so far, oldest first.
/// An empty `Vec`, not an error, if no history file exists yet (a custom
/// template that's never been updated, or a built-in — which never has
/// one).
pub fn list_template_history(
    dir: &Path,
    template_id: &str,
) -> Result<Vec<Template>, TemplateError> {
    let path = history_file_path(dir, template_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|e| TemplateError::IoFailed {
        details: format!("could not read {}: {e}", path.display()),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| TemplateError::CorruptJson {
        details: format!("{}: {e}", path.display()),
    })
}

/// Appends `previous` (the full pre-update snapshot) onto `template_id`'s
/// history file, creating it if this is the first update ever recorded for
/// this template. Called by `commands::templates::update_custom_template`
/// *before* the new version overwrites `<id>.json`, so the old content is
/// never lost even for a single instant.
pub fn append_template_history(dir: &Path, previous: &Template) -> Result<(), TemplateError> {
    fs::create_dir_all(dir).map_err(|e| TemplateError::IoFailed {
        details: format!("could not create {}: {e}", dir.display()),
    })?;
    let path = history_file_path(dir, &previous.id)?;
    let mut history = list_template_history(dir, &previous.id)?;
    history.push(previous.clone());
    write_history_atomic(&path, &history)
}

/// Import Template (§36): read a `Template` back from an arbitrary
/// caller-chosen file path.
pub fn import_template_from_path(path: &Path) -> Result<Template, TemplateError> {
    read_template(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::templates::all_templates;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-templates-io-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_list_then_delete_custom_template_round_trips() {
        let dir = temp_dir("save-list-delete");
        let mut template = all_templates().remove(2); // tiktok
        template.id = format!("custom_{}", uuid::Uuid::new_v4());
        template.is_built_in = false;
        template.name = "My Custom Template".to_string();

        let path = save_custom_template(&dir, &template).expect("save");
        assert!(path.exists());
        assert!(
            !path.with_extension("json.tmp").exists(),
            "no leftover .tmp"
        );

        let listed = list_custom_templates(&dir).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, template.id);
        assert_eq!(listed[0].name, "My Custom Template");

        delete_custom_template(&dir, &template.id).expect("delete");
        let listed_after = list_custom_templates(&dir).expect("list after delete");
        assert!(listed_after.is_empty());
    }

    #[test]
    fn list_custom_templates_on_a_missing_directory_is_an_empty_vec_not_an_error() {
        let dir =
            std::env::temp_dir().join(format!("ave-templates-io-missing-{}", uuid::Uuid::new_v4()));
        assert!(!dir.exists());
        let listed = list_custom_templates(&dir).expect("list on missing dir");
        assert!(listed.is_empty());
    }

    #[test]
    fn deleting_an_unknown_template_id_errors() {
        let dir = temp_dir("delete-unknown");
        let err = delete_custom_template(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, TemplateError::UnknownTemplate { .. }));
    }

    #[test]
    fn export_then_import_round_trips_exactly() {
        let dir = temp_dir("export-import");
        let template = all_templates()
            .into_iter()
            .find(|t| t.id == "tmpl_football_highlight")
            .unwrap();
        let export_path = dir.join("football_highlight_export.json");

        export_template_to_path(&template, &export_path).expect("export");
        assert!(export_path.exists());
        assert!(
            !export_path.with_extension("json.tmp").exists(),
            "no leftover .tmp after export"
        );

        let imported = import_template_from_path(&export_path).expect("import");
        assert_eq!(imported, template);
    }

    /// §88 Windows path edge case: an export/import path containing spaces
    /// and real Vietnamese/other Unicode.
    #[test]
    fn export_then_import_round_trips_under_a_unicode_and_space_path() {
        let dir = temp_dir("export-import-unicode");
        let unicode_dir = dir.join("Nguyễn Văn A's Templates 🎬");
        fs::create_dir_all(&unicode_dir).unwrap();
        let template = all_templates()
            .into_iter()
            .find(|t| t.id == "tmpl_football_highlight")
            .unwrap();
        let export_path = unicode_dir.join("Mẫu Bóng Đá Xuất Sắc.json");

        export_template_to_path(&template, &export_path)
            .expect("export succeeds under a Unicode/space-containing path");
        assert!(export_path.exists());

        let imported = import_template_from_path(&export_path).expect("import");
        assert_eq!(imported, template);
    }

    #[test]
    fn importing_corrupt_json_errors() {
        let dir = temp_dir("corrupt");
        let path = dir.join("corrupt.json");
        fs::write(&path, b"not json").unwrap();
        let err = import_template_from_path(&path).unwrap_err();
        assert!(matches!(err, TemplateError::CorruptJson { .. }));
    }

    // -- §53 path traversal prevention: a crafted/corrupted imported
    //    template's `id` must never escape the `templates/` directory ------

    #[test]
    fn saving_a_template_with_a_path_traversal_id_is_rejected_not_written_outside_dir() {
        // Simulates `commands::templates::import_template`'s real risk: an
        // imported template's `id` comes straight from whatever JSON file
        // the user chose to import, not from this backend's own
        // `custom_<uuid>` generator.
        let dir = temp_dir("traversal-save");
        let marker_name = format!("ave-templates-io-escaped-{}", uuid::Uuid::new_v4());
        // The exact path a naive (unvalidated) `dir.join(format!("{id}.json"))`
        // would have resolved to — a uniquely-named sibling of `dir`, so
        // checking it specifically is safe under parallel test execution
        // (unaffected by any other test's own temp directories).
        let would_be_outside_path = dir.parent().unwrap().join(format!("{marker_name}.json"));

        let mut template = all_templates().remove(2);
        template.id = format!("../{marker_name}");

        let err = save_custom_template(&dir, &template).unwrap_err();
        assert!(matches!(err, TemplateError::UnsafeTemplateId { .. }));
        assert!(
            !would_be_outside_path.exists(),
            "a traversal id must never write outside the templates directory"
        );
    }

    #[test]
    fn deleting_a_path_traversal_id_is_rejected_without_touching_the_filesystem() {
        let dir = temp_dir("traversal-delete");
        let err = delete_custom_template(&dir, "../../../etc/passwd").unwrap_err();
        assert!(matches!(err, TemplateError::UnsafeTemplateId { .. }));
    }

    #[test]
    fn deleting_a_windows_style_traversal_id_is_rejected() {
        let dir = temp_dir("traversal-delete-backslash");
        let err = delete_custom_template(&dir, "..\\..\\Users\\victim\\secret").unwrap_err();
        assert!(matches!(err, TemplateError::UnsafeTemplateId { .. }));
    }

    // -- version history (upgrade spec §20) ----------------------------------

    #[test]
    fn a_template_that_has_never_been_updated_has_no_history() {
        let dir = temp_dir("history-none-yet");
        let history = list_template_history(&dir, "custom_never_updated").expect("list history");
        assert!(history.is_empty());
    }

    #[test]
    fn appending_history_then_listing_it_round_trips() {
        let dir = temp_dir("history-round-trip");
        let mut v1 = all_templates().remove(2); // tiktok
        v1.id = "custom_history_test".to_string();
        v1.is_built_in = false;
        v1.version = 1;
        v1.name = "v1 name".to_string();

        append_template_history(&dir, &v1).expect("append v1 to history");

        let mut v2 = v1.clone();
        v2.version = 2;
        v2.name = "v2 name".to_string();
        append_template_history(&dir, &v2).expect("append v2 to history");

        let history = list_template_history(&dir, "custom_history_test").expect("list history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version, 1);
        assert_eq!(history[0].name, "v1 name");
        assert_eq!(history[1].version, 2);
        assert_eq!(history[1].name, "v2 name");
    }

    #[test]
    fn history_is_independent_per_template_id() {
        let dir = temp_dir("history-independent");
        let mut a = all_templates().remove(0);
        a.id = "custom_a".to_string();
        a.is_built_in = false;
        append_template_history(&dir, &a).expect("append a");

        let history_b = list_template_history(&dir, "custom_b").expect("list history for b");
        assert!(
            history_b.is_empty(),
            "template b's history must be unaffected by template a's"
        );
    }
}
