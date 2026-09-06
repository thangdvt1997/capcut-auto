//! Asset Library Tauri command surface (upgrade spec §17). Thin per master
//! prompt §66 — all real logic lives in `crate::assets::{self, io}`.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::assets::{self, io as asset_io, Asset, AssetError, AssetKind};
use crate::error::AppErrorPayload;

/// Asset Library storage location: `$APPLOCALDATA/assets/` — the exact same
/// this-app's-own-data-directory convention `commands::templates::templates_dir`
/// uses for custom templates.
///
/// `pub(crate)`, not private: `commands::templates` reuses this exact
/// resolution logic to validate a template's `intro`/`outro`/`watermark`/
/// `background_music` asset-id references against the same on-disk catalog
/// this module's own commands read/write, rather than duplicating it.
pub(crate) fn assets_dir(app: &AppHandle) -> Result<PathBuf, AssetError> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("assets"))
        .map_err(|e| AssetError::StorageUnavailable {
            details: format!("resolving app local data dir: {e}"),
        })
}

/// Lists every registered asset, optionally filtered to one `AssetKind`.
#[tauri::command]
#[specta::specta]
pub fn list_assets(app: AppHandle, kind: Option<AssetKind>) -> Result<Vec<Asset>, AppErrorPayload> {
    let dir = assets_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    asset_io::list_assets(&dir, kind).map_err(|e| AppErrorPayload::from(&e))
}

/// Registers a new asset. Validates `file_path` really exists on disk
/// before ever persisting the reference (`assets::new_asset`).
#[tauri::command]
#[specta::specta]
pub fn add_asset(
    app: AppHandle,
    kind: AssetKind,
    name: String,
    file_path: String,
) -> Result<Asset, AppErrorPayload> {
    let asset = assets::new_asset(kind, name, file_path).map_err(|e| AppErrorPayload::from(&e))?;
    let dir = assets_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    asset_io::save_asset(&dir, &asset).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(asset)
}

/// Removes an asset from the library. Does NOT check whether any saved
/// `Template` still references this id — a `Template`'s own
/// `intro`/`outro`/`watermark`/`background_music` reference is only
/// re-validated the next time that template is saved/updated
/// (`templates::validate_asset_references`), matching how a caption
/// style/export preset id can already go stale between a template's save
/// and a later re-save.
#[tauri::command]
#[specta::specta]
pub fn remove_asset(app: AppHandle, asset_id: String) -> Result<(), AppErrorPayload> {
    let dir = assets_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    asset_io::delete_asset(&dir, &asset_id).map_err(|e| AppErrorPayload::from(&e))
}

/// Updates an existing asset's `name`/`file_path`/`tags` in place (`None` =
/// leave that field unchanged). A new `file_path` is re-validated the same
/// way `add_asset` validates one — never silently accepted.
#[tauri::command]
#[specta::specta]
pub fn update_asset(
    app: AppHandle,
    asset_id: String,
    name: Option<String>,
    file_path: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<Asset, AppErrorPayload> {
    let dir = assets_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let mut asset = asset_io::load_asset(&dir, &asset_id).map_err(|e| AppErrorPayload::from(&e))?;

    if let Some(name) = name {
        asset.name = name;
    }
    if let Some(file_path) = file_path {
        if !std::path::Path::new(&file_path).is_file() {
            return Err(AppErrorPayload::from(&AssetError::FileNotFound {
                file_path,
            }));
        }
        asset.file_path = file_path;
    }
    if let Some(tags) = tags {
        asset.tags = tags;
    }

    asset_io::save_asset(&dir, &asset).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(asset)
}
