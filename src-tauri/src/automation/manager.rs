//! `RuleWatcherManager` — Tauri-managed state tracking one real, live
//! `notify` watcher per currently-enabled `WatchFolder` rule, mirroring how
//! `batch::BatchJobManager`/`db::MediaLibrary` are already registered as
//! Tauri-managed state (`lib.rs`'s `run()` setup). The `AppHandle`-dependent
//! half of Smart Automation (upgrade spec §27) — `automation::watcher` holds
//! the real, `AppHandle`-free watch/debounce primitive this module builds on
//! top of.
//!
//! Starting/stopping a watcher is keyed by `AutomationRule::id`:
//! - `start_watch` is called whenever a rule transitions to "should be
//!   watching" — right after `create_automation_rule` (if `enabled`),
//!   `set_automation_rule_enabled(_, true)`, or `update_automation_rule` on
//!   an already-enabled rule (which always restarts, below).
//! - `stop_watch` is called whenever a rule transitions to "should not be
//!   watching" — `set_automation_rule_enabled(_, false)` or
//!   `delete_automation_rule` — and simply drops the tracked
//!   `RecommendedWatcher`, which is `notify`'s own real signal to tear down
//!   the underlying OS watch (no custom stop logic needed).
//! - `update_automation_rule` always calls `stop_watch` then (if still
//!   `enabled`) `start_watch` again, even when the trigger's `path` itself
//!   didn't change — the live watcher's callback closure captured a
//!   snapshot of the *whole* rule (`condition`/`action` included) at the
//!   moment it was started, so an update to, say, `condition` or `action`
//!   would otherwise never take effect on an already-running watch.
//!
//! On app startup, [`RuleWatcherManager::hydrate`] re-starts every enabled
//! `WatchFolder` rule persisted on disk — mirroring how this codebase's
//! other startup-time re-hydration already works (`commands::media::init_media_library`
//! opening the same database file every run); a rule's watcher is otherwise
//! purely in-memory (like `BatchJobManager`'s own job registry) and would
//! not survive an app restart without this.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use notify::RecommendedWatcher;
use tauri::{AppHandle, Manager};

use crate::batch::{self, BatchJobManager};

use super::error::AutomationError;
use super::{
    condition_passes, io, watcher, AutomationAction, AutomationRule, AutomationTrigger,
    RuleFireRecord,
};

#[derive(Default)]
pub struct RuleWatcherManager {
    watchers: Mutex<HashMap<String, RecommendedWatcher>>,
}

impl RuleWatcherManager {
    /// Starts a real `notify` watch for `rule`'s `WatchFolder` trigger,
    /// tracked under `rule.id`. If a watcher for this id already exists, it
    /// is replaced (the old one is dropped — its OS watch is torn down —
    /// before the new one is inserted), so calling this twice for the same
    /// rule is safe and idempotent rather than accumulating watchers.
    pub fn start_watch(&self, app: AppHandle, rule: AutomationRule) -> Result<(), AutomationError> {
        let AutomationTrigger::WatchFolder { path } = &rule.trigger;
        let folder = Path::new(path).to_path_buf();
        let rule_for_callback = rule.clone();
        let app_for_callback = app.clone();
        let new_watcher = watcher::watch_folder(&folder, move |arrived_path| {
            handle_arrived_file(&app_for_callback, &rule_for_callback, &arrived_path);
        })
        .map_err(|e| AutomationError::WatchFailed {
            path: path.clone(),
            details: e.to_string(),
        })?;

        self.watchers
            .lock()
            .expect("rule watchers mutex poisoned")
            .insert(rule.id.clone(), new_watcher);
        Ok(())
    }

    /// Stops (drops) the tracked watcher for `rule_id`, if any. A no-op —
    /// not an error — if this rule has no live watcher (already stopped,
    /// never started, or a non-`WatchFolder` trigger).
    pub fn stop_watch(&self, rule_id: &str) {
        self.watchers
            .lock()
            .expect("rule watchers mutex poisoned")
            .remove(rule_id);
    }

    /// Re-starts every enabled `WatchFolder` rule persisted under
    /// `rules_dir` (module doc comment). Best-effort per rule: a rule whose
    /// watch folder no longer exists (moved/deleted while the app was
    /// closed) logs a warning and is simply skipped rather than blocking
    /// every other rule's own hydration, or app startup itself.
    pub fn hydrate(&self, app: &AppHandle, rules_dir: &Path) {
        let rules = match io::list_rules(rules_dir) {
            Ok(rules) => rules,
            Err(e) => {
                tracing::warn!("failed to list automation rules at startup: {e}");
                return;
            }
        };
        for rule in rules {
            if !rule.enabled {
                continue;
            }
            let rule_id = rule.id.clone();
            if let Err(e) = self.start_watch(app.clone(), rule) {
                tracing::warn!("failed to start automation rule {rule_id} watcher at startup: {e}");
            }
        }
    }
}

/// Real callback behind a `WatchFolder` trigger firing (module doc
/// comment): probes the arrived file's real duration, checks it against
/// `rule.condition` (`condition_passes`), and — only if it passes — runs the
/// existing batch pipeline against it (`rule.action`'s own
/// `RunPipeline.config`/`template_ids`, `batch::manager::start_batch`/
/// `start_multi_template_batch`, reused verbatim). Every outcome (probe
/// failure, condition failure, or a real fired batch) is recorded as one
/// `RuleFireRecord` (`automation` module doc comment's "Rule execution log"
/// section) — best-effort: a failure to *record* the firing is logged
/// (`tracing::warn!`) and never allowed to affect whether the pipeline
/// itself actually ran, matching `batch::manager::record_history_for_job`'s
/// own "recording must never be why real work appears to have failed"
/// discipline.
fn handle_arrived_file(app: &AppHandle, rule: &AutomationRule, file_path: &Path) {
    let file_path_str = file_path.to_string_lossy().to_string();
    let occurred_at = crate::project::now_rfc3339();

    let duration_us = match probe_duration(app, file_path) {
        Ok(us) => us,
        Err(details) => {
            tracing::warn!(
                "automation rule {} failed to probe arrived file {}: {details}",
                rule.id,
                file_path_str
            );
            record_fire(
                app,
                rule.id.clone(),
                file_path_str,
                occurred_at,
                false,
                None,
                Vec::new(),
            );
            return;
        }
    };

    if !condition_passes(rule.condition.as_ref(), duration_us) {
        record_fire(
            app,
            rule.id.clone(),
            file_path_str,
            occurred_at,
            false,
            None,
            Vec::new(),
        );
        return;
    }

    let AutomationAction::RunPipeline {
        config,
        template_ids,
    } = &rule.action;
    let manager = app.state::<BatchJobManager>();
    let (batch_id, job_ids) = match template_ids {
        Some(ids) if !ids.is_empty() => {
            match batch::manager::start_multi_template_batch(
                app.clone(),
                &manager,
                vec![file_path_str.clone()],
                ids.clone(),
                config.clone(),
            ) {
                Ok(batch_id) => {
                    let job_ids = job_ids_for_batch(&manager, &batch_id);
                    (Some(batch_id), job_ids)
                }
                Err(e) => {
                    tracing::warn!(
                        "automation rule {} failed to start a multi-template batch for {}: {e}",
                        rule.id,
                        file_path_str
                    );
                    (None, Vec::new())
                }
            }
        }
        _ => {
            let batch_id = batch::manager::start_batch(
                app.clone(),
                &manager,
                vec![file_path_str.clone()],
                config.clone(),
            );
            let job_ids = job_ids_for_batch(&manager, &batch_id);
            (Some(batch_id), job_ids)
        }
    };

    record_fire(
        app,
        rule.id.clone(),
        file_path_str,
        occurred_at,
        true,
        batch_id,
        job_ids,
    );
}

fn job_ids_for_batch(manager: &BatchJobManager, batch_id: &str) -> Vec<String> {
    manager
        .list_jobs(batch_id)
        .map(|jobs| jobs.into_iter().map(|j| j.id).collect())
        .unwrap_or_default()
}

fn probe_duration(app: &AppHandle, path: &Path) -> Result<i64, String> {
    let resource_dir = app.path().resource_dir().ok();
    let ffprobe = crate::ffmpeg::binaries::ffprobe_path(resource_dir.as_deref())
        .map_err(|e| e.to_string())?;
    crate::media::probe::probe(&ffprobe, path)
        .map(|probed| probed.duration_us)
        .map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
fn record_fire(
    app: &AppHandle,
    rule_id: String,
    file_path: String,
    occurred_at: String,
    condition_passed: bool,
    batch_id: Option<String>,
    job_ids: Vec<String>,
) {
    let dir = match crate::commands::automation::automation_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            tracing::warn!("failed to resolve automation rules directory for fire log: {e}");
            return;
        }
    };
    let record = RuleFireRecord {
        rule_id: rule_id.clone(),
        file_path,
        occurred_at,
        condition_passed,
        batch_id,
        job_ids,
    };
    if let Err(e) = io::append_fire_record(&dir, &record) {
        tracing::warn!("failed to append automation fire record for rule {rule_id}: {e}");
    }
}
