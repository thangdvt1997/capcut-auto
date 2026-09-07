//! Smart Automation (upgrade spec §27 / `UPGRADE_PLAN.md` Phase U4): a
//! minimal rule engine — `WHEN <trigger> IF <condition> THEN <action>` —
//! starting with exactly the one trigger/condition/action shape §27's own
//! worked example needs, per that section's own explicit instruction to
//! design this *extensibly* but *not over-engineer it beyond what today's
//! codebase actually needs* ("không over-engineer nếu codebase hiện tại
//! chưa cần"):
//!
//! - **Trigger**: [`AutomationTrigger::WatchFolder`] — "a new video appears
//!   in this folder". The one genuinely new capability here: nothing else in
//!   this codebase watches the filesystem (see `automation::watcher` module
//!   doc comment for the real `notify`-crate-backed implementation and its
//!   debounce design).
//! - **Condition**: [`AutomationCondition::MinDurationSeconds`] — a real
//!   ffprobe duration check (`media::probe`, reused directly, never
//!   re-implemented) against a threshold. `None` (no condition configured)
//!   always proceeds.
//! - **Action**: [`AutomationAction::RunPipeline`] — §27's own worked
//!   example ends in "CapCut edit -> export", which in this codebase's real,
//!   already-confirmed architecture (`UPGRADE_PLAN.md`'s "Explicitly out of
//!   scope" section: direct CapCut draft-file export, never GUI/RPA
//!   automation) *is* the existing batch pipeline
//!   (`batch::manager::start_batch`/`create_multi_template_batch` +
//!   `BatchPipelineConfig`) — reused verbatim here, not reinvented. No new
//!   CapCut-facing code exists anywhere in this module.
//!
//! Each of the three enums above is intentionally closed with exactly one
//! variant today — adding a second trigger/condition/action "for
//! completeness" was deliberately not done (per this pass's own task brief);
//! a future pass can add one the same way `assets::AssetKind`/
//! `templates::TransitionType` already show this codebase growing a closed
//! enum by one variant at a time, without redesigning the surrounding types.
//!
//! ## Storage
//!
//! One JSON file per rule, `$APPLOCALDATA/automation_rules/<id>.json`,
//! atomic temp-file-then-rename writes — the exact same convention
//! `assets::io`/`templates::io` already use (`automation::io` module doc
//! comment), not a fourth persistence mechanism.
//!
//! ## Rule execution log: what's real vs. reused for free
//!
//! §27 doesn't spell out a dedicated log format, but "did this rule actually
//! fire, for which file, and what did it start" is worth recording. Two
//! things were checked before adding anything:
//!
//! - **A fired rule's actual batch job(s) already get a full,
//!   already-shipped `history::HistoryEntry` row for free** (Phase U3, once
//!   that job reaches a terminal state) — `automation::manager` calls the
//!   exact same `batch::manager::start_batch`/`start_multi_template_batch`
//!   every other batch caller uses, and `run_job_with_events`'s own
//!   unconditional `record_history_for_job` hook does not care *who*
//!   started the batch. So a full second copy of input/output
//!   paths/timings/status/error for the *job itself* would be pure
//!   duplication of Video Processing History — not built here.
//! - **What History genuinely cannot answer**: "was this arrival even
//!   attempted, which rule was responsible, and did it clear the condition
//!   check" — a file that fails the `MinDurationSeconds` check never starts
//!   a batch job at all, so it would leave *no* trace anywhere without this.
//!   [`RuleFireRecord`] is the intentionally small, honest answer to
//!   exactly that gap: rule id, matched file path, timestamp, whether the
//!   condition passed, and — only when it did — the real batch/job id(s)
//!   that were actually started (a thin link into History, not a
//!   duplicate of it). Stored the same way `templates::io`'s own
//!   `<id>.history.json` version-history file already works: one flat JSON
//!   array per rule, appended to, atomic-written — `automation::io`'s own
//!   `append_fire_record`/`list_fire_records`, mirroring
//!   `templates::io::append_template_history`/`list_template_history`
//!   directly rather than inventing a new append-log convention.
//!
//! `list_fire_records` is real and unit-tested but not (yet) wired to a
//! Tauri command in this pass — no command surface in this task's own scope
//! asks for one, and there is no frontend consumer yet either (frontend for
//! Phase U4 is a later pass); a future pass can add a thin
//! `list_automation_rule_fire_log` command on top of this already-real
//! primitive in an afternoon, without touching this module again.

pub mod error;
pub mod io;
pub mod manager;
pub mod watcher;

pub use error::AutomationError;
pub use manager::RuleWatcherManager;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::batch::BatchPipelineConfig;

/// Closed trigger enum (module doc comment) — exactly one variant today.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationTrigger {
    /// Fires once per new video file that appears in `path` (upgrade spec
    /// §27's own worked example: "new video added to folder X"). Real
    /// implementation: `automation::watcher::watch_folder`.
    WatchFolder { path: String },
}

/// Closed condition enum (module doc comment) — exactly one variant today.
/// `AutomationRule::condition` being `None` (no condition configured at all)
/// always proceeds — this enum only ever represents a *configured* check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationCondition {
    /// Real ffprobe duration (`media::probe`), not an estimate — passes when
    /// the arrived file's real duration is `>=` this many seconds (upgrade
    /// spec §27's own worked example: "duration > 5 minutes"; this checks
    /// `>=`, matching this codebase's existing inclusive-threshold
    /// convention elsewhere, e.g. `SmartEditCategory` scoring).
    ///
    /// A struct variant (`{ min_seconds: f64 }`), not a tuple variant
    /// (`(f64)`) — serde's internally-tagged representation (`tag = "kind"`,
    /// matching `AutomationTrigger`/`AutomationAction`'s own convention)
    /// cannot serialize a newtype variant wrapping a bare, non-self-describing
    /// value like `f64` (there's no JSON object to embed the tag field
    /// into); a real `cargo test` run surfaced this exact serialization
    /// failure before this doc comment was written. A named field sidesteps
    /// it entirely and is more self-documenting at every call site besides.
    MinDurationSeconds { min_seconds: f64 },
}

/// Closed action enum (module doc comment) — exactly one variant today. The
/// entire action *sequence* §27's worked example describes (AI analyze ->
/// create shorts -> apply template -> CapCut edit -> export) collapses into
/// this one real, already-existing pipeline call — see module doc comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationAction {
    /// Runs the arrived file through the real batch pipeline. `template_ids`
    /// mirrors `commands::batch::start_multi_template_batch`'s own shape
    /// exactly: `Some(ids)` (1+ entries) fans this single file out across
    /// every listed template via `batch::manager::create_multi_template_batch`
    /// (one job per template, §11's own N x M naming convention, here with
    /// N = 1); `None` runs the plain single-template `batch::manager::start_batch`
    /// path using whatever `config.template_id` already carries (or none at
    /// all). Never a third, automation-specific render path.
    RunPipeline {
        config: BatchPipelineConfig,
        template_ids: Option<Vec<String>>,
    },
}

/// One persisted automation rule (module doc comment).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AutomationRule {
    pub id: String,
    pub name: String,
    /// Whether this rule's trigger is currently live-watching. Toggling this
    /// (via `set_automation_rule_enabled`) starts/stops the real
    /// `notify` watcher for this rule's `trigger`
    /// (`RuleWatcherManager::start_watch`/`stop_watch`) — a rule saved with
    /// `enabled: false` is inert: persisted, listed, editable, but not
    /// actually watching anything.
    pub enabled: bool,
    pub trigger: AutomationTrigger,
    /// `None` means "always proceed" — this rule has no configured
    /// condition at all, not merely a condition that always evaluates true.
    pub condition: Option<AutomationCondition>,
    pub action: AutomationAction,
    /// RFC3339, when this rule was created.
    pub created_at: String,
}

/// Validates a trigger before it's ever persisted (same "check real
/// existence before ever registering" discipline as `assets::new_asset`'s
/// own `Path::is_file()` check) — shared by [`new_rule`] and
/// `commands::automation::update_automation_rule` (whenever a caller
/// changes an existing rule's trigger).
pub fn validate_trigger(trigger: &AutomationTrigger) -> Result<(), AutomationError> {
    match trigger {
        AutomationTrigger::WatchFolder { path } => {
            if !std::path::Path::new(path).is_dir() {
                return Err(AutomationError::FolderNotFound { path: path.clone() });
            }
            Ok(())
        }
    }
}

/// Builds a new `AutomationRule` with a fresh `rule_<uuid>` id, `enabled:
/// true` by default (a newly created rule is live immediately — the same
/// "no separate activation step" posture every other create-then-use flow in
/// this codebase already has, e.g. a freshly-added `Asset` is usable right
/// away), after validating the trigger really is watchable
/// ([`validate_trigger`]).
pub fn new_rule(
    name: String,
    trigger: AutomationTrigger,
    condition: Option<AutomationCondition>,
    action: AutomationAction,
) -> Result<AutomationRule, AutomationError> {
    validate_trigger(&trigger)?;
    Ok(AutomationRule {
        id: format!("rule_{}", uuid::Uuid::new_v4()),
        name,
        enabled: true,
        trigger,
        condition,
        action,
        created_at: crate::project::now_rfc3339(),
    })
}

/// Real logic behind "IF <condition>": `None` (no condition configured)
/// always proceeds; `Some(MinDurationSeconds { min_seconds })` proceeds only
/// when the arrived file's real, ffprobe-measured `duration_us` converts to
/// at least `min_seconds` seconds. Pure and directly unit-testable against a
/// real probed duration — `automation::manager` is the one real caller, feeding it
/// `media::probe::probe`'s own actual output, never a mocked/estimated one.
pub fn condition_passes(condition: Option<&AutomationCondition>, duration_us: i64) -> bool {
    match condition {
        None => true,
        Some(AutomationCondition::MinDurationSeconds { min_seconds }) => {
            let duration_seconds = duration_us as f64 / 1_000_000.0;
            duration_seconds >= *min_seconds
        }
    }
}

/// One real record of a rule actually firing (module doc comment's "Rule
/// execution log" section).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct RuleFireRecord {
    pub rule_id: String,
    pub file_path: String,
    /// RFC3339, when this arrival was handled.
    pub occurred_at: String,
    /// Whether the file's real, probed duration cleared the rule's
    /// `condition` (always `true` when the rule has no condition
    /// configured).
    pub condition_passed: bool,
    /// `None` when `condition_passed` is `false` (the pipeline was never
    /// started) or when probing itself failed.
    pub batch_id: Option<String>,
    /// Always empty alongside a `None` `batch_id`.
    pub job_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ave-automation-mod-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
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

    // -- new_rule / validate_trigger -----------------------------------------

    #[test]
    fn new_rule_succeeds_for_a_real_existing_directory() {
        let dir = temp_dir("real-dir");
        let rule = new_rule(
            "My Rule".to_string(),
            AutomationTrigger::WatchFolder {
                path: dir.to_string_lossy().to_string(),
            },
            Some(AutomationCondition::MinDurationSeconds { min_seconds: 300.0 }),
            AutomationAction::RunPipeline {
                config: config(),
                template_ids: None,
            },
        )
        .expect("new_rule");
        assert!(rule.id.starts_with("rule_"));
        assert_eq!(rule.name, "My Rule");
        assert!(rule.enabled, "a freshly created rule is enabled by default");
        assert!(!rule.created_at.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn new_rule_rejects_a_nonexistent_watch_folder_path() {
        let dir = std::env::temp_dir().join(format!(
            "ave-automation-mod-test-missing-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(!dir.exists());
        let err = new_rule(
            "Bad Rule".to_string(),
            AutomationTrigger::WatchFolder {
                path: dir.to_string_lossy().to_string(),
            },
            None,
            AutomationAction::RunPipeline {
                config: config(),
                template_ids: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, AutomationError::FolderNotFound { .. }));
    }

    #[test]
    fn new_rule_rejects_a_file_not_a_directory_as_the_watch_path() {
        let dir = temp_dir("file-not-dir");
        let file_path = dir.join("not_a_folder.txt");
        std::fs::write(&file_path, b"x").unwrap();
        let err = new_rule(
            "Bad Rule".to_string(),
            AutomationTrigger::WatchFolder {
                path: file_path.to_string_lossy().to_string(),
            },
            None,
            AutomationAction::RunPipeline {
                config: config(),
                template_ids: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, AutomationError::FolderNotFound { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- condition_passes: pure logic -----------------------------------------

    #[test]
    fn condition_passes_always_true_with_no_condition_configured() {
        assert!(condition_passes(None, 0));
        assert!(condition_passes(None, 999_000_000));
    }

    #[test]
    fn condition_passes_checks_min_duration_seconds_inclusively() {
        let cond = AutomationCondition::MinDurationSeconds { min_seconds: 300.0 };
        assert!(!condition_passes(Some(&cond), 299_999_999), "just under");
        assert!(condition_passes(Some(&cond), 300_000_000), "exactly at");
        assert!(condition_passes(Some(&cond), 301_000_000), "over");
    }

    // -- condition_passes against a REAL probed duration (real ffmpeg/ffprobe,
    //    not mocked) ----------------------------------------------------------

    #[test]
    fn condition_passes_against_a_real_probed_duration() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = temp_dir("real-probe");
        let source = dir.join("clip.mp4");
        {
            use crate::ffmpeg::command::{run_checked, FfmpegArgs};
            let args = FfmpegArgs::new()
                .args([
                    "-y",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc=duration=2:size=320x240:rate=10",
                ])
                .path(&source);
            run_checked(&ffmpeg, &args).expect("synthesizing a real 2s test clip");
        }

        let probed = crate::media::probe::probe(&ffprobe, &source).expect("real ffprobe call");
        assert!(
            probed.duration_us > 1_500_000 && probed.duration_us < 2_500_000,
            "real probed duration should be close to the real 2s clip: {}",
            probed.duration_us
        );

        // A threshold clearly under the real 2s clip passes...
        assert!(condition_passes(
            Some(&AutomationCondition::MinDurationSeconds { min_seconds: 1.0 }),
            probed.duration_us
        ));
        // ...and a threshold clearly over it (five real minutes, matching
        // upgrade spec §27's own worked example) does not.
        assert!(!condition_passes(
            Some(&AutomationCondition::MinDurationSeconds { min_seconds: 300.0 }),
            probed.duration_us
        ));

        std::fs::remove_dir_all(&dir).ok();
    }
}
