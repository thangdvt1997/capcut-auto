//! Atomic project-file I/O and the version-migration dispatcher, per
//! `docs/project-format.md` "Migration layer": `project.json.tmp` -> fsync
//! -> rename over `project.json` (master prompt §6). Recovery-snapshot
//! rotation (`.bak.<timestamp>`, pruned by count) is explicitly out of
//! scope here — that's master prompt §86 / Phase 12 (crash handling &
//! recovery), a UI-and-policy feature, not a schema-layer primitive.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use super::error::ProjectError;
use super::types::ProjectV1;

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

impl ProjectV1 {
    /// Parses `project.json` (or any bytes claiming to be one), dispatching
    /// on the `version` field. Today there is only `ProjectV1`, so this is
    /// the no-op case the schema doc describes — but it's a real dispatch,
    /// not a hardcoded parse, so `ProjectV2` gets its own arm later without
    /// touching every call site.
    /// ## §63 test-coverage honesty note
    ///
    /// This schema has only ever been at `SCHEMA_VERSION == 1` throughout
    /// this whole project (every phase added fields additively; none ever
    /// bumped it) — there is no real, shipped `ProjectV2` to migrate *from*,
    /// so a genuine "vN -> vN+1 migration actually transforms real old
    /// data" test cannot exist honestly yet. What real test coverage
    /// *does* exist for this dispatcher today (`tests` module below):
    /// `migrate_to_latest_is_a_real_no_op_dispatch_for_the_current_version`
    /// (the `1 => ...` arm, a real dispatch through the match rather than a
    /// hardcoded parse, even though only one real version exists to
    /// dispatch on), `migrate_rejects_future_version` (the `v >
    /// SCHEMA_VERSION` arm), and
    /// `migrate_rejects_an_old_version_with_no_implemented_migration_path`
    /// (the `other` arm — an old, unimplemented version number, standing in
    /// for a hypothetical pre-v1 schema, must be a real, clear
    /// `MigrationFailed` error rather than a silent fallthrough or a panic).
    /// Fabricating a fake "v0" fixture and pretending it exercises a real
    /// migration would be dishonest; this is the real, complete coverage
    /// that's actually possible for a single-version schema.
    pub fn migrate_to_latest(bytes: &[u8]) -> Result<ProjectV1, ProjectError> {
        let raw: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| ProjectError::CorruptJson {
                details: e.to_string(),
            })?;

        let version = raw.get("version").and_then(|v| v.as_u64()).ok_or_else(|| {
            ProjectError::CorruptJson {
                details: "missing or non-numeric \"version\" field".to_string(),
            }
        })?;

        match version {
            1 => serde_json::from_value(raw).map_err(|e| ProjectError::MigrationFailed {
                from: 1,
                to: ProjectV1::SCHEMA_VERSION,
                details: e.to_string(),
            }),
            v if v > u64::from(ProjectV1::SCHEMA_VERSION) => {
                Err(ProjectError::SchemaVersionTooNew {
                    found: v as u32,
                    max_supported: ProjectV1::SCHEMA_VERSION,
                })
            }
            other => Err(ProjectError::MigrationFailed {
                from: other as u32,
                to: ProjectV1::SCHEMA_VERSION,
                details: "no migration path implemented for this version".to_string(),
            }),
        }
    }

    /// Loads and migrates a project file from disk.
    pub fn load(path: &Path) -> Result<ProjectV1, ProjectError> {
        let bytes = fs::read(path).map_err(|e| ProjectError::CorruptJson {
            details: e.to_string(),
        })?;
        ProjectV1::migrate_to_latest(&bytes)
    }

    /// Atomically writes this project to `path`: serialize -> write to
    /// `<path>.tmp` -> fsync -> rename over `path`. A crash or power loss
    /// mid-write leaves either the old file or the new one intact, never a
    /// truncated/corrupt one.
    pub fn save_atomic(&self, path: &Path) -> Result<(), ProjectError> {
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| ProjectError::AtomicWriteFailed {
                details: format!("serialize failed: {e}"),
            })?;

        let tmp_path = path.with_extension("json.tmp");
        {
            let mut file =
                File::create(&tmp_path).map_err(|e| ProjectError::AtomicWriteFailed {
                    details: format!("could not create {}: {e}", tmp_path.display()),
                })?;
            file.write_all(&json)
                .map_err(|e| ProjectError::AtomicWriteFailed {
                    details: format!("write failed: {e}"),
                })?;
            file.sync_all()
                .map_err(|e| ProjectError::AtomicWriteFailed {
                    details: format!("fsync failed: {e}"),
                })?;
        }

        fs::rename(&tmp_path, path).map_err(|e| ProjectError::AtomicWriteFailed {
            details: format!(
                "rename {} -> {} failed: {e}",
                tmp_path.display(),
                path.display()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let dir =
            std::env::temp_dir().join(format!("ave-project-io-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("project.json");

        let original = ProjectV1::new("Atomic Save Test");
        original.save_atomic(&path).expect("save_atomic");

        let loaded = ProjectV1::load(&path).expect("load");
        assert_eq!(loaded.project.id, original.project.id);
        assert_eq!(loaded.project.name, "Atomic Save Test");
        assert_eq!(loaded.version, 1);

        // No leftover .tmp file after a successful save.
        assert!(!path.with_extension("json.tmp").exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrate_rejects_future_version() {
        let bytes = serde_json::json!({"version": 99}).to_string();
        let err = ProjectV1::migrate_to_latest(bytes.as_bytes()).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::SchemaVersionTooNew { found: 99, .. }
        ));
    }

    #[test]
    fn migrate_rejects_corrupt_json() {
        let err = ProjectV1::migrate_to_latest(b"not json").unwrap_err();
        assert!(matches!(err, ProjectError::CorruptJson { .. }));
    }

    // -- §63: the real, honest migration-dispatch coverage a single-version
    //    schema can actually have (see `migrate_to_latest`'s own doc comment
    //    for why a real vN -> vN+1 test can't exist yet) -------------------

    #[test]
    fn migrate_to_latest_is_a_real_no_op_dispatch_for_the_current_version() {
        let project = ProjectV1::new("No-op Migration Test");
        let bytes = serde_json::to_vec(&project).unwrap();

        let migrated =
            ProjectV1::migrate_to_latest(&bytes).expect("the current version dispatches cleanly");
        assert_eq!(migrated.project.id, project.project.id);
        assert_eq!(migrated.project.name, "No-op Migration Test");
        assert_eq!(migrated.version, ProjectV1::SCHEMA_VERSION);
    }

    #[test]
    fn migrate_rejects_an_old_version_with_no_implemented_migration_path() {
        // Version 0 stands in for a hypothetical pre-v1 schema this
        // dispatcher has no migration arm for — the `other` match arm,
        // otherwise never exercised by any test.
        let bytes = serde_json::json!({"version": 0}).to_string();
        let err = ProjectV1::migrate_to_latest(bytes.as_bytes()).unwrap_err();
        assert!(matches!(
            err,
            ProjectError::MigrationFailed { from: 0, to: 1, .. }
        ));
    }

    // -- §88 Windows path edge cases: spaces, Unicode, Vietnamese, other
    //    non-ASCII, a very long path, and a UNC-shaped path string --------
    //
    // This crate's real dev/test environment is WSL2/Linux (`HANDOFF.md`),
    // so these exercise this code's own path-joining/string-handling logic
    // for real (a real `fs::create_dir_all`/`File::create`/`fs::rename`
    // round trip against these exact byte sequences) — not Windows'
    // specific 260-char `MAX_PATH` enforcement or real UNC network I/O,
    // neither of which WSL2's own filesystem reproduces. Whether a real
    // Windows build additionally needs a `\\?\` extended-path prefix for
    // the long-path case, and whether a real `\\server\share\...` UNC
    // target is actually writable, both still need verification on a real
    // Windows machine.

    fn round_trip_project_at(dir_name: &str, file_name: &str) {
        let dir = std::env::temp_dir().join(format!("{dir_name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file_name);

        let original = ProjectV1::new("Path Edge Case Test");
        original
            .save_atomic(&path)
            .unwrap_or_else(|e| panic!("save_atomic failed for {}: {e}", path.display()));

        let loaded = ProjectV1::load(&path)
            .unwrap_or_else(|e| panic!("load failed for {}: {e}", path.display()));
        assert_eq!(loaded.project.id, original.project.id);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "no leftover .tmp for {}",
            path.display()
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_and_load_round_trip_a_path_containing_spaces() {
        round_trip_project_at("ave-path-spaces", "My Video Project.json");
    }

    #[test]
    fn save_and_load_round_trip_a_real_vietnamese_filename() {
        // A real Vietnamese filename with combining diacritics, not an
        // ASCII transliteration (master prompt §88's own worked example
        // shape).
        round_trip_project_at("ave-path-vietnamese", "Việt Nam - Xin chào.json");
    }

    #[test]
    fn save_and_load_round_trip_other_non_ascii_unicode() {
        // Japanese plus an emoji in the same filename — a broader Unicode
        // case than Vietnamese alone.
        round_trip_project_at("ave-path-unicode", "動画プロジェクト 🎬.json");
    }

    #[test]
    fn save_and_load_round_trip_a_very_long_path() {
        // Windows' classic MAX_PATH is 260 characters. WSL2's own
        // filesystem doesn't enforce that limit (module doc comment), so
        // this proves this crate's own string-handling/path-joining logic
        // doesn't itself choke on a long path — not that a real Windows
        // build handles it without a `\\?\` prefix, which needs separate
        // real-Windows verification.
        let dir = std::env::temp_dir().join(format!("ave-path-long-{}", uuid::Uuid::new_v4()));
        // Several long nested directory segments to comfortably clear 260
        // total characters by the time the file itself is reached.
        let mut path = dir.clone();
        for i in 0..6 {
            path = path.join(format!(
                "a-very-long-nested-directory-segment-number-{i}-to-approach-the-windows-max-path-limit"
            ));
        }
        fs::create_dir_all(&path).unwrap();
        let file_path = path.join("project.json");
        assert!(
            file_path.to_string_lossy().len() > 260,
            "test setup should actually exceed 260 chars: {}",
            file_path.to_string_lossy().len()
        );

        let original = ProjectV1::new("Long Path Test");
        original
            .save_atomic(&file_path)
            .unwrap_or_else(|e| panic!("save_atomic failed for a long path: {e}"));
        let loaded = ProjectV1::load(&file_path).expect("load succeeds for a long path");
        assert_eq!(loaded.project.id, original.project.id);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_atomic_and_load_do_not_mis_parse_a_unc_shaped_path_string() {
        // UNC paths (`\\server\share\...`) are a Windows-specific string
        // convention; real UNC network I/O can only be verified on real
        // Windows. This proves this code's own path handling doesn't
        // assume forward-slash-only splitting or otherwise choke on a
        // UNC-*shaped* string — `Path::with_extension`/`.parent()`/
        // `fs::rename` all still need to behave sanely given one. Since a
        // literal backslash is just an ordinary filename character to a
        // POSIX filesystem (not a separator), this exercises the string
        // handling without attempting real network I/O.
        let dir = std::env::temp_dir().join(format!("ave-path-unc-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        // The backslashes here are literal filename characters on this
        // POSIX test environment, not path separators — this is
        // deliberately testing string robustness, not real UNC access.
        // Built via `format!`, not `dir.join(r"\\server\share\...")`:
        // `Path::join` replaces the whole path when its argument looks
        // absolute (clippy's `join_absolute_paths` catches exactly this),
        // which isn't what this test wants to exercise.
        let unc_shaped_name = r"\\server\share\project.json";
        let path = std::path::PathBuf::from(format!("{}/{unc_shaped_name}", dir.display()));

        let original = ProjectV1::new("UNC Shaped Path Test");
        original
            .save_atomic(&path)
            .unwrap_or_else(|e| panic!("save_atomic failed for a UNC-shaped name: {e}"));
        let loaded = ProjectV1::load(&path).expect("load succeeds for a UNC-shaped name");
        assert_eq!(loaded.project.id, original.project.id);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "no leftover .tmp for a UNC-shaped name"
        );

        fs::remove_dir_all(&dir).ok();
    }
}
