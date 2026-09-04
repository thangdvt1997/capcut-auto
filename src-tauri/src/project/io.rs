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
}
