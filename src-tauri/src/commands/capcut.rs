//! CapCut/Jianying installation-detection + process-control Tauri command
//! surface. Thin per master prompt §66 — all real logic lives in
//! `crate::capcut::{detect, validate}`.

use std::path::Path;

use crate::capcut::detect::{self, CapCutProduct, CapCutRegistryHint, DetectedCapCutInstallation};
use crate::capcut::error::CapCutError;
use crate::capcut::validate::{self, DraftValidationReport};
use crate::error::AppErrorPayload;

/// Detects every confirmed CapCut/Jianying draft-root installation on this
/// machine (filesystem-based — see `crate::capcut::detect` module doc
/// comment for the full heuristic and its honest limitations). Returns an
/// empty `Vec`, never an error, when nothing is found — "no installation
/// found" is an expected, legitimate outcome (e.g. neither app installed,
/// or running on a non-Windows dev host), not a failure.
#[tauri::command]
#[specta::specta]
pub fn detect_capcut_installations() -> Vec<DetectedCapCutInstallation> {
    detect::detect_windows_installations()
}

/// Best-effort, additive uninstall-registry hints (see
/// `crate::capcut::detect::scan_uninstall_registry` doc comment for
/// confidence caveats). Always succeeds with an empty `Vec` on failure or
/// on a non-Windows host — never blocks or replaces
/// [`detect_capcut_installations`].
#[tauri::command]
#[specta::specta]
pub fn detect_capcut_registry_hints() -> Vec<CapCutRegistryHint> {
    detect::scan_uninstall_registry()
}

/// "Open CapCut" (`STUDIO_PLAN.md` Phase S1, `promt.md` §10): launches the
/// real installed executable for `product`, resolved fresh from `user_profile`
/// (a `DetectedCapCutInstallation`'s own field — the caller passes back
/// exactly what `detect_capcut_installations` already returned, never a
/// user-typed path) via `capcut::detect::executable_path`. Opens CapCut to
/// its own home screen; jumping straight into a *specific* draft was tried
/// for real during this feature's own development (a plain draft-folder
/// path as a launch argument) and confirmed not to work — see
/// `STUDIO_PLAN.md`'s Phase S1 writeup — so this command does not attempt
/// to accept or use a draft path at all, rather than silently ignore one.
#[tauri::command]
#[specta::specta]
pub fn open_capcut(product: CapCutProduct, user_profile: String) -> Result<(), AppErrorPayload> {
    let exe = detect::executable_path(product, Path::new(&user_profile))
        .ok_or(CapCutError::ExecutableNotFound)
        .map_err(|e| AppErrorPayload::from(&e))?;
    std::process::Command::new(&exe).spawn().map_err(|e| {
        AppErrorPayload::from(&CapCutError::LaunchFailed {
            details: e.to_string(),
        })
    })?;
    Ok(())
}

/// "Validate Draft" (`STUDIO_PLAN.md` Phase S1, `promt.md` §10): a real
/// integrity check against an existing on-disk draft folder — see
/// `capcut::validate` module doc comment for exactly what's checked. Always
/// returns a real, fully-populated report, never an error — an unhealthy
/// draft is a normal, expected result this command reports honestly, not a
/// failure of the check itself.
#[tauri::command]
#[specta::specta]
pub fn validate_capcut_draft(draft_dir: String) -> DraftValidationReport {
    validate::validate_draft(Path::new(&draft_dir))
}

/// Reveals `draft_dir` in Windows Explorer with it pre-selected — a real,
/// honest substitute for "open this specific draft in CapCut" (which, per
/// [`open_capcut`]'s own doc comment, does not actually work): this at
/// least gets the user one click away from the real folder, rather than
/// shipping a button labeled "Open Current Project" that silently does
/// nothing more than [`open_capcut`] already does.
#[tauri::command]
#[specta::specta]
pub fn reveal_capcut_draft_in_explorer(draft_dir: String) -> Result<(), AppErrorPayload> {
    let path = Path::new(&draft_dir);
    if !path.exists() {
        return Err(AppErrorPayload::from(&CapCutError::LaunchFailed {
            details: format!("path does not exist: {draft_dir}"),
        }));
    }
    #[cfg(target_os = "windows")]
    {
        // `explorer.exe`'s own exit code is unreliable even on success (a
        // long-documented Windows quirk) — spawn and never treat its exit
        // status as this command's own failure.
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|e| {
                AppErrorPayload::from(&CapCutError::LaunchFailed {
                    details: e.to_string(),
                })
            })?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(AppErrorPayload::from(&CapCutError::LaunchFailed {
            details: "revealing a folder in Explorer is only supported on Windows".to_string(),
        }))
    }
}
