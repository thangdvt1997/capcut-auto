//! On-disk storage for Smart Automation rules (upgrade spec §27), under this
//! app's own `automation_rules/` app-data directory — the exact same
//! JSON-file-per-item, atomic temp-file-then-rename convention
//! `assets::io`/`templates::io` already use (see `automation` module doc
//! comment for why this simpler convention is the right fit here rather
//! than a `db::MediaLibrary`-style SQLite table: a handful of user-defined
//! rules, not a bulk/searchable index).
//!
//! Also holds each rule's fire log (`RuleFireRecord`s) — one flat JSON array
//! file per rule, `<id>.fire_log.json`, appended to and atomically
//! rewritten — mirroring `templates::io::append_template_history`/
//! `list_template_history`'s exact convention for a template's version
//! history, not a new append-log design.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::error::AutomationError;
use super::{AutomationRule, RuleFireRecord};

/// One rule's on-disk filename inside `automation_rules/`: `<id>.json`.
/// Rejects an id that isn't a safe single path component (master prompt §53
/// path traversal prevention) before ever joining it onto `dir` — same
/// defense-in-depth rationale as `assets::io::asset_file_path`/
/// `templates::io::template_file_path`.
fn rule_file_path(dir: &Path, rule_id: &str) -> Result<PathBuf, AutomationError> {
    if !crate::fs_safety::is_safe_path_component(rule_id) {
        return Err(AutomationError::UnsafeRuleId {
            rule_id: rule_id.to_string(),
        });
    }
    Ok(dir.join(format!("{rule_id}.json")))
}

/// `<id>.fire_log.json` — deliberately a different suffix than
/// `<id>.json` (the rule itself) so both can live side by side in the same
/// directory without colliding, same "one flat file per concern" layout
/// `templates::io`'s `<id>.history.json` already established.
fn fire_log_path(dir: &Path, rule_id: &str) -> Result<PathBuf, AutomationError> {
    if !crate::fs_safety::is_safe_path_component(rule_id) {
        return Err(AutomationError::UnsafeRuleId {
            rule_id: rule_id.to_string(),
        });
    }
    Ok(dir.join(format!("{rule_id}.fire_log.json")))
}

fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), AutomationError> {
    let json = serde_json::to_vec_pretty(value).map_err(|e| AutomationError::IoFailed {
        details: format!("serialize failed: {e}"),
    })?;

    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp_path).map_err(|e| AutomationError::IoFailed {
            details: format!("could not create {}: {e}", tmp_path.display()),
        })?;
        file.write_all(&json)
            .map_err(|e| AutomationError::IoFailed {
                details: format!("write failed: {e}"),
            })?;
        file.sync_all().map_err(|e| AutomationError::IoFailed {
            details: format!("fsync failed: {e}"),
        })?;
    }

    fs::rename(&tmp_path, path).map_err(|e| AutomationError::IoFailed {
        details: format!(
            "rename {} -> {} failed: {e}",
            tmp_path.display(),
            path.display()
        ),
    })
}

fn read_rule(path: &Path) -> Result<AutomationRule, AutomationError> {
    let bytes = fs::read(path).map_err(|e| AutomationError::IoFailed {
        details: format!("could not read {}: {e}", path.display()),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| AutomationError::CorruptJson {
        details: format!("{}: {e}", path.display()),
    })
}

/// Atomically writes `rule` into `automation_rules/` as `<id>.json`,
/// creating the directory if it doesn't exist yet — used for both a
/// brand-new rule and an in-place update (same file, new content).
pub fn save_rule(dir: &Path, rule: &AutomationRule) -> Result<PathBuf, AutomationError> {
    fs::create_dir_all(dir).map_err(|e| AutomationError::IoFailed {
        details: format!("could not create {}: {e}", dir.display()),
    })?;
    let path = rule_file_path(dir, &rule.id)?;
    write_json_atomic(&path, rule)?;
    Ok(path)
}

/// Loads a single rule by id. Errors with `AutomationError::UnknownRule` if
/// no such file exists.
pub fn load_rule(dir: &Path, rule_id: &str) -> Result<AutomationRule, AutomationError> {
    let path = rule_file_path(dir, rule_id)?;
    if !path.exists() {
        return Err(AutomationError::UnknownRule {
            rule_id: rule_id.to_string(),
        });
    }
    read_rule(&path)
}

/// Lists every rule saved under `dir`. An empty `Vec`, not an error, if the
/// directory doesn't exist yet — same "absence means empty" convention
/// `assets::io::list_assets`/`templates::io::list_custom_templates` use.
/// Sorted by name for a stable listing order.
pub fn list_rules(dir: &Path) -> Result<Vec<AutomationRule>, AutomationError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir).map_err(|e| AutomationError::IoFailed {
        details: format!("could not read {}: {e}", dir.display()),
    })?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| AutomationError::IoFailed {
            details: e.to_string(),
        })?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // A rule's own file is exactly `<id>.json` — this skips
        // `<id>.fire_log.json` (and any stray `.tmp`) without special-casing
        // the suffix by string length.
        if !file_name.ends_with(".json") || file_name.ends_with(".fire_log.json") {
            continue;
        }
        out.push(read_rule(&path)?);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Removes a rule's file (and its fire log, if any — best-effort, a missing
/// or already-gone log is not an error) from `dir`. Errors with
/// `AutomationError::UnknownRule` if no such rule file exists.
pub fn delete_rule(dir: &Path, rule_id: &str) -> Result<(), AutomationError> {
    let path = rule_file_path(dir, rule_id)?;
    if !path.exists() {
        return Err(AutomationError::UnknownRule {
            rule_id: rule_id.to_string(),
        });
    }
    fs::remove_file(&path).map_err(|e| AutomationError::IoFailed {
        details: format!("could not remove {}: {e}", path.display()),
    })?;
    if let Ok(log_path) = fire_log_path(dir, rule_id) {
        let _ = fs::remove_file(log_path);
    }
    Ok(())
}

// -- Fire log: a flat JSON array per rule, mirroring
//    `templates::io::append_template_history`/`list_template_history` -------

/// Every fire record recorded so far for `rule_id`, oldest first. An empty
/// `Vec`, not an error, if no fire log exists yet (a rule that has never
/// fired).
pub fn list_fire_records(
    dir: &Path,
    rule_id: &str,
) -> Result<Vec<RuleFireRecord>, AutomationError> {
    let path = fire_log_path(dir, rule_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|e| AutomationError::IoFailed {
        details: format!("could not read {}: {e}", path.display()),
    })?;
    serde_json::from_slice(&bytes).map_err(|e| AutomationError::CorruptJson {
        details: format!("{}: {e}", path.display()),
    })
}

/// Appends `record` onto its rule's fire log, creating the file if this is
/// the rule's first-ever recorded firing.
pub fn append_fire_record(dir: &Path, record: &RuleFireRecord) -> Result<(), AutomationError> {
    fs::create_dir_all(dir).map_err(|e| AutomationError::IoFailed {
        details: format!("could not create {}: {e}", dir.display()),
    })?;
    let path = fire_log_path(dir, &record.rule_id)?;
    let mut records = list_fire_records(dir, &record.rule_id)?;
    records.push(record.clone());
    write_json_atomic(&path, &records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::{AutomationAction, AutomationCondition, AutomationTrigger};
    use crate::batch::BatchPipelineConfig;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-automation-io-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config() -> BatchPipelineConfig {
        BatchPipelineConfig {
            remove_silence: None,
            captions: None,
            transcription_model_id: None,
            transcription_language: None,
            template_id: None,
            export_preset_id: Some("p1080".to_string()),
            output_suffix: None,
        }
    }

    fn sample_rule(watch_dir: &Path) -> AutomationRule {
        crate::automation::new_rule(
            "Auto-process long videos".to_string(),
            AutomationTrigger::WatchFolder {
                path: watch_dir.to_string_lossy().to_string(),
            },
            Some(AutomationCondition::MinDurationSeconds { min_seconds: 300.0 }),
            AutomationAction::RunPipeline {
                config: config(),
                template_ids: Some(vec!["tmpl_tiktok".to_string()]),
            },
        )
        .expect("sample rule")
    }

    #[test]
    fn save_then_list_then_delete_rule_round_trips() {
        let root = temp_dir("save-list-delete");
        let watch_dir = root.join("watch");
        fs::create_dir_all(&watch_dir).unwrap();
        let rules_dir = root.join("automation_rules");

        let rule = sample_rule(&watch_dir);
        let path = save_rule(&rules_dir, &rule).expect("save");
        assert!(path.exists());
        assert!(
            !path.with_extension("json.tmp").exists(),
            "no leftover .tmp"
        );

        let listed = list_rules(&rules_dir).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, rule.id);
        assert_eq!(listed[0].name, rule.name);
        assert_eq!(listed[0].condition, rule.condition);
        assert_eq!(listed[0].action, rule.action);

        let loaded = load_rule(&rules_dir, &rule.id).expect("load");
        assert_eq!(loaded, rule);

        delete_rule(&rules_dir, &rule.id).expect("delete");
        let listed_after = list_rules(&rules_dir).expect("list after delete");
        assert!(listed_after.is_empty());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn list_rules_on_a_missing_directory_is_an_empty_vec_not_an_error() {
        let dir = std::env::temp_dir().join(format!(
            "ave-automation-io-missing-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(!dir.exists());
        let listed = list_rules(&dir).expect("list on missing dir");
        assert!(listed.is_empty());
    }

    #[test]
    fn loading_and_deleting_an_unknown_rule_id_errors() {
        let dir = temp_dir("unknown");
        assert!(matches!(
            load_rule(&dir, "does_not_exist").unwrap_err(),
            AutomationError::UnknownRule { .. }
        ));
        assert!(matches!(
            delete_rule(&dir, "does_not_exist").unwrap_err(),
            AutomationError::UnknownRule { .. }
        ));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn saving_a_rule_with_a_path_traversal_id_is_rejected_not_written_outside_dir() {
        let dir = temp_dir("traversal-save");
        let marker_name = format!("ave-automation-io-escaped-{}", uuid::Uuid::new_v4());
        let would_be_outside_path = dir.parent().unwrap().join(format!("{marker_name}.json"));

        let mut rule = sample_rule(&dir);
        rule.id = format!("../{marker_name}");

        let err = save_rule(&dir, &rule).unwrap_err();
        assert!(matches!(err, AutomationError::UnsafeRuleId { .. }));
        assert!(
            !would_be_outside_path.exists(),
            "a traversal id must never write outside the automation_rules directory"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn importing_corrupt_json_errors() {
        let dir = temp_dir("corrupt");
        fs::write(dir.join("broken.json"), b"not json").unwrap();
        let err = load_rule(&dir, "broken").unwrap_err();
        assert!(matches!(err, AutomationError::CorruptJson { .. }));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_rules_skips_fire_log_files_and_stray_tmp_files() {
        let dir = temp_dir("skip-fire-log");
        let rule = sample_rule(&dir);
        save_rule(&dir, &rule).unwrap();
        append_fire_record(
            &dir,
            &RuleFireRecord {
                rule_id: rule.id.clone(),
                file_path: "clip.mp4".to_string(),
                occurred_at: "2026-09-06T00:00:00Z".to_string(),
                condition_passed: true,
                batch_id: Some("batch1".to_string()),
                job_ids: vec!["job1".to_string()],
            },
        )
        .unwrap();
        fs::write(dir.join("stray.json.tmp"), b"leftover").unwrap();

        let listed = list_rules(&dir).unwrap();
        assert_eq!(
            listed.len(),
            1,
            "the fire log/.tmp files must not be listed as rules"
        );
        assert_eq!(listed[0].id, rule.id);
        fs::remove_dir_all(&dir).ok();
    }

    // -- Fire log round trip --------------------------------------------------

    #[test]
    fn fire_log_starts_empty_then_appends_in_order() {
        let dir = temp_dir("fire-log");
        let rule_id = "rule_test1";

        assert!(list_fire_records(&dir, rule_id).unwrap().is_empty());

        let first = RuleFireRecord {
            rule_id: rule_id.to_string(),
            file_path: "/watch/clip1.mp4".to_string(),
            occurred_at: "2026-09-06T00:00:00Z".to_string(),
            condition_passed: true,
            batch_id: Some("batch1".to_string()),
            job_ids: vec!["job1".to_string()],
        };
        let second = RuleFireRecord {
            rule_id: rule_id.to_string(),
            file_path: "/watch/clip2.mp4".to_string(),
            occurred_at: "2026-09-06T00:05:00Z".to_string(),
            condition_passed: false,
            batch_id: None,
            job_ids: Vec::new(),
        };
        append_fire_record(&dir, &first).unwrap();
        append_fire_record(&dir, &second).unwrap();

        let records = list_fire_records(&dir, rule_id).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], first, "oldest first");
        assert_eq!(records[1], second);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_a_rule_also_removes_its_fire_log() {
        let dir = temp_dir("delete-with-log");
        let rule = sample_rule(&dir);
        save_rule(&dir, &rule).unwrap();
        append_fire_record(
            &dir,
            &RuleFireRecord {
                rule_id: rule.id.clone(),
                file_path: "clip.mp4".to_string(),
                occurred_at: "2026-09-06T00:00:00Z".to_string(),
                condition_passed: true,
                batch_id: Some("batch1".to_string()),
                job_ids: vec!["job1".to_string()],
            },
        )
        .unwrap();
        let log_path = fire_log_path(&dir, &rule.id).unwrap();
        assert!(log_path.exists());

        delete_rule(&dir, &rule.id).unwrap();
        assert!(!log_path.exists(), "the fire log should be removed too");

        fs::remove_dir_all(&dir).ok();
    }
}
