//! Persistence for `max_concurrent_jobs` — the worker-pool-size setting
//! (Phase D11, `STUDIO_PLAN.md`, closing Phase D4a's own explicitly-flagged
//! gap: "no settings-persistence UI exists yet for this value").
//!
//! ## Why this can't just be a `localStorage`-only frontend setting, like
//! every other app-level preference in this codebase
//!
//! `stores/aiSettings.svelte.ts`/`stores/capcut.svelte.ts`/
//! `stores/updateSettings.svelte.ts` each document the same thing: "there is
//! no backend settings-persistence surface yet, so this is
//! `localStorage`-only" — every other app-level setting in this app is read
//! and written entirely by the frontend, with the backend never needing to
//! know its value until a command call passes it in explicitly.
//!
//! This setting is the one genuine exception, for a real architectural
//! reason, not a stylistic one: `batch::manager::spawn_worker_pool` is
//! called exactly **once**, from `lib.rs`'s `setup` hook — *before* the
//! webview/frontend has loaded, let alone had a chance to read
//! `localStorage` and call a command with the value. A value that only ever
//! lives in the browser's own `localStorage` would not exist yet at the
//! exact moment Rust needs it. So `max_concurrent_jobs` needs a real,
//! backend-side, on-disk file Rust itself can read at startup — this module
//! is that file's real I/O.
//!
//! ## Storage
//!
//! A single JSON file, `$APPLOCALDATA/batch_settings.json` —
//! `{ "max_concurrent_jobs": <usize> }` — using the exact same
//! atomic-write-via-temp-file-then-rename convention `automation::io`/
//! `templates::io`/`assets::io` already use for their own on-disk state
//! (`automation::io::write_json_atomic`'s own doc comment), just inlined
//! here for one whole-file scalar value instead of imported for a
//! one-file-per-item collection.
//!
//! Every function here is `AppHandle`-free, taking the real settings-file
//! path directly — the same "pure, directly-unit-testable core; a thin
//! `AppHandle`-dependent path-resolution wrapper lives in the `commands`
//! layer" split every other piece of on-disk state in this codebase already
//! uses (`commands::assets::assets_dir`/`commands::automation::automation_dir`
//! resolve the real directory; `automation::io`'s own functions never see an
//! `AppHandle`). `commands::batch::batch_settings_file` is that wrapper for
//! this module.
//!
//! ## Read vs. write: different honesty postures, on purpose
//!
//! [`load_max_concurrent_jobs`] is deliberately best-effort: a missing file,
//! corrupt JSON, or an out-of-bounds stored value all just fall back to
//! [`crate::batch::manager::DEFAULT_MAX_CONCURRENT_JOBS`] rather than ever
//! failing app startup over a settings file a user could have hand-edited or
//! deleted — matching every other "absence/corruption means use the honest
//! default" read path in this codebase (`automation::io::list_rules`'s own
//! "missing dir -> empty Vec, not an error" doc comment).
//!
//! [`save_max_concurrent_jobs`] is the opposite: a real validation or I/O
//! failure is returned to the caller, never swallowed — silently discarding
//! an explicit settings change the user just made would be exactly the kind
//! of "looks like it worked but didn't" dishonesty this codebase's own
//! discipline forbids.
//!
//! ## Why this is a restart-to-apply setting, not a live-resizable one
//!
//! See `STUDIO_PLAN.md`'s "Phase D11" section for the full reasoning. In
//! short: `batch::manager::pop_blocking`'s only cooperative-shutdown signal
//! (`BatchJobManager::shutdown`) is a single, global, `#[cfg(test)]`-only
//! flag that stops **every** worker at once — there is no existing mechanism
//! to make exactly N of the currently-running M workers exit while leaving
//! the rest running, and building one would mean changing `pop_blocking`'s
//! own condvar loop, which this task was explicitly told not to touch.
//! Growing the pool live (spawning extra threads) would have been safe and
//! easy; shrinking it live would not — and a setting that only works in one
//! direction is worse than one that honestly always says "restart to
//! apply". So `set_max_concurrent_jobs` only ever persists a value here for
//! the *next* startup's `spawn_worker_pool` call (`lib.rs`) to read; it never
//! touches the currently-running pool.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::BatchError;
use super::manager::DEFAULT_MAX_CONCURRENT_JOBS;

/// Real, sane bounds for a value this app will actually spawn that many real
/// OS worker threads (each capable of running a real ffmpeg/whisper
/// subprocess) for — not "unlimited". Matches real CPU core counts on the
/// hardware this app targets (task brief: "1 to some sane cap like 8 or
/// 16").
pub const MIN_MAX_CONCURRENT_JOBS: usize = 1;
pub const MAX_MAX_CONCURRENT_JOBS: usize = 16;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct StoredBatchSettings {
    max_concurrent_jobs: usize,
}

/// The one real, shared bounds check — used by both [`save_max_concurrent_jobs`]
/// and [`load_max_concurrent_jobs`] (to reject a stored value that's somehow
/// out of range, e.g. hand-edited), so "what counts as valid" is defined in
/// exactly one place.
pub fn validate_max_concurrent_jobs(value: usize) -> Result<(), BatchError> {
    if (MIN_MAX_CONCURRENT_JOBS..=MAX_MAX_CONCURRENT_JOBS).contains(&value) {
        Ok(())
    } else {
        Err(BatchError::InvalidMaxConcurrentJobs {
            value,
            min: MIN_MAX_CONCURRENT_JOBS,
            max: MAX_MAX_CONCURRENT_JOBS,
        })
    }
}

/// Reads the persisted `max_concurrent_jobs` from `path`, falling back to
/// [`DEFAULT_MAX_CONCURRENT_JOBS`] for a missing file, unreadable file,
/// corrupt JSON, or an in-range-violating stored value — see module doc
/// comment for why this read path is deliberately best-effort rather than an
/// error `lib.rs`'s own `setup` hook would have to handle.
pub fn load_max_concurrent_jobs(path: &Path) -> usize {
    let Ok(bytes) = fs::read(path) else {
        return DEFAULT_MAX_CONCURRENT_JOBS;
    };
    let Ok(stored) = serde_json::from_slice::<StoredBatchSettings>(&bytes) else {
        return DEFAULT_MAX_CONCURRENT_JOBS;
    };
    if validate_max_concurrent_jobs(stored.max_concurrent_jobs).is_ok() {
        stored.max_concurrent_jobs
    } else {
        DEFAULT_MAX_CONCURRENT_JOBS
    }
}

/// Validates `value`, then atomically persists it to `path` — the same
/// temp-file-then-rename-plus-`fsync` convention
/// `automation::io::write_json_atomic` uses, inlined here since this module
/// has exactly one file to ever write, not a collection worth importing a
/// shared helper for. Creates `path`'s parent directory if it doesn't exist
/// yet (a fresh install may never have created `$APPLOCALDATA` for this app
/// at all).
pub fn save_max_concurrent_jobs(path: &Path, value: usize) -> Result<(), BatchError> {
    validate_max_concurrent_jobs(value)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| BatchError::SettingsIoFailed {
            details: format!("could not create {}: {e}", parent.display()),
        })?;
    }

    let stored = StoredBatchSettings {
        max_concurrent_jobs: value,
    };
    let json = serde_json::to_vec_pretty(&stored).map_err(|e| BatchError::SettingsIoFailed {
        details: format!("serialize failed: {e}"),
    })?;

    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp_path).map_err(|e| BatchError::SettingsIoFailed {
            details: format!("could not create {}: {e}", tmp_path.display()),
        })?;
        file.write_all(&json)
            .map_err(|e| BatchError::SettingsIoFailed {
                details: format!("write failed: {e}"),
            })?;
        file.sync_all().map_err(|e| BatchError::SettingsIoFailed {
            details: format!("fsync failed: {e}"),
        })?;
    }

    fs::rename(&tmp_path, path).map_err(|e| BatchError::SettingsIoFailed {
        details: format!(
            "rename {} -> {} failed: {e}",
            tmp_path.display(),
            path.display()
        ),
    })
}

/// A real, live snapshot of this setting (`commands::batch::get_max_concurrent_jobs`/
/// `set_max_concurrent_jobs`) — carries both the value persisted to disk
/// (what the *next* app startup will use) and the value the
/// currently-running worker pool was actually spawned with
/// (`BatchJobManager::worker_pool_status().workers`), plus the real bounds
/// the frontend's numeric input should enforce. Having both numbers in one
/// place is what lets the UI honestly show "this won't take effect until you
/// restart" exactly when (and only when) the two actually differ, rather
/// than always displaying a blanket disclaimer or silently hiding the
/// distinction.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub struct MaxConcurrentJobsSetting {
    /// The value persisted to `batch_settings.json` — what the *next* app
    /// startup's `spawn_worker_pool` call will use.
    pub persisted: usize,
    /// The value the currently-running worker pool was actually spawned
    /// with. May differ from `persisted` immediately after a change, until
    /// the app is restarted (Phase D11: a real, deliberate restart-to-apply
    /// setting — see this module's own doc comment for why).
    pub active: usize,
    pub min: usize,
    pub max: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ave-batch-settings-test-{label}-{}.json",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn validate_accepts_the_full_inclusive_range() {
        assert!(validate_max_concurrent_jobs(MIN_MAX_CONCURRENT_JOBS).is_ok());
        assert!(validate_max_concurrent_jobs(MAX_MAX_CONCURRENT_JOBS).is_ok());
        assert!(validate_max_concurrent_jobs(4).is_ok());
    }

    #[test]
    fn validate_rejects_zero_and_anything_past_the_cap() {
        assert!(matches!(
            validate_max_concurrent_jobs(0).unwrap_err(),
            BatchError::InvalidMaxConcurrentJobs { value: 0, .. }
        ));
        assert!(matches!(
            validate_max_concurrent_jobs(MAX_MAX_CONCURRENT_JOBS + 1).unwrap_err(),
            BatchError::InvalidMaxConcurrentJobs { .. }
        ));
    }

    #[test]
    fn load_on_a_missing_file_returns_the_honest_default() {
        let path = temp_settings_path("missing");
        assert!(!path.exists());
        assert_eq!(load_max_concurrent_jobs(&path), DEFAULT_MAX_CONCURRENT_JOBS);
    }

    #[test]
    fn load_on_corrupt_json_returns_the_honest_default_not_an_error() {
        let path = temp_settings_path("corrupt");
        fs::write(&path, b"not json").unwrap();
        assert_eq!(load_max_concurrent_jobs(&path), DEFAULT_MAX_CONCURRENT_JOBS);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn load_on_an_out_of_range_stored_value_returns_the_honest_default() {
        let path = temp_settings_path("out-of-range");
        fs::write(&path, br#"{"max_concurrent_jobs": 999}"#).unwrap();
        assert_eq!(load_max_concurrent_jobs(&path), DEFAULT_MAX_CONCURRENT_JOBS);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn save_then_load_round_trips_a_valid_value() {
        let path = temp_settings_path("round-trip");
        save_max_concurrent_jobs(&path, 7).expect("save");
        assert!(path.exists());
        assert!(
            !path.with_extension("json.tmp").exists(),
            "no leftover .tmp file after a successful save"
        );
        assert_eq!(load_max_concurrent_jobs(&path), 7);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn save_rejects_an_out_of_range_value_and_writes_nothing() {
        let path = temp_settings_path("reject");
        let err = save_max_concurrent_jobs(&path, 0).unwrap_err();
        assert!(matches!(err, BatchError::InvalidMaxConcurrentJobs { .. }));
        assert!(!path.exists(), "an invalid value must never be persisted");
    }

    #[test]
    fn save_overwrites_a_previously_saved_value() {
        let path = temp_settings_path("overwrite");
        save_max_concurrent_jobs(&path, 2).expect("first save");
        assert_eq!(load_max_concurrent_jobs(&path), 2);
        save_max_concurrent_jobs(&path, 8).expect("second save");
        assert_eq!(load_max_concurrent_jobs(&path), 8);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn save_creates_a_missing_parent_directory() {
        let root = std::env::temp_dir().join(format!(
            "ave-batch-settings-test-missing-parent-{}",
            uuid::Uuid::new_v4()
        ));
        assert!(!root.exists());
        let path = root.join("nested").join("batch_settings.json");
        save_max_concurrent_jobs(&path, 5).expect("save should create parent dirs");
        assert_eq!(load_max_concurrent_jobs(&path), 5);
        fs::remove_dir_all(&root).ok();
    }
}
