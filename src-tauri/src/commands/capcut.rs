//! CapCut/Jianying installation-detection Tauri command surface. Thin per
//! master prompt §66 — all real logic lives in `crate::capcut::detect`.

use crate::capcut::detect::{self, CapCutRegistryHint, DetectedCapCutInstallation};

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
