//! Templates Tauri command surface (master prompt §36/§37). Thin per master
//! prompt §66 — all real logic lives in `crate::templates::{self, io}`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::assets::io as asset_io;
use crate::error::AppErrorPayload;
use crate::project::ProjectV1;
use crate::templates::{self, io as template_io, SaveAsTemplateInput, Template, TemplateError};

/// The set of asset ids currently registered in the Asset Library — read
/// fresh on every save/update so a template's `intro`/`outro`/`watermark`/
/// `background_music` reference is checked against what's *actually* there
/// right now (upgrade spec §17), not a stale snapshot.
fn known_asset_ids(app: &AppHandle) -> Result<HashSet<String>, AppErrorPayload> {
    let dir = crate::commands::assets::assets_dir(app).map_err(|e| AppErrorPayload::from(&e))?;
    let assets = asset_io::list_assets(&dir, None).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(assets.into_iter().map(|a| a.id).collect())
}

/// Custom-template storage location (master prompt §36's literal "Directory:
/// templates/" instruction): `$APPLOCALDATA/templates/` — the same
/// this-app's-own-data-directory convention
/// `commands::transcription::models_dir` uses for downloaded Whisper models.
///
/// `pub(crate)`, not private: `commands::batch` reuses this exact
/// resolution logic to resolve a batch's `template_id` (built-in or
/// custom) against the same on-disk catalog `list_templates`/`export_template`
/// already read from, rather than duplicating it.
pub(crate) fn templates_dir(app: &AppHandle) -> Result<PathBuf, TemplateError> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("templates"))
        .map_err(|e| TemplateError::StorageUnavailable {
            details: format!("resolving app local data dir: {e}"),
        })
}

/// The combined template catalog a "browse templates" UI needs: the 8
/// built-ins (`templates::all_templates`, always present, never edited on
/// disk) plus every custom template saved under `templates_dir`.
#[derive(Debug, Clone, Serialize, Type)]
pub struct TemplateCatalog {
    pub built_in: Vec<Template>,
    pub custom: Vec<Template>,
}

#[tauri::command]
#[specta::specta]
pub fn list_templates(app: AppHandle) -> Result<TemplateCatalog, AppErrorPayload> {
    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let custom = template_io::list_custom_templates(&dir).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(TemplateCatalog {
        built_in: templates::all_templates(),
        custom,
    })
}

/// Save as Template (master prompt §36): snapshots `project`'s canvas plus
/// the caller's current settings bundle (`input`) into a new custom
/// `Template`, persisted under `templates_dir`.
#[tauri::command]
#[specta::specta]
pub fn save_as_template(
    app: AppHandle,
    project: ProjectV1,
    input: SaveAsTemplateInput,
) -> Result<Template, AppErrorPayload> {
    let known_ids = known_asset_ids(&app)?;
    let template = templates::save_as_template_from_project(&project, input, &known_ids)
        .map_err(|e| AppErrorPayload::from(&e))?;
    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    template_io::save_custom_template(&dir, &template).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(template)
}

/// Update an existing custom template in place (upgrade spec §20): bumps
/// `version`, and preserves the pre-update content in that template's
/// version-history file (`template_io::append_template_history`) *before*
/// overwriting `<id>.json`, so a job that recorded the old
/// `template_id`+`template_version` can still resolve exactly what it ran
/// with via [`get_template_version`]. Refuses to edit a built-in template
/// (`TemplateError::CannotEditBuiltIn`), same guard `delete_custom_template`
/// already applies for deletion.
#[tauri::command]
#[specta::specta]
pub fn update_custom_template(
    app: AppHandle,
    template_id: String,
    project: ProjectV1,
    input: SaveAsTemplateInput,
) -> Result<Template, AppErrorPayload> {
    if templates::all_templates()
        .iter()
        .any(|t| t.id == template_id)
    {
        return Err(AppErrorPayload::from(&TemplateError::CannotEditBuiltIn {
            template_id,
        }));
    }

    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let existing = template_io::list_custom_templates(&dir)
        .map_err(|e| AppErrorPayload::from(&e))?
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| TemplateError::UnknownTemplate {
            template_id: template_id.clone(),
        })
        .map_err(|e| AppErrorPayload::from(&e))?;

    let known_ids = known_asset_ids(&app)?;
    let updated = templates::update_custom_template(&existing, &project, input, &known_ids)
        .map_err(|e| AppErrorPayload::from(&e))?;

    // Preserve the pre-update snapshot before overwriting the on-disk file.
    template_io::append_template_history(&dir, &existing).map_err(|e| AppErrorPayload::from(&e))?;
    template_io::save_custom_template(&dir, &updated).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(updated)
}

/// Resolves the exact `Template` content for `template_id`+`version`
/// (upgrade spec §20 — a batch job pins both so it stays reproducible even
/// after the template is edited further). Checks, in order: the built-in
/// catalog (always exactly version 1); the current on-disk custom template
/// (if its `version` matches); then that template's version-history file.
#[tauri::command]
#[specta::specta]
pub fn get_template_version(
    app: AppHandle,
    template_id: String,
    version: u32,
) -> Result<Template, AppErrorPayload> {
    if let Some(built_in) = templates::all_templates()
        .into_iter()
        .find(|t| t.id == template_id)
    {
        return if built_in.version == version {
            Ok(built_in)
        } else {
            Err(AppErrorPayload::from(
                &TemplateError::UnknownTemplateVersion {
                    template_id,
                    version,
                },
            ))
        };
    }

    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let current = template_io::list_custom_templates(&dir)
        .map_err(|e| AppErrorPayload::from(&e))?
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| TemplateError::UnknownTemplate {
            template_id: template_id.clone(),
        })
        .map_err(|e| AppErrorPayload::from(&e))?;

    if current.version == version {
        return Ok(current);
    }

    template_io::list_template_history(&dir, &template_id)
        .map_err(|e| AppErrorPayload::from(&e))?
        .into_iter()
        .find(|t| t.version == version)
        .ok_or_else(|| {
            AppErrorPayload::from(&TemplateError::UnknownTemplateVersion {
                template_id,
                version,
            })
        })
}

/// Import Template (master prompt §36): reads a `Template` from
/// `file_path` and copies it into this machine's `templates_dir` as a new
/// custom template (never as a built-in, even if it was originally exported
/// from one of the 8 built-ins — an imported copy is always independently
/// editable/deletable, rather than colliding with or being confused for the
/// read-only built-in of the same original id).
#[tauri::command]
#[specta::specta]
pub fn import_template(app: AppHandle, file_path: String) -> Result<Template, AppErrorPayload> {
    let mut template = template_io::import_template_from_path(Path::new(&file_path))
        .map_err(|e| AppErrorPayload::from(&e))?;
    template.is_built_in = false;
    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    template_io::save_custom_template(&dir, &template).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(template)
}

/// Export Template (master prompt §36): writes the built-in or custom
/// template identified by `template_id` to `file_path`.
#[tauri::command]
#[specta::specta]
pub fn export_template(
    app: AppHandle,
    template_id: String,
    file_path: String,
) -> Result<(), AppErrorPayload> {
    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let custom = template_io::list_custom_templates(&dir).map_err(|e| AppErrorPayload::from(&e))?;
    let template = templates::all_templates()
        .into_iter()
        .chain(custom)
        .find(|t| t.id == template_id)
        .ok_or_else(|| TemplateError::UnknownTemplate {
            template_id: template_id.clone(),
        })
        .map_err(|e| AppErrorPayload::from(&e))?;
    template_io::export_template_to_path(&template, Path::new(&file_path))
        .map_err(|e| AppErrorPayload::from(&e))
}

/// Deletes a custom template. Refuses to delete any of the 8 built-in ids
/// (`TemplateError::CannotDeleteBuiltIn`) before ever touching disk.
#[tauri::command]
#[specta::specta]
pub fn delete_custom_template(app: AppHandle, template_id: String) -> Result<(), AppErrorPayload> {
    if templates::all_templates()
        .iter()
        .any(|t| t.id == template_id)
    {
        return Err(AppErrorPayload::from(&TemplateError::CannotDeleteBuiltIn {
            template_id,
        }));
    }
    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    template_io::delete_custom_template(&dir, &template_id).map_err(|e| AppErrorPayload::from(&e))
}
