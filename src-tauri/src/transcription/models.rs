//! Model Manager catalog (master prompt §14/§60): the 5 standard
//! whisper.cpp `ggml` model sizes, with real metadata — file size confirmed
//! by an actual `HEAD`/range request against the download URL (not
//! guessed — see `docs/upstream.md`-style verification note below), and the
//! well-known download source, Hugging Face's `ggerganov/whisper.cpp` repo.
//!
//! ## Multilingual-only, by design
//!
//! Hugging Face also publishes `.en`-suffixed English-only variants
//! (`ggml-tiny.en.bin`, etc.) for every size except `large`. This catalog
//! offers only the multilingual variant of each size, so it matches master
//! prompt §14's own list exactly — five ids, `tiny`/`base`/`small`/`medium`/
//! `large`, no doubled catalog with a `.en` sibling for each. Multilingual
//! models transcribe English content just fine (marginally more compute per
//! token than the `.en` variant, not a correctness difference); offering
//! English-only variants as an additional, clearly-labeled catalog option
//! is a reasonable future enhancement if a user specifically wants the
//! smaller/faster `.en` weights, not implemented in this pass.
//!
//! ## URL verified, not assumed
//!
//! `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/<filename>`
//! was checked with a real `curl -IL` HEAD request (following the redirect
//! Hugging Face's CDN issues) for every filename in this catalog before
//! being hardcoded here — each returned HTTP 200 with a `Content-Length`
//! matching `approx_size_bytes` below (verified 2026-09-05). See
//! `download::tests` for an automated real-network smoke test of this exact
//! pattern.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::error::ModelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelId {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl ModelId {
    pub const ALL: [ModelId; 5] = [
        ModelId::Tiny,
        ModelId::Base,
        ModelId::Small,
        ModelId::Medium,
        ModelId::Large,
    ];

    /// Stable string id, used for command parameters / cache keys / the
    /// `.part`/final filename stem — deliberately not `Display`/`Debug`
    /// (those are for humans and would break if a variant were renamed).
    pub fn as_str(self) -> &'static str {
        match self {
            ModelId::Tiny => "tiny",
            ModelId::Base => "base",
            ModelId::Small => "small",
            ModelId::Medium => "medium",
            ModelId::Large => "large",
        }
    }

    pub fn from_str_id(s: &str) -> Result<Self, ModelError> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| ModelError::UnknownModel {
                model_id: s.to_string(),
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ModelCatalogEntry {
    pub id: ModelId,
    /// Real ggml filename on disk and in the Hugging Face repo, e.g.
    /// `ggml-tiny.bin`. `large` maps to `ggml-large-v3.bin` — whisper.cpp's
    /// upstream repo has published v1/v2/v3(/v3-turbo) large weights over
    /// time; v3 is the current best-accuracy release as of this writing.
    pub filename: String,
    pub display_name: String,
    /// Verified via a real HEAD request (module doc comment), not an
    /// estimate.
    pub approx_size_bytes: u64,
    /// `true` for every model in this catalog (module doc comment:
    /// English-only `.en` variants are not offered), kept as a field rather
    /// than assumed so the frontend never has to hardcode "all models here
    /// are multilingual" — master prompt §14 explicitly asks for
    /// "language support" as a shown property.
    pub multilingual: bool,
    pub download_url: String,
}

/// `pub(crate)`, not private: `download::peek_expected_sha256` gates its
/// Hugging-Face-specific `X-Linked-ETag` hash lookup on a download URL
/// actually starting with this exact base (that module's doc comment) —
/// shared here so the two never drift apart.
pub(crate) const HF_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

fn entry(
    id: ModelId,
    filename: &str,
    display_name: &str,
    approx_size_bytes: u64,
) -> ModelCatalogEntry {
    ModelCatalogEntry {
        id,
        filename: filename.to_string(),
        display_name: display_name.to_string(),
        approx_size_bytes,
        multilingual: true,
        download_url: format!("{HF_BASE_URL}/{filename}"),
    }
}

/// The static model catalog (master prompt §60 "Available models"). Sizes
/// are real `Content-Length` values from Hugging Face, checked 2026-09-05 —
/// see module doc comment.
pub fn catalog() -> Vec<ModelCatalogEntry> {
    vec![
        entry(ModelId::Tiny, "ggml-tiny.bin", "Tiny", 77_691_713),
        entry(ModelId::Base, "ggml-base.bin", "Base", 147_951_465),
        entry(ModelId::Small, "ggml-small.bin", "Small", 487_601_967),
        entry(ModelId::Medium, "ggml-medium.bin", "Medium", 1_533_763_059),
        entry(
            ModelId::Large,
            "ggml-large-v3.bin",
            "Large (v3)",
            3_095_033_483,
        ),
    ]
}

pub fn catalog_entry(id: ModelId) -> ModelCatalogEntry {
    // Infallible: `catalog()` always has exactly one entry per `ModelId`
    // variant (enforced by `catalog_has_exactly_one_entry_per_model_id`
    // below), so this `expect` can never actually fire.
    catalog()
        .into_iter()
        .find(|e| e.id == id)
        .expect("catalog always has one entry per ModelId")
}

/// A model found actually installed on disk (master prompt §60 "Installed
/// models").
#[derive(Debug, Clone, Serialize, Type)]
pub struct InstalledModel {
    pub id: ModelId,
    pub path: String,
    pub size_bytes: u64,
}

/// Scans `dest_dir` for fully-downloaded models — deliberately only ever
/// looks for the *final* filename (`ggml-tiny.bin`, never
/// `ggml-tiny.bin.part`), so a killed/interrupted download is never
/// reported as installed (master prompt §60: "Do not treat partially
/// downloaded models as installed").
pub fn list_installed(dest_dir: &Path) -> Vec<InstalledModel> {
    catalog()
        .into_iter()
        .filter_map(|e| {
            let path = dest_dir.join(&e.filename);
            let size_bytes = std::fs::metadata(&path).ok()?.len();
            Some(InstalledModel {
                id: e.id,
                path: path.to_string_lossy().to_string(),
                size_bytes,
            })
        })
        .collect()
}

pub fn is_installed(dest_dir: &Path, id: ModelId) -> bool {
    dest_dir.join(catalog_entry(id).filename).is_file()
}

/// Deletes an installed model's final file (never touches a `.part` file —
/// there is nothing "installed" to delete if only a partial download
/// exists; the caller sees `ModelError::NotInstalled` for that case, same
/// as for a model that was never downloaded at all).
pub fn delete_model(dest_dir: &Path, id: ModelId) -> Result<(), ModelError> {
    let entry = catalog_entry(id);
    let path = dest_dir.join(&entry.filename);
    if !path.is_file() {
        return Err(ModelError::NotInstalled {
            model_id: id.as_str().to_string(),
        });
    }
    std::fs::remove_file(&path).map_err(|e| ModelError::IoFailed {
        model_id: id.as_str().to_string(),
        details: format!("deleting {}: {e}", path.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_exactly_one_entry_per_model_id() {
        let entries = catalog();
        assert_eq!(entries.len(), ModelId::ALL.len());
        let ids: HashSet<ModelId> = entries.iter().map(|e| e.id).collect();
        for id in ModelId::ALL {
            assert!(
                ids.contains(&id),
                "missing catalog entry for {}",
                id.as_str()
            );
        }
    }

    #[test]
    fn every_catalog_entry_has_sane_metadata() {
        for e in catalog() {
            assert!(!e.filename.is_empty());
            assert!(e.filename.ends_with(".bin"));
            assert!(!e.display_name.is_empty());
            // Real model files are all comfortably between 50MB and 4GB —
            // catches an obviously-wrong size (a copy/paste typo, an off-
            // by-a-thousand unit mistake) without hardcoding exact bytes
            // twice.
            assert!(
                e.approx_size_bytes > 50_000_000 && e.approx_size_bytes < 4_000_000_000,
                "{}: {}",
                e.filename,
                e.approx_size_bytes
            );
            assert!(e.multilingual);
            assert!(e.download_url.starts_with("https://huggingface.co/"));
            assert!(e.download_url.ends_with(&e.filename));
        }
    }

    #[test]
    fn catalog_sizes_strictly_increase_with_model_size() {
        // tiny < base < small < medium < large — a sanity check that the
        // hardcoded sizes weren't transposed between entries.
        let sizes: Vec<u64> = ModelId::ALL
            .map(|id| catalog_entry(id).approx_size_bytes)
            .to_vec();
        for pair in sizes.windows(2) {
            assert!(pair[0] < pair[1], "{sizes:?}");
        }
    }

    #[test]
    fn model_id_round_trips_through_its_string_form() {
        for id in ModelId::ALL {
            assert_eq!(ModelId::from_str_id(id.as_str()).unwrap(), id);
        }
    }

    #[test]
    fn unknown_string_id_is_a_real_error() {
        let err = ModelId::from_str_id("gigantic").unwrap_err();
        assert!(matches!(err, ModelError::UnknownModel { .. }));
    }

    #[test]
    fn catalog_entry_matches_the_requested_id() {
        for id in ModelId::ALL {
            assert_eq!(catalog_entry(id).id, id);
        }
    }

    // -- list_installed / is_installed / delete_model --------------------

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-model-mgr-test-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn empty_directory_has_no_installed_models() {
        let dir = temp_dir("empty");
        assert!(list_installed(&dir).is_empty());
        for id in ModelId::ALL {
            assert!(!is_installed(&dir, id));
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_final_file_is_reported_installed_but_a_part_file_is_not() {
        let dir = temp_dir("part-vs-final");
        std::fs::write(dir.join("ggml-tiny.bin"), b"fake-tiny-model-bytes").unwrap();
        std::fs::write(dir.join("ggml-base.bin.part"), b"still-downloading").unwrap();

        assert!(is_installed(&dir, ModelId::Tiny));
        assert!(!is_installed(&dir, ModelId::Base));

        let installed = list_installed(&dir);
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id, ModelId::Tiny);
        assert_eq!(
            installed[0].size_bytes,
            "fake-tiny-model-bytes".len() as u64
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_model_removes_an_installed_file() {
        let dir = temp_dir("delete-ok");
        let path = dir.join("ggml-tiny.bin");
        std::fs::write(&path, b"data").unwrap();
        delete_model(&dir, ModelId::Tiny).expect("delete succeeds");
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_model_errors_when_not_installed() {
        let dir = temp_dir("delete-missing");
        let err = delete_model(&dir, ModelId::Tiny).unwrap_err();
        assert!(matches!(err, ModelError::NotInstalled { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_model_does_not_delete_a_part_file() {
        let dir = temp_dir("delete-part-only");
        std::fs::write(dir.join("ggml-tiny.bin.part"), b"partial").unwrap();
        let err = delete_model(&dir, ModelId::Tiny).unwrap_err();
        assert!(matches!(err, ModelError::NotInstalled { .. }));
        assert!(dir.join("ggml-tiny.bin.part").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- §88 Windows path edge cases: a model destination directory
    //    containing spaces and real non-ASCII Unicode (a real Windows user
    //    profile can look exactly like this, e.g.
    //    `C:\Users\Nguyễn Văn A\AppData\Local\AI Video Editor\models`) -----

    #[test]
    fn list_installed_and_delete_model_work_under_a_unicode_and_space_containing_dest_dir() {
        let base = temp_dir("unicode-dest-dir");
        let dest_dir = base.join("Users").join("Nguyễn Văn A 🎬").join("models");
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("ggml-tiny.bin"), b"fake-tiny-model-bytes").unwrap();

        assert!(is_installed(&dest_dir, ModelId::Tiny));
        let installed = list_installed(&dest_dir);
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id, ModelId::Tiny);
        // The reported path really does carry the Unicode segment through,
        // not a mangled/lossy substitute.
        assert!(installed[0].path.contains("Nguyễn Văn A 🎬"));

        delete_model(&dest_dir, ModelId::Tiny).expect("delete succeeds under a Unicode path");
        assert!(!dest_dir.join("ggml-tiny.bin").exists());

        std::fs::remove_dir_all(&base).ok();
    }
}
