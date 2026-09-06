//! On-disk storage for the Asset Library (upgrade spec §17), under this
//! app's own `assets/` app-data directory — the exact same convention
//! `templates::io` already uses for `templates/` (JSON-file-per-item, atomic
//! temp-file-then-rename writes, see `assets::mod`'s own module doc comment
//! for why this simpler convention is the right fit here rather than a
//! `db::MediaLibrary`-style SQLite table).

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::error::AssetError;
use super::{Asset, AssetKind};

/// One asset's on-disk filename inside the `assets/` directory: `<id>.json`.
/// Rejects an `asset_id` that isn't a safe single path component (master
/// prompt §53 path traversal prevention) before ever joining it onto `dir` —
/// same defense-in-depth rationale as `templates::io::template_file_path`.
fn asset_file_path(dir: &Path, asset_id: &str) -> Result<PathBuf, AssetError> {
    if !crate::fs_safety::is_safe_path_component(asset_id) {
        return Err(AssetError::UnsafeAssetId {
            asset_id: asset_id.to_string(),
        });
    }
    Ok(dir.join(format!("{asset_id}.json")))
}

/// Serialize -> write to `<path>.tmp` -> fsync -> rename over `path`. Same
/// atomic-write discipline as `templates::io::write_atomic`/
/// `project::io::save_atomic`.
fn write_atomic(path: &Path, asset: &Asset) -> Result<(), AssetError> {
    let json = serde_json::to_vec_pretty(asset).map_err(|e| AssetError::IoFailed {
        details: format!("serialize failed: {e}"),
    })?;

    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp_path).map_err(|e| AssetError::IoFailed {
            details: format!("could not create {}: {e}", tmp_path.display()),
        })?;
        file.write_all(&json).map_err(|e| AssetError::IoFailed {
            details: format!("write failed: {e}"),
        })?;
        file.sync_all().map_err(|e| AssetError::IoFailed {
            details: format!("fsync failed: {e}"),
        })?;
    }

    fs::rename(&tmp_path, path).map_err(|e| AssetError::IoFailed {
        details: format!(
            "rename {} -> {} failed: {e}",
            tmp_path.display(),
            path.display()
        ),
    })
}

fn read_asset(path: &Path) -> Result<Asset, AssetError> {
    let bytes = fs::read(path).map_err(|e| AssetError::IoFailed {
        details: format!("could not read {}: {e}", path.display()),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| AssetError::CorruptJson {
        details: format!("{}: {e}", path.display()),
    })
}

/// Atomically writes `asset` into the `assets/` directory as `<id>.json`,
/// creating the directory if it doesn't exist yet — used for both a
/// brand-new `add_asset` and an in-place `update_asset` (same file, new
/// content, same discipline every other overwrite in this codebase already
/// follows).
pub fn save_asset(dir: &Path, asset: &Asset) -> Result<PathBuf, AssetError> {
    fs::create_dir_all(dir).map_err(|e| AssetError::IoFailed {
        details: format!("could not create {}: {e}", dir.display()),
    })?;
    let path = asset_file_path(dir, &asset.id)?;
    write_atomic(&path, asset)?;
    Ok(path)
}

/// Loads a single asset by id. Errors with `AssetError::UnknownAsset` if no
/// such file exists.
pub fn load_asset(dir: &Path, asset_id: &str) -> Result<Asset, AssetError> {
    let path = asset_file_path(dir, asset_id)?;
    if !path.exists() {
        return Err(AssetError::UnknownAsset {
            asset_id: asset_id.to_string(),
        });
    }
    read_asset(&path)
}

/// Lists every asset saved under `dir`, optionally filtered to one
/// `AssetKind` (`db::search_media`'s own optional-kind-filter convention).
/// An empty `Vec`, not an error, if the directory doesn't exist yet — same
/// "absence means empty" convention `templates::io::list_custom_templates`
/// uses. Sorted by name for a stable listing order.
pub fn list_assets(dir: &Path, kind: Option<AssetKind>) -> Result<Vec<Asset>, AssetError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir).map_err(|e| AssetError::IoFailed {
        details: format!("could not read {}: {e}", dir.display()),
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| AssetError::IoFailed {
            details: e.to_string(),
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue; // ignore stray non-.json files (e.g. a leftover .tmp)
        }
        let asset = read_asset(&path)?;
        if kind.map(|k| asset.kind == k).unwrap_or(true) {
            out.push(asset);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Removes an asset's file from `dir`. Errors with `AssetError::UnknownAsset`
/// if no such file exists.
pub fn delete_asset(dir: &Path, asset_id: &str) -> Result<(), AssetError> {
    let path = asset_file_path(dir, asset_id)?;
    if !path.exists() {
        return Err(AssetError::UnknownAsset {
            asset_id: asset_id.to_string(),
        });
    }
    fs::remove_file(&path).map_err(|e| AssetError::IoFailed {
        details: format!("could not remove {}: {e}", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::new_asset;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-assets-io-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn temp_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"fake media bytes").unwrap();
        path
    }

    #[test]
    fn save_then_list_then_delete_asset_round_trips() {
        let dir = temp_dir("save-list-delete");
        let real_file = temp_file(&dir, "intro.mp4");
        let asset = new_asset(
            AssetKind::Intro,
            "My Intro".to_string(),
            real_file.to_string_lossy().to_string(),
        )
        .expect("new_asset");

        let assets_dir = dir.join("assets");
        let path = save_asset(&assets_dir, &asset).expect("save");
        assert!(path.exists());
        assert!(
            !path.with_extension("json.tmp").exists(),
            "no leftover .tmp"
        );

        let listed = list_assets(&assets_dir, None).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, asset.id);
        assert_eq!(listed[0].name, "My Intro");

        delete_asset(&assets_dir, &asset.id).expect("delete");
        let listed_after = list_assets(&assets_dir, None).expect("list after delete");
        assert!(listed_after.is_empty());
    }

    #[test]
    fn list_assets_filters_by_kind() {
        let dir = temp_dir("kind-filter");
        let intro_file = temp_file(&dir, "intro.mp4");
        let music_file = temp_file(&dir, "music.mp3");
        let assets_dir = dir.join("assets");

        let intro = new_asset(
            AssetKind::Intro,
            "Intro".to_string(),
            intro_file.to_string_lossy().to_string(),
        )
        .unwrap();
        let music = new_asset(
            AssetKind::Music,
            "Music".to_string(),
            music_file.to_string_lossy().to_string(),
        )
        .unwrap();
        save_asset(&assets_dir, &intro).unwrap();
        save_asset(&assets_dir, &music).unwrap();

        let all = list_assets(&assets_dir, None).unwrap();
        assert_eq!(all.len(), 2);

        let only_music = list_assets(&assets_dir, Some(AssetKind::Music)).unwrap();
        assert_eq!(only_music.len(), 1);
        assert_eq!(only_music[0].id, music.id);
    }

    #[test]
    fn list_assets_on_a_missing_directory_is_an_empty_vec_not_an_error() {
        let dir =
            std::env::temp_dir().join(format!("ave-assets-io-missing-{}", uuid::Uuid::new_v4()));
        assert!(!dir.exists());
        let listed = list_assets(&dir, None).expect("list on missing dir");
        assert!(listed.is_empty());
    }

    #[test]
    fn deleting_an_unknown_asset_id_errors() {
        let dir = temp_dir("delete-unknown");
        let err = delete_asset(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, AssetError::UnknownAsset { .. }));
    }

    #[test]
    fn loading_an_unknown_asset_id_errors() {
        let dir = temp_dir("load-unknown");
        let err = load_asset(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, AssetError::UnknownAsset { .. }));
    }

    #[test]
    fn saving_an_asset_with_a_path_traversal_id_is_rejected_not_written_outside_dir() {
        let dir = temp_dir("traversal-save");
        let marker_name = format!("ave-assets-io-escaped-{}", uuid::Uuid::new_v4());
        let would_be_outside_path = dir.parent().unwrap().join(format!("{marker_name}.json"));

        let real_file = temp_file(&dir, "intro.mp4");
        let mut asset = new_asset(
            AssetKind::Intro,
            "Intro".to_string(),
            real_file.to_string_lossy().to_string(),
        )
        .unwrap();
        asset.id = format!("../{marker_name}");

        let err = save_asset(&dir, &asset).unwrap_err();
        assert!(matches!(err, AssetError::UnsafeAssetId { .. }));
        assert!(
            !would_be_outside_path.exists(),
            "a traversal id must never write outside the assets directory"
        );
    }

    #[test]
    fn deleting_a_path_traversal_id_is_rejected_without_touching_the_filesystem() {
        let dir = temp_dir("traversal-delete");
        let err = delete_asset(&dir, "../../../etc/passwd").unwrap_err();
        assert!(matches!(err, AssetError::UnsafeAssetId { .. }));
    }

    #[test]
    fn importing_corrupt_json_errors() {
        let dir = temp_dir("corrupt");
        fs::write(dir.join("broken.json"), b"not json").unwrap();
        let err = load_asset(&dir, "broken").unwrap_err();
        assert!(matches!(err, AssetError::CorruptJson { .. }));
    }
}
