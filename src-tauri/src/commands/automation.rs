//! Smart Automation Tauri command surface (upgrade spec §27,
//! `UPGRADE_PLAN.md` Phase U4). Thin per master prompt §66 — all real
//! rule-building/validation logic lives in `crate::automation`, all
//! persistence in `crate::automation::io`, and all live-watcher
//! start/stop/hydrate logic in `crate::automation::manager::RuleWatcherManager`.

use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::automation::{
    self, io as automation_io, AutomationAction, AutomationCondition, AutomationError,
    AutomationRule, AutomationTrigger, RuleWatcherManager,
};
use crate::error::AppErrorPayload;

/// Automation rule storage location: `$APPLOCALDATA/automation_rules/` — the
/// exact same this-app's-own-data-directory convention
/// `commands::assets::assets_dir`/`commands::templates::templates_dir` use.
///
/// `pub(crate)`, not private: `automation::manager::record_fire` resolves
/// this exact same directory to append a rule's fire log, rather than
/// duplicating the resolution logic.
pub(crate) fn automation_dir(app: &AppHandle) -> Result<PathBuf, AutomationError> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("automation_rules"))
        .map_err(|e| AutomationError::StorageUnavailable {
            details: format!("resolving app local data dir: {e}"),
        })
}

/// Lists every persisted automation rule.
#[tauri::command]
#[specta::specta]
pub fn list_automation_rules(app: AppHandle) -> Result<Vec<AutomationRule>, AppErrorPayload> {
    let dir = automation_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    automation_io::list_rules(&dir).map_err(|e| AppErrorPayload::from(&e))
}

/// Creates a new automation rule (`enabled: true` by default,
/// `automation::new_rule`'s own doc comment) and, since it starts enabled,
/// immediately starts its real `WatchFolder` watcher
/// (`RuleWatcherManager::start_watch`). If starting that watcher fails (a
/// real OS-level watch error, distinct from an invalid/nonexistent path,
/// which `automation::new_rule` already rejects before ever reaching here),
/// the just-saved rule file is removed again — this command never leaves a
/// rule persisted on disk that looks "enabled" but has no real watcher
/// behind it.
#[tauri::command]
#[specta::specta]
pub fn create_automation_rule(
    app: AppHandle,
    manager: State<'_, RuleWatcherManager>,
    name: String,
    trigger: AutomationTrigger,
    condition: Option<AutomationCondition>,
    action: AutomationAction,
) -> Result<AutomationRule, AppErrorPayload> {
    let rule = automation::new_rule(name, trigger, condition, action)
        .map_err(|e| AppErrorPayload::from(&e))?;
    let dir = automation_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    automation_io::save_rule(&dir, &rule).map_err(|e| AppErrorPayload::from(&e))?;

    if rule.enabled {
        if let Err(e) = manager.start_watch(app.clone(), rule.clone()) {
            let _ = automation_io::delete_rule(&dir, &rule.id);
            return Err(AppErrorPayload::from(&e));
        }
    }
    Ok(rule)
}

/// Updates an existing rule's `name`/`trigger`/`condition`/`action` in place
/// (`None` = leave that field unchanged). A new `trigger` is re-validated
/// the same way `create_automation_rule` validates one (real folder
/// existence) — never silently accepted.
///
/// Honest scope note: `condition` here can only be left-unchanged (`None`)
/// or replaced with a new configured condition (`Some(c)`) — there is no way
/// to explicitly clear an existing condition back to "always proceed"
/// through this command alone (that would need a second, separate flag to
/// disambiguate from "leave unchanged" without an ambiguous nested
/// `Option<Option<_>>` across the Tauri IPC boundary, where both would
/// arrive as a bare JSON `null`). Not needed by any caller in this pass
/// (there is no frontend yet); a caller wanting that today deletes and
/// recreates the rule.
///
/// Always restarts this rule's live watcher (stop then, if still `enabled`,
/// start again) — see `RuleWatcherManager` module doc comment for why: the
/// running watcher's callback closure captured a snapshot of the *whole*
/// rule when it was started, so any field change here (not just `trigger`)
/// needs a fresh watcher to actually take effect.
#[tauri::command]
#[specta::specta]
pub fn update_automation_rule(
    app: AppHandle,
    manager: State<'_, RuleWatcherManager>,
    rule_id: String,
    name: Option<String>,
    trigger: Option<AutomationTrigger>,
    condition: Option<AutomationCondition>,
    action: Option<AutomationAction>,
) -> Result<AutomationRule, AppErrorPayload> {
    let dir = automation_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let mut rule =
        automation_io::load_rule(&dir, &rule_id).map_err(|e| AppErrorPayload::from(&e))?;

    if let Some(name) = name {
        rule.name = name;
    }
    if let Some(trigger) = trigger {
        automation::validate_trigger(&trigger).map_err(|e| AppErrorPayload::from(&e))?;
        rule.trigger = trigger;
    }
    if let Some(condition) = condition {
        rule.condition = Some(condition);
    }
    if let Some(action) = action {
        rule.action = action;
    }

    automation_io::save_rule(&dir, &rule).map_err(|e| AppErrorPayload::from(&e))?;

    manager.stop_watch(&rule.id);
    if rule.enabled {
        if let Err(e) = manager.start_watch(app.clone(), rule.clone()) {
            // Roll back the just-saved `enabled: true` (and any other field
            // change) — mirrors `create_automation_rule`'s own "never leave a
            // rule persisted that looks enabled/updated but has no real
            // watcher behind it" discipline. Best-effort: a failure to roll
            // back is logged, not escalated, since the original watch-start
            // error is already the one being returned to the caller.
            rule.enabled = false;
            if let Err(rollback_err) = automation_io::save_rule(&dir, &rule) {
                tracing::warn!(
                    "failed to roll back rule {} to disabled after a watch-start failure: {rollback_err}",
                    rule.id
                );
            }
            return Err(AppErrorPayload::from(&e));
        }
    }
    Ok(rule)
}

/// Toggles a rule on/off — starts or stops its real `WatchFolder` watcher to
/// match (`RuleWatcherManager::start_watch`/`stop_watch`), on top of
/// persisting the new `enabled` value.
#[tauri::command]
#[specta::specta]
pub fn set_automation_rule_enabled(
    app: AppHandle,
    manager: State<'_, RuleWatcherManager>,
    rule_id: String,
    enabled: bool,
) -> Result<AutomationRule, AppErrorPayload> {
    let dir = automation_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let mut rule =
        automation_io::load_rule(&dir, &rule_id).map_err(|e| AppErrorPayload::from(&e))?;
    rule.enabled = enabled;
    automation_io::save_rule(&dir, &rule).map_err(|e| AppErrorPayload::from(&e))?;

    manager.stop_watch(&rule.id);
    if rule.enabled {
        if let Err(e) = manager.start_watch(app.clone(), rule.clone()) {
            // Same rollback discipline as `create_automation_rule`/
            // `update_automation_rule` — never leave `enabled: true`
            // persisted on disk with no real watcher actually behind it.
            rule.enabled = false;
            if let Err(rollback_err) = automation_io::save_rule(&dir, &rule) {
                tracing::warn!(
                    "failed to roll back rule {} to disabled after a watch-start failure: {rollback_err}",
                    rule.id
                );
            }
            return Err(AppErrorPayload::from(&e));
        }
    }
    Ok(rule)
}

/// Deletes a rule: stops its live watcher first (if any), then removes it
/// (and its fire log, `automation::io::delete_rule`'s own doc comment) from
/// disk.
#[tauri::command]
#[specta::specta]
pub fn delete_automation_rule(
    app: AppHandle,
    manager: State<'_, RuleWatcherManager>,
    rule_id: String,
) -> Result<(), AppErrorPayload> {
    manager.stop_watch(&rule_id);
    let dir = automation_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    automation_io::delete_rule(&dir, &rule_id).map_err(|e| AppErrorPayload::from(&e))
}
