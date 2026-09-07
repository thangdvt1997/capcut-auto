//! Validate Draft (`STUDIO_PLAN.md` Phase S1 — `promt.md` §10): a real
//! integrity check against an *existing* on-disk draft folder, as opposed
//! to `capcut::compat`'s pre-export lint (which checks the in-app project
//! *before* it's ever written to disk). Answers three real questions a
//! draft can silently fail on:
//!
//! 1. Do `draft_content.json`/`draft_info.json`/`draft_meta_info.json` all
//!    exist and parse as valid JSON? A draft missing `draft_meta_info.json`
//!    is exactly the real bug `capcut::meta` fixed for *new* exports
//!    (`IMPLEMENTATION_PLAN.md` Phase 9's real-CapCut-Pro validation) — this
//!    check catches the same problem for a draft that predates the fix, or
//!    one edited by hand.
//! 2. Is this draft actually registered in its parent folder's shared
//!    `root_meta_info.json` (`capcut::meta` module doc comment)? A draft
//!    folder can exist and be perfectly valid on its own while still being
//!    invisible in CapCut's own Projects-list UI if this is `false`.
//! 3. Does every media file `draft_content.json`'s own `materials.videos`/
//!    `materials.audios` reference still exist on disk? A moved or deleted
//!    source file is a real, common real-world failure mode this project
//!    has no other way to detect short of opening the draft in CapCut
//!    itself and seeing a broken link.
//!
//! Never silently reports "OK" on a draft it didn't actually check — every
//! field below reflects a real, performed check, and [`DraftValidationReport::is_healthy`]
//! is `false` whenever any of them failed.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use specta::Type;

use super::meta::forward_slashes;

#[derive(Debug, Clone, Serialize, Type)]
pub struct DraftValidationReport {
    pub draft_dir: String,
    pub draft_dir_exists: bool,
    pub has_draft_content_json: bool,
    pub has_draft_info_json: bool,
    pub has_draft_meta_info_json: bool,
    /// `false` whenever any of the three files above exists but fails to
    /// parse as JSON — a corrupt file is reported here, not silently
    /// treated the same as a missing one.
    pub json_files_parse_cleanly: bool,
    /// Whether this draft's own `draft_fold_path` was found in its parent
    /// folder's `root_meta_info.json`'s `all_draft_store` array
    /// (`capcut::meta`'s own registry). `false` if the registry file itself
    /// is missing or fails to parse, or if no matching entry exists.
    pub registered_in_root_registry: bool,
    /// Every path from `draft_content.json`'s own `materials.videos`/
    /// `materials.audios` that does not currently exist on disk. Empty if
    /// every referenced file was found, or if `draft_content.json` itself
    /// couldn't be read (in which case that's already reflected in
    /// `json_files_parse_cleanly`, not duplicated here).
    pub missing_media_files: Vec<String>,
    /// Human-readable summary of every problem found, empty iff
    /// [`Self::is_healthy`] is `true`.
    pub problems: Vec<String>,
}

impl DraftValidationReport {
    pub fn is_healthy(&self) -> bool {
        self.problems.is_empty()
    }
}

fn try_read_json(path: &Path) -> Option<Result<Value, String>> {
    if !path.is_file() {
        return None;
    }
    Some(
        std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string())),
    )
}

/// Extracts every `materials.<kind>[].path` string from a real, already-
/// parsed `draft_content.json` value — `kind` is `"videos"` or `"audios"`,
/// matching `capcut::script::ScriptFile::export_json`'s own top-level
/// `materials` shape exactly (no other material kind carries a real
/// on-disk file path today).
fn media_paths(content: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for kind in ["videos", "audios"] {
        let Some(items) = content
            .get("materials")
            .and_then(|m| m.get(kind))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for item in items {
            if let Some(path) = item.get("path").and_then(Value::as_str) {
                if !path.is_empty() {
                    out.push(path.to_string());
                }
            }
        }
    }
    out
}

/// Runs every real check (module doc comment) against `draft_dir`. Never
/// panics and never returns an `Err` — a draft that doesn't exist at all,
/// or whose files are all corrupt, still produces a real, fully-populated
/// report describing exactly what's wrong, since "tell the user precisely
/// what's broken" is the entire point of this function.
pub fn validate_draft(draft_dir: &Path) -> DraftValidationReport {
    let mut problems = Vec::new();
    let draft_dir_exists = draft_dir.is_dir();
    if !draft_dir_exists {
        problems.push(format!(
            "draft folder does not exist: {}",
            draft_dir.display()
        ));
        return DraftValidationReport {
            draft_dir: draft_dir.display().to_string(),
            draft_dir_exists: false,
            has_draft_content_json: false,
            has_draft_info_json: false,
            has_draft_meta_info_json: false,
            json_files_parse_cleanly: false,
            registered_in_root_registry: false,
            missing_media_files: Vec::new(),
            problems,
        };
    }

    let mut json_files_parse_cleanly = true;
    let mut content_value: Option<Value> = None;

    let content_result = try_read_json(&draft_dir.join("draft_content.json"));
    let has_draft_content_json = content_result.is_some();
    match content_result {
        None => {
            problems.push("draft_content.json is missing".to_string());
            json_files_parse_cleanly = false;
        }
        Some(Err(e)) => {
            problems.push(format!("draft_content.json failed to parse: {e}"));
            json_files_parse_cleanly = false;
        }
        Some(Ok(value)) => content_value = Some(value),
    }

    // `draft_info.json` is deliberately never required here: real
    // CapCut-created projects never write this file at all (confirmed
    // against this project's own real, healthy projects during Phase S1's
    // own development — see `STUDIO_PLAN.md`) — it exists only as this
    // app's own "dual-file-compatibility" export convenience
    // (`CapCutAdapter::export_draft`'s own doc comment), a byte-identical
    // copy of `draft_content.json` under a second name for some other
    // reference tool's own naming convention. Its absence is normal, not a
    // problem; reported informationally only, never added to `problems`.
    let info_result = try_read_json(&draft_dir.join("draft_info.json"));
    let has_draft_info_json = info_result.is_some();
    if let Some(Err(e)) = &info_result {
        // Present but corrupt is still worth surfacing — nothing reads
        // this file back, but a corrupt copy sitting next to a healthy
        // `draft_content.json` is still a real, honest anomaly to report.
        problems.push(format!("draft_info.json failed to parse: {e}"));
        json_files_parse_cleanly = false;
    }

    let meta_result = try_read_json(&draft_dir.join("draft_meta_info.json"));
    let has_draft_meta_info_json = meta_result.is_some();
    if let Some(Err(e)) = &meta_result {
        problems.push(format!("draft_meta_info.json failed to parse: {e}"));
        json_files_parse_cleanly = false;
    } else if meta_result.is_none() {
        problems.push(
            "draft_meta_info.json is missing (CapCut Pro will not show this draft in its own Projects list — see capcut::meta module doc comment)"
                .to_string(),
        );
        json_files_parse_cleanly = false;
    }

    let registered_in_root_registry = check_root_registry(draft_dir, &mut problems);

    let missing_media_files = match &content_value {
        Some(content) => {
            let mut missing = Vec::new();
            for path in media_paths(content) {
                if !Path::new(&path).is_file() {
                    missing.push(path);
                }
            }
            if !missing.is_empty() {
                problems.push(format!(
                    "{} referenced media file(s) no longer exist on disk: {}",
                    missing.len(),
                    missing.join(", ")
                ));
            }
            missing
        }
        None => Vec::new(),
    };

    DraftValidationReport {
        draft_dir: draft_dir.display().to_string(),
        draft_dir_exists,
        has_draft_content_json,
        has_draft_info_json,
        has_draft_meta_info_json,
        json_files_parse_cleanly,
        registered_in_root_registry,
        missing_media_files,
        problems,
    }
}

fn check_root_registry(draft_dir: &Path, problems: &mut Vec<String>) -> bool {
    let Some(draft_root) = draft_dir.parent() else {
        problems.push("draft folder has no parent directory to hold a root registry".to_string());
        return false;
    };
    let registry_path = draft_root.join("root_meta_info.json");
    let Some(result) = try_read_json(&registry_path) else {
        problems.push(format!(
            "root_meta_info.json is missing at {} — CapCut cannot discover this draft",
            registry_path.display()
        ));
        return false;
    };
    let registry = match result {
        Ok(value) => value,
        Err(e) => {
            problems.push(format!("root_meta_info.json failed to parse: {e}"));
            return false;
        }
    };
    let draft_fold_path = forward_slashes(draft_dir);
    let found = registry
        .get("all_draft_store")
        .and_then(Value::as_array)
        .is_some_and(|store| {
            store.iter().any(|e| {
                e.get("draft_fold_path").and_then(Value::as_str) == Some(draft_fold_path.as_str())
            })
        });
    if !found {
        problems.push(
            "this draft is not registered in root_meta_info.json's all_draft_store — CapCut's own Projects list will not show it".to_string(),
        );
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capcut::adapter::CapCutAdapter;
    use uuid::Uuid;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-capcut-validate-test-{label}-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_nonexistent_draft_folder_is_reported_unhealthy_with_a_clear_problem() {
        let root = temp_dir("missing");
        let draft_dir = root.join("Nonexistent Draft");
        let report = validate_draft(&draft_dir);
        assert!(!report.draft_dir_exists);
        assert!(!report.is_healthy());
        assert!(report.problems.iter().any(|p| p.contains("does not exist")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_real_export_via_capcutadapter_validates_as_fully_healthy() {
        let root = temp_dir("healthy");
        let draft_dir = root.join("Healthy Draft");
        let adapter = CapCutAdapter::create_draft(1080, 1920, 30.0);
        adapter
            .export_draft(&draft_dir)
            .expect("export should succeed");

        let report = validate_draft(&draft_dir);
        assert!(report.has_draft_content_json);
        assert!(report.has_draft_info_json);
        assert!(report.has_draft_meta_info_json);
        assert!(report.json_files_parse_cleanly);
        assert!(report.registered_in_root_registry);
        assert!(report.missing_media_files.is_empty());
        assert!(
            report.is_healthy(),
            "expected a real, freshly-exported draft to validate as healthy: {:?}",
            report.problems
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// Real-world regression: a draft created by CapCut Pro itself (not
    /// this app's own exporter) never has a `draft_info.json` at all —
    /// confirmed by running this exact check against this project's own
    /// real, healthy CapCut projects during Phase S1's own development.
    /// Its absence must never be reported as a problem.
    #[test]
    fn a_draft_with_no_draft_info_json_still_validates_as_healthy() {
        let root = temp_dir("no-draft-info-json");
        let draft_dir = root.join("Real CapCut Native Draft");
        let adapter = CapCutAdapter::create_draft(1080, 1920, 30.0);
        adapter
            .export_draft(&draft_dir)
            .expect("export should succeed");
        std::fs::remove_file(draft_dir.join("draft_info.json")).unwrap();

        let report = validate_draft(&draft_dir);
        assert!(!report.has_draft_info_json);
        assert!(
            report.is_healthy(),
            "a missing draft_info.json alone must never make a draft unhealthy: {:?}",
            report.problems
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_draft_missing_meta_info_json_reports_that_specific_problem() {
        let root = temp_dir("no-meta");
        let draft_dir = root.join("No Meta Draft");
        let adapter = CapCutAdapter::create_draft(1080, 1920, 30.0);
        adapter
            .export_draft(&draft_dir)
            .expect("export should succeed");
        std::fs::remove_file(draft_dir.join("draft_meta_info.json")).unwrap();

        let report = validate_draft(&draft_dir);
        assert!(!report.has_draft_meta_info_json);
        assert!(!report.is_healthy());
        assert!(report
            .problems
            .iter()
            .any(|p| p.contains("draft_meta_info.json is missing")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_draft_never_registered_in_the_root_registry_reports_that_problem() {
        let root = temp_dir("unregistered");
        let draft_dir = root.join("Unregistered Draft");
        std::fs::create_dir_all(&draft_dir).unwrap();
        std::fs::write(draft_dir.join("draft_content.json"), b"{}").unwrap();
        std::fs::write(draft_dir.join("draft_info.json"), b"{}").unwrap();
        std::fs::write(draft_dir.join("draft_meta_info.json"), b"{}").unwrap();
        // No root_meta_info.json written at all in `root`.

        let report = validate_draft(&draft_dir);
        assert!(!report.registered_in_root_registry);
        assert!(!report.is_healthy());
        assert!(report
            .problems
            .iter()
            .any(|p| p.contains("root_meta_info.json is missing")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_draft_referencing_a_deleted_media_file_reports_it_by_path() {
        let root = temp_dir("missing-media");
        let media_dir = temp_dir("missing-media-source");
        let missing_media_path = media_dir.join("gone.mp4");
        std::fs::write(&missing_media_path, b"pretend video").unwrap();

        let mut adapter = CapCutAdapter::create_draft(1080, 1920, 30.0);
        let material = crate::capcut::material::VideoMaterial::new(
            missing_media_path.to_string_lossy().to_string(),
            "gone.mp4",
            5_000_000,
            1080,
            1920,
            crate::capcut::material::VideoMaterialKind::Video,
        );
        let track_id = adapter.add_track(crate::capcut::track::TrackType::Video, "V1", 0);
        adapter
            .add_video(
                &track_id,
                material,
                crate::capcut::timerange::Timerange::new(0, 5_000_000),
                crate::capcut::timerange::Timerange::new(0, 5_000_000),
                1.0,
                1.0,
                crate::capcut::clip_settings::CapCutClipSettings::default(),
            )
            .unwrap();

        let draft_dir = root.join("Missing Media Draft");
        adapter
            .export_draft(&draft_dir)
            .expect("export should succeed");

        // Now actually delete the referenced source file.
        std::fs::remove_file(&missing_media_path).unwrap();

        let report = validate_draft(&draft_dir);
        assert_eq!(
            report.missing_media_files,
            vec![missing_media_path.to_string_lossy().to_string()]
        );
        assert!(!report.is_healthy());
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&media_dir).ok();
    }
}
