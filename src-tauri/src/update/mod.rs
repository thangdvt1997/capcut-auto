//! Auto-update settings/result types (Phase 12, master prompt §62: "Prepare
//! architecture for application updates... Do not force-update users while
//! they are editing... Never update while rendering.").
//!
//! The actual check/download/install/signature-verification machinery is
//! `tauri-plugin-updater` itself (registered in `lib.rs`, paired with
//! `tauri-plugin-process` for the restart-after-install action) — Tauri's
//! own official, idiomatic updater mechanism, not a custom-built HTTP
//! polling system. This module holds only the two small, specta-typed
//! pieces the frontend needs: the three-way update-mode setting, and the
//! closed outcome enum `commands::update::check_for_update`/
//! `install_available_update` report back. See `commands::update`'s own doc
//! comment for the "never update mid-render" enforcement (the part of this
//! feature that has real logic worth testing) and for exactly which
//! `tauri.conf.json` fields are still human-fill-in placeholders.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The three update-check behaviors master prompt §62 requires, named
/// exactly as it lists them ("Automatically check" / "Notify only" /
/// "Disabled"). Persisted the same way every other non-project, app-level
/// setting in this codebase is — `localStorage`, via a small frontend store
/// mirroring `stores/aiSettings.svelte.ts`'s/`stores/capcut.svelte.ts`'s own
/// precedent (no backend settings-persistence surface exists yet, same gap
/// those stores' own doc comments already note). This backend enum exists
/// so that value still round-trips through `check_for_update`/
/// `install_available_update` with real specta-typed correctness (this
/// phase's own required test) instead of as a bare untyped string, and so
/// the backend independently refuses to check/install when `Disabled` is
/// selected even if a future frontend bug ever got that gating wrong —
/// defense in depth, not the only place this is enforced.
///
/// What each mode actually changes, by design (documented here since the
/// master prompt itself only names the three options, not their exact
/// behavior): `AutomaticallyCheck` has the frontend call `check_for_update`
/// once on startup (in addition to the manual "Check for Updates Now"
/// button); `NotifyOnly` never checks on its own — only the manual button
/// checks, and only ever *notifies* via the status display, never
/// auto-installs; `Disabled` disables the button entirely and this enum's
/// own backend gate short-circuits before any network access. No mode ever
/// auto-installs without an explicit further user action — master prompt
/// §62 names three *checking* behaviors, not an auto-install behavior, and
/// "never updating mid-render" would be meaningless to enforce if an update
/// could silently install itself the moment a check succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateCheckMode {
    AutomaticallyCheck,
    NotifyOnly,
    Disabled,
}

/// What `commands::update::check_for_update`/`install_available_update`
/// report back — a closed, specifically-named outcome (rather than a loose
/// `available: bool`) so the frontend can render one unambiguous status
/// line, and so the mid-render deferral case has its own distinct,
/// real-testable variant instead of being folded into a generic error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateCheckOutcome {
    /// `UpdateCheckMode::Disabled` is selected — no network request was
    /// made.
    Disabled,
    /// A real check against the update manifest endpoint found no newer
    /// version (or, honestly, in this build: the endpoint is still an
    /// unconfigured placeholder — see `lib.rs`'s plugin-registration
    /// comment — so this variant in practice only appears once a human has
    /// filled in a real `endpoints`/`pubkey`).
    UpToDate,
    /// A newer version exists but at least one render or batch job is
    /// currently running — installing is deferred until it's safe (master
    /// prompt §62: "Never update while rendering"). Nothing was downloaded
    /// or installed.
    Deferred {
        version: String,
        notes: Option<String>,
    },
    /// A newer version exists and nothing is currently running — safe to
    /// install; `install_available_update` performs the real download,
    /// signature verification, and restart-to-apply.
    Available {
        version: String,
        notes: Option<String>,
    },
    /// The update endpoint could not be reached, or returned something the
    /// updater couldn't parse/verify — carries the real underlying error.
    /// This is the outcome every check honestly produces today, since the
    /// endpoint is still a documented placeholder with zero configured
    /// endpoints (no network request is ever actually attempted).
    CheckFailed { message: String },
    /// `install_available_update` only: the download + signature
    /// verification succeeded and the platform installer has been launched
    /// (Windows: the app process exits itself right after this to hand off
    /// to the installer, so a caller should not expect to observe this
    /// variant in practice — it exists for the non-Windows/mobile code
    /// paths where `tauri-plugin-updater`'s `install()` returns instead of
    /// exiting the process directly).
    Installing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_check_mode_serializes_to_the_master_prompts_three_snake_case_options() {
        assert_eq!(
            serde_json::to_string(&UpdateCheckMode::AutomaticallyCheck).unwrap(),
            "\"automatically_check\""
        );
        assert_eq!(
            serde_json::to_string(&UpdateCheckMode::NotifyOnly).unwrap(),
            "\"notify_only\""
        );
        assert_eq!(
            serde_json::to_string(&UpdateCheckMode::Disabled).unwrap(),
            "\"disabled\""
        );
    }

    #[test]
    fn update_check_mode_round_trips_through_json() {
        for mode in [
            UpdateCheckMode::AutomaticallyCheck,
            UpdateCheckMode::NotifyOnly,
            UpdateCheckMode::Disabled,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            let back: UpdateCheckMode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn update_check_outcome_variants_round_trip_through_json() {
        let outcomes = vec![
            UpdateCheckOutcome::Disabled,
            UpdateCheckOutcome::UpToDate,
            UpdateCheckOutcome::Deferred {
                version: "1.2.3".to_string(),
                notes: Some("bugfixes".to_string()),
            },
            UpdateCheckOutcome::Available {
                version: "1.2.3".to_string(),
                notes: None,
            },
            UpdateCheckOutcome::CheckFailed {
                message: "network error".to_string(),
            },
            UpdateCheckOutcome::Installing,
        ];
        for outcome in outcomes {
            let json = serde_json::to_string(&outcome).unwrap();
            let back: UpdateCheckOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(back, outcome);
        }
    }
}
