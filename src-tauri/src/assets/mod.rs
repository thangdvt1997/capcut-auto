//! Asset Library (upgrade spec §17): a small, curated catalog of reusable
//! external files — intro/outro clips, logo/watermark images, background
//! music, etc. — each given a stable id so a `Template` (see
//! `templates::mod`'s `intro`/`outro`/`watermark`/`background_music` fields)
//! can reference an asset by id instead of hard-coding a path (§17's own
//! "template reference asset bằng ID thay vì hard-code path" requirement).
//!
//! ## Storage: JSON-file-per-asset, not SQLite
//!
//! Unlike `db::MediaLibrary` (a large, searchable SQLite index of every
//! video/audio/image a user has ever imported, with substring search and
//! pagination), this catalog is small and curated — a handful of "the
//! logo", "the outro", "my background music track" entries a user
//! deliberately registers one at a time, never a bulk import pipeline.
//! `templates::io`'s simpler JSON-file-per-item convention
//! (`assets_dir/<id>.json`, atomic write-tmp-then-rename) is the better fit
//! and is followed exactly — see `io` module doc comment.
//!
//! ## File reference, not copy
//!
//! Same convention `project::MediaItem::source_path`/`Template`'s own
//! catalog already use: `Asset::file_path` stores an absolute path to the
//! real file wherever it already lives on disk. Nothing here copies the
//! file into app-data — copying would double disk usage for typically large
//! media files (intro/outro clips, music tracks) for no real benefit, and
//! every other "reference an external file" convention in this codebase
//! already works this way.
//!
//! ## Which `AssetKind`s are really consumed today vs. structural-only
//!
//! - **Real, wired to a consuming feature**: `Intro`/`Outro` (referenced
//!   from `Template::intro`/`outro`, resolvable to a real video file usable
//!   as a `project::MediaItem`); `Logo`/`Watermark` (an image file,
//!   referenced from `Template::watermark`); `Music` (an audio file,
//!   referenced from `Template::background_music` — could feed
//!   `project::AudioClipSettings`/`AudioRole::Music` per the original Phase
//!   11 audio-features work once a caller builds a project from a
//!   template).
//! - **Structural-only** (§17 lists these kinds, but no consuming feature
//!   exists yet anywhere in this codebase — same honest "documented gap"
//!   treatment `templates::mod`'s own `TransitionSettings`/
//!   `SportsOverlaySettings` doc comments already use for comparable gaps):
//!   `SoundEffect`, `Overlay`, `Font`, `SubtitleStyle`, `TransitionPreset`,
//!   `Background`. In particular `SubtitleStyle` is kept as a bare
//!   `file_path` reference here rather than special-cased to reference an
//!   existing `project::CaptionStyle`/`captions::styles` catalog id — a
//!   `Template`'s caption styling is already fully real via
//!   `Template::caption_style` (a whole owned `CaptionStyle`, not an asset
//!   reference), so there is no real consumer this kind would plug into
//!   today; a `SubtitleStyle` asset registered here is bookkeeping only,
//!   until some future feature (e.g. "install a caption style shared as a
//!   file") gives it one.
//!
//! This catalog deliberately starts empty — no built-in seed assets — since
//! unlike `captions::styles`/`render::presets` (which start with real,
//! meaningful built-in defaults), there is nothing sensible to bundle here:
//! an intro/logo/music file is inherently a specific user's own asset, and
//! §17 explicitly frames this as the user registering *their own* files.

use serde::{Deserialize, Serialize};
use specta::Type;

pub mod error;
pub mod io;

pub use error::AssetError;

/// Upgrade spec §17's exact catalog (closed enum, in the order §17 lists
/// it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Intro,
    Outro,
    Logo,
    Watermark,
    Music,
    SoundEffect,
    Overlay,
    Font,
    SubtitleStyle,
    TransitionPreset,
    Background,
}

/// One Asset Library entry (module doc comment). `file_path` is an absolute
/// path to the real underlying file — referenced, never copied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Asset {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    pub file_path: String,
    /// RFC3339, when this asset was registered.
    pub created_at: String,
    /// Optional, lightly mirroring `db::MediaLibraryEntry::tags`'s own
    /// convention — free-form labels a user can filter/search by later.
    pub tags: Vec<String>,
}

/// Builds a new `Asset` with a fresh `asset_<uuid>` id, after validating
/// that `file_path` really exists as a file on disk (§17 never says "trust
/// any string" — a caller handing in a typo'd or already-moved path would
/// otherwise only discover the break much later, at template-apply/export
/// time). Never inspects the file's *content* — an Intro asset pointed at a
/// `.png` is a user error this module doesn't try to catch, same
/// "structural, not content-validating" honesty the rest of this catalog
/// uses.
pub fn new_asset(kind: AssetKind, name: String, file_path: String) -> Result<Asset, AssetError> {
    if !std::path::Path::new(&file_path).is_file() {
        return Err(AssetError::FileNotFound { file_path });
    }
    Ok(Asset {
        id: format!("asset_{}", uuid::Uuid::new_v4()),
        kind,
        name,
        file_path,
        created_at: crate::project::now_rfc3339(),
        tags: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(label: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("ave-assets-test-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"fake bytes").unwrap();
        path
    }

    #[test]
    fn new_asset_succeeds_for_a_real_existing_file() {
        let path = temp_file("real");
        let asset = new_asset(
            AssetKind::Intro,
            "My Intro".to_string(),
            path.to_string_lossy().to_string(),
        )
        .expect("new_asset");
        assert!(asset.id.starts_with("asset_"));
        assert_eq!(asset.kind, AssetKind::Intro);
        assert_eq!(asset.name, "My Intro");
        assert!(asset.tags.is_empty());
        assert!(!asset.created_at.is_empty());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn new_asset_rejects_a_nonexistent_path() {
        let path = std::env::temp_dir().join(format!(
            "ave-assets-test-does-not-exist-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(!path.exists());
        let err = new_asset(
            AssetKind::Music,
            "Missing".to_string(),
            path.to_string_lossy().to_string(),
        )
        .unwrap_err();
        assert!(matches!(err, AssetError::FileNotFound { .. }));
    }

    #[test]
    fn new_asset_rejects_a_directory_not_just_any_existing_path() {
        // A directory exists but is not a *file* — `add_asset` is meant to
        // register a single file reference, not a folder.
        let dir =
            std::env::temp_dir().join(format!("ave-assets-test-dir-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = new_asset(
            AssetKind::Overlay,
            "A Directory".to_string(),
            dir.to_string_lossy().to_string(),
        )
        .unwrap_err();
        assert!(matches!(err, AssetError::FileNotFound { .. }));
    }
}
