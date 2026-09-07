//! CapCut/Jianying installation detection (Phase 9, `IMPLEMENTATION_PLAN.md`
//! "Implement CapCut/Jianying installation detection on Windows"). A direct
//! Rust port of `vendor/capcut-mate/desktop-client/nodeapi/draftPathDetect.js`'s
//! Windows heuristic, restructured per this crate's established
//! "OS-agnostic testable core + thin real-filesystem wrapper" pattern (see
//! `crate::ffmpeg::binaries` module doc comment for the model this follows).
//!
//! ## What the reference JS does
//!
//! `draftPathDetect.js`'s `detectWindowsDraftPath()`:
//! 1. builds `<os.homedir()>\AppData\Local\JianyingPro\User Data\Projects\com.lveditor.draft`
//!    as a fast-path candidate, checked first;
//! 2. lists every immediate subdirectory of `<SystemDrive>\Users` (falling
//!    back to `C:` if `SystemDrive` is unset) as a candidate user profile,
//!    building the same relative draft-root path under each;
//! 3. returns the *first* candidate confirmed by
//!    `isConfirmedJianyingDraftDir` — present, readable/writable, and
//!    containing either a `root_meta_info.json` **file** or a
//!    `.recycle_bin` **directory**.
//!
//! ## What this port changes, and why
//!
//! - **International CapCut folder name added.** The reference list only
//!   ever checks the `JianyingPro` AppData folder name (China-region
//!   Jianying Pro). International CapCut installs on Windows use the same
//!   `AppData\Local\<Product>\User Data\Projects\com.lveditor.draft`
//!   directory *shape*, just with the product folder named `CapCut` instead
//!   of `JianyingPro` — this mirrors the two names capcut-mate's own macOS
//!   path list (`detectMacDraftPath`, same file) already distinguishes
//!   (`~/Movies/CapCut/...` vs `~/Movies/JianyingPro/...`), so the Windows
//!   list carrying only one of the two names looks like an oversight in the
//!   original, not a deliberate choice — this is a moderate-confidence
//!   inference from that internal inconsistency plus the well-documented
//!   general convention that CapCut's international build otherwise mirrors
//!   Jianying Pro's directory layout 1:1 (same `com.lveditor.draft` leaf,
//!   same `root_meta_info.json`/`.recycle_bin` markers), not a directly
//!   cited external source — flagged honestly for real-CapCut validation
//!   (`IMPLEMENTATION_PLAN.md`'s separate "Validate draft compatibility
//!   against a real installed CapCut build" bullet already covers verifying
//!   this against an actual CapCut installation).
//! - **Returns every confirmed installation, not just the first.** The
//!   reference returns on the first hit (single global draft root, since
//!   node-capcut-mate assumes exactly one Jianying Pro install per machine).
//!   This app's CapCut settings UI wants to show *what's actually there*
//!   (a machine could plausibly have both Jianying Pro and international
//!   CapCut installed, or multiple Windows user profiles each with their
//!   own install) — so [`scan_users_root`] collects every confirmed
//!   candidate into a `Vec` instead of short-circuiting. Order is
//!   documented on [`scan_users_root`] below; callers that want
//!   "first/preferred" behavior can just take `.first()`.
//! - **`root_meta_info.json` is not parsed for version fields.** Grepping
//!   `vendor/capcut-mate/src/` for `root_meta_info` turns up only
//!   deletion/cleanup logic (`src/utils/jianying_export_cleanup.py`) — no
//!   code anywhere in the vendored tree reads *fields out of* the file, and
//!   no schema/example for it is checked in (the only meta-info schema
//!   present, `assets/draft_meta_info.json`, is a *different*, per-draft
//!   file, not the shared root-level one). Rather than guess a shape with
//!   no reference, [`DetectedCapCutInstallation`] reports presence/kind of
//!   the confirming marker only, honestly, and leaves version parsing
//!   unimplemented until a real sample file (or documentation) turns up.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

/// Which product's AppData folder name a candidate draft root was built
/// under. Both are checked for every candidate user profile — see this
/// module's doc comment for why the reference JS's Jianying-only list is
/// extended here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CapCutProduct {
    /// China-region 剪映专业版 (Jianying Pro) — `AppData\Local\JianyingPro`.
    Jianying,
    /// International CapCut — `AppData\Local\CapCut`.
    CapCut,
}

impl CapCutProduct {
    const ALL: [CapCutProduct; 2] = [CapCutProduct::Jianying, CapCutProduct::CapCut];

    /// The literal AppData product-folder name for each variant.
    fn appdata_folder_name(self) -> &'static str {
        match self {
            CapCutProduct::Jianying => "JianyingPro",
            CapCutProduct::CapCut => "CapCut",
        }
    }

    /// The path segments under a user's home directory, matching
    /// `draftPathDetect.js`'s `DRAFT_REL_WIN` (`path.join("AppData",
    /// "Local", "JianyingPro", "User Data", "Projects",
    /// "com.lveditor.draft")`), parameterized by product folder name.
    fn draft_root_rel_segments(self) -> [&'static str; 6] {
        [
            "AppData",
            "Local",
            self.appdata_folder_name(),
            "User Data",
            "Projects",
            "com.lveditor.draft",
        ]
    }

    fn draft_root_under(self, home: &Path) -> PathBuf {
        let mut p = home.to_path_buf();
        for seg in self.draft_root_rel_segments() {
            p.push(seg);
        }
        p
    }
}

/// Real, launch-ready executable path for a confirmed installation's own
/// `product`, given its `user_profile` home directory. Real observed
/// convention (verified against an actual installed international CapCut
/// Pro, v9.3.0.3970 — `STUDIO_PLAN.md` Phase S1): a stable launcher shim at
/// `%LOCALAPPDATA%\<Product>\Apps\<Product>.exe`, distinct from the
/// versioned subdirectory (e.g. `Apps\9.3.0.3970\CapCut.exe`) that changes
/// on every update — the unversioned launcher is used deliberately so this
/// keeps working across CapCut's own updates without this app needing to
/// track version numbers. Returns `None` if that exact file doesn't exist —
/// never guesses at a versioned path instead. Jianying Pro's own exe naming
/// is assumed to mirror this pattern (same reasoning as
/// `draft_root_rel_segments`'s own folder-name convention) but has **not**
/// been verified against a real Jianying installation (this project's real
/// hands-on access has only ever been to a real international CapCut).
pub fn executable_path(product: CapCutProduct, user_profile: &Path) -> Option<PathBuf> {
    let exe = user_profile
        .join("AppData")
        .join("Local")
        .join(product.appdata_folder_name())
        .join("Apps")
        .join(format!("{}.exe", product.appdata_folder_name()));
    exe.is_file().then_some(exe)
}

/// One confirmed CapCut/Jianying draft-root installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DetectedCapCutInstallation {
    pub product: CapCutProduct,
    /// The user-profile home directory this installation was found under
    /// (e.g. `C:\Users\Alice`), as a display string — see `crate::commands`
    /// convention of exposing filesystem paths as `String`, not `PathBuf`,
    /// over the Tauri/specta boundary.
    pub user_profile: String,
    /// The confirmed draft-root directory itself
    /// (`...\User Data\Projects\com.lveditor.draft`).
    pub draft_root: String,
    /// Whether `root_meta_info.json` (a file) was found directly inside
    /// `draft_root`.
    pub has_root_meta_info: bool,
    /// Whether a `.recycle_bin` directory was found directly inside
    /// `draft_root`. Either this or `has_root_meta_info` is always `true`
    /// for a value that made it into a result `Vec` — that's the
    /// confirmation condition (see [`confirm_draft_root`]).
    pub has_recycle_bin: bool,
}

/// Present in `draft_root` iff it's a real, confirmed Jianying/CapCut draft
/// root: contains `root_meta_info.json` (a file) or a `.recycle_bin`
/// (a directory) — same two markers `isConfirmedJianyingDraftDir` in the
/// reference JS checks. Unlike the reference, this does not separately probe
/// read/write access on the directory (`fs.access(dir, R_OK | W_OK)`) before
/// checking markers: `Path::is_file`/`Path::is_dir` below already return
/// `false` for a directory that doesn't exist or can't be stat'd, which
/// covers the "candidate isn't real" case the access check was guarding;
/// a directory that exists and stats fine but somehow isn't writable is a
/// real-world edge case better surfaced as an error at the moment the app
/// actually tries to write a draft into it, not silently disqualified here.
fn confirm_draft_root(
    draft_root: &Path,
    product: CapCutProduct,
    user_profile: &Path,
) -> Option<DetectedCapCutInstallation> {
    let has_root_meta_info = draft_root.join("root_meta_info.json").is_file();
    let has_recycle_bin = draft_root.join(".recycle_bin").is_dir();
    if !has_root_meta_info && !has_recycle_bin {
        return None;
    }
    Some(DetectedCapCutInstallation {
        product,
        user_profile: user_profile.display().to_string(),
        draft_root: draft_root.display().to_string(),
        has_root_meta_info,
        has_recycle_bin,
    })
}

/// OS-agnostic core: given a "users root" directory (in real Windows use,
/// `C:\Users`; in tests, a temp directory standing in for it) and an
/// optional fast-path home directory (mirrors the reference's
/// `os.homedir()` pre-check, tried before the full scan), enumerate every
/// immediate subdirectory of `users_root` as a candidate user profile, build
/// a candidate draft-root path under each candidate profile for *both*
/// [`CapCutProduct`] variants, and return every confirmed installation
/// found (see [`confirm_draft_root`]).
///
/// Order of the returned `Vec`: `fast_path_home` first (if given and
/// confirmed), Jianying before CapCut for a given profile (documented
/// preference for display purposes only — both are always included when
/// both are confirmed, never silently dropped, unlike the reference JS's
/// first-hit-wins short circuit), then `users_root`'s subdirectories in
/// whatever order the OS's directory listing returns them. Duplicate
/// `(product, draft_root)` pairs (e.g. `fast_path_home` also appearing as a
/// `users_root` subdirectory) are only reported once.
///
/// Non-directory entries under `users_root` (a stray file sitting directly
/// in `C:\Users`) are skipped, not treated as an error. A `users_root` that
/// doesn't exist or can't be read produces an empty scan (not a panic/error)
/// — `fast_path_home`, if given, is still checked.
pub fn scan_users_root(
    users_root: &Path,
    fast_path_home: Option<&Path>,
) -> Vec<DetectedCapCutInstallation> {
    let mut profile_dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = fast_path_home {
        profile_dirs.push(home.to_path_buf());
    }

    if let Ok(entries) = fs::read_dir(users_root) {
        for entry in entries.flatten() {
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            profile_dirs.push(entry.path());
        }
    }

    let mut seen: HashSet<(CapCutProduct, PathBuf)> = HashSet::new();
    let mut results = Vec::new();
    for profile_dir in &profile_dirs {
        for product in CapCutProduct::ALL {
            let draft_root = product.draft_root_under(profile_dir);
            if !seen.insert((product, draft_root.clone())) {
                continue;
            }
            if let Some(installation) = confirm_draft_root(&draft_root, product, profile_dir) {
                results.push(installation);
            }
        }
    }
    results
}

/// Real Windows entry point: scans `%SystemDrive%\Users` (falling back to
/// `C:\Users` if `SystemDrive` is unset, matching the reference's own
/// `process.env.SystemDrive || "C:"` fallback), with the current user's own
/// home directory (`%USERPROFILE%`, this crate's dependency-free equivalent
/// of Node's `os.homedir()` on Windows) checked first as a fast path —
/// preserving the same optimization `detectWindowsDraftPath` makes.
#[cfg(target_os = "windows")]
pub fn detect_windows_installations() -> Vec<DetectedCapCutInstallation> {
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let users_root = PathBuf::from(format!("{system_drive}\\Users"));
    let home = std::env::var("USERPROFILE").ok().map(PathBuf::from);
    scan_users_root(&users_root, home.as_deref())
}

/// Non-Windows stub. This app is Windows-only in scope (master prompt), but
/// this crate is built and unit-tested on Linux in this project's actual dev
/// environment (`HANDOFF.md` "Build/test environment") and the Tauri command
/// surface (`crate::commands`) must compile on every host the crate targets
/// — so a same-named function has to exist here too. It honestly reports "no
/// installations" rather than attempting any real detection, which is a
/// correct answer on a platform CapCut/Jianying were never installed on to
/// begin with.
#[cfg(not(target_os = "windows"))]
pub fn detect_windows_installations() -> Vec<DetectedCapCutInstallation> {
    Vec::new()
}

// ---------------------------------------------------------------------------
// Optional registry lookups (additive, best-effort — never gates filesystem
// detection above)
// ---------------------------------------------------------------------------

/// One uninstall-registry hit that looks like a CapCut/Jianying install,
/// found by [`registry::scan_uninstall_entries`]. Deliberately a separate
/// shape from [`DetectedCapCutInstallation`] rather than merged into it: a
/// registry entry can exist with no matching draft folder yet (installed but
/// never launched) and carries different fields (a version string, an
/// installer-reported location that may or may not be the actual draft
/// root) — conflating the two would mean either fabricating fields on one
/// side or silently dropping information on the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CapCutRegistryHint {
    pub product: CapCutProduct,
    pub display_name: String,
    pub display_version: Option<String>,
    pub install_location: Option<String>,
}

/// Best-effort Windows uninstall-registry scan for a CapCut/Jianying entry.
///
/// **Honesty note on confidence.** No specific uninstall registry *subkey
/// name* (the GUID/ProductCode or literal string Windows installers key
/// their `Uninstall\<name>` entry under) could be confirmed for either
/// product from any source available while writing this — general web
/// search turned up only generic "CapCut/Bytedance entries exist somewhere
/// under `HKCU\Software` / `HKLM\Software`" guidance, nothing citing an
/// exact key name, GUID, or confirming that either product's installer even
/// populates `InstallLocation` (many per-user installers don't). Rather than
/// fabricate a specific subkey path, this does the only thing that doesn't
/// require guessing one: **enumerate every subkey** under the standard
/// per-user uninstall root
/// (`HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall`,
/// the same root the Microsoft Q&A guidance found during research points
/// at) and match by `DisplayName` substring ("CapCut" / "剪映" / "Jianying"),
/// reading `DisplayVersion`/`InstallLocation` only from subkeys that match.
/// This is a legitimate, standard technique (the same one Windows' own "Apps
/// & Features" UI is built on) rather than a guessed fixed path — but it's
/// still unverified against a real Windows machine with CapCut/Jianying
/// actually installed, so treat its results as a hint to display, not a
/// certainty. Deliberately per-user (`HKEY_CURRENT_USER`) only, not
/// `HKEY_LOCAL_MACHINE`: both products are consumer apps overwhelmingly
/// installed per-user without elevation, and skipping `HKLM` avoids the
/// 32-bit/`WOW6432Node` view-redirection complexity for a lookup that's
/// explicitly optional/supplementary.
///
/// Always additive: any failure here (key doesn't exist, access denied,
/// anything) returns an empty `Vec` rather than an error, and this is never
/// called by [`scan_users_root`]/[`detect_windows_installations`] above —
/// filesystem-based detection works identically whether or not this finds
/// anything.
#[cfg(target_os = "windows")]
pub fn scan_uninstall_registry() -> Vec<CapCutRegistryHint> {
    registry::scan_uninstall_entries()
}

/// Non-Windows stub, mirroring [`detect_windows_installations`]'s twin above.
#[cfg(not(target_os = "windows"))]
pub fn scan_uninstall_registry() -> Vec<CapCutRegistryHint> {
    Vec::new()
}

#[cfg(target_os = "windows")]
mod registry {
    use super::{CapCutProduct, CapCutRegistryHint};

    const UNINSTALL_ROOT: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

    fn match_product(display_name: &str) -> Option<CapCutProduct> {
        let lower = display_name.to_lowercase();
        if lower.contains("capcut") {
            Some(CapCutProduct::CapCut)
        } else if lower.contains("jianying") || display_name.contains('剪') {
            // "剪" (from 剪映, Jianying's Chinese name) matched directly
            // rather than lowercased — CJK characters have no case, and
            // matching a single distinctive character from the two-character
            // product name avoids requiring the exact three-character
            // string ("剪映" vs "剪映专业版" vs other variants installers
            // might use) be guessed correctly.
            Some(CapCutProduct::Jianying)
        } else {
            None
        }
    }

    pub fn scan_uninstall_entries() -> Vec<CapCutRegistryHint> {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let mut hints = Vec::new();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(uninstall) = hkcu.open_subkey(UNINSTALL_ROOT) else {
            return hints;
        };
        for name in uninstall.enum_keys().flatten() {
            let Ok(app_key) = uninstall.open_subkey(&name) else {
                continue;
            };
            let display_name: String = match app_key.get_value("DisplayName") {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(product) = match_product(&display_name) else {
                continue;
            };
            let display_version: Option<String> = app_key.get_value("DisplayVersion").ok();
            let install_location: Option<String> = app_key.get_value("InstallLocation").ok();
            hints.push(CapCutRegistryHint {
                product,
                display_name,
                display_version,
                install_location,
            });
        }
        hints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self as stdfs, File};

    /// A self-cleaning temp directory, following this crate's established
    /// `std::env::temp_dir().join(format!("ave-..-test-{uuid}"))` pattern
    /// (see e.g. `crate::media::import`'s tests) rather than pulling in a
    /// `tempfile` dev-dependency this crate doesn't otherwise have.
    struct TempScratch {
        path: PathBuf,
    }

    impl TempScratch {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ave-capcut-detect-{label}-{}",
                uuid::Uuid::new_v4()
            ));
            stdfs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempScratch {
        fn drop(&mut self) {
            let _ = stdfs::remove_dir_all(&self.path);
        }
    }

    /// Builds `<root>/<profile>/AppData/Local/<ProductFolder>/User Data/
    /// Projects/com.lveditor.draft` and returns its path, creating all
    /// parent directories.
    fn make_draft_dir(root: &Path, profile: &str, product: CapCutProduct) -> PathBuf {
        let dir = product.draft_root_under(&root.join(profile));
        stdfs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn executable_path_finds_a_real_launcher_at_the_expected_location() {
        let tmp = TempScratch::new("exe-found");
        let profile = tmp.path.join("Alice");
        let apps_dir = profile
            .join("AppData")
            .join("Local")
            .join("CapCut")
            .join("Apps");
        stdfs::create_dir_all(&apps_dir).unwrap();
        File::create(apps_dir.join("CapCut.exe")).unwrap();

        let found = executable_path(CapCutProduct::CapCut, &profile);
        assert_eq!(found, Some(apps_dir.join("CapCut.exe")));
    }

    #[test]
    fn executable_path_returns_none_when_no_launcher_exists() {
        let tmp = TempScratch::new("exe-missing");
        let profile = tmp.path.join("Bob");
        stdfs::create_dir_all(&profile).unwrap();

        assert_eq!(executable_path(CapCutProduct::CapCut, &profile), None);
    }

    #[test]
    fn executable_path_does_not_fall_back_to_a_versioned_subdirectory() {
        let tmp = TempScratch::new("exe-versioned-only");
        let profile = tmp.path.join("Carol");
        let versioned_dir = profile
            .join("AppData")
            .join("Local")
            .join("CapCut")
            .join("Apps")
            .join("9.3.0.3970");
        stdfs::create_dir_all(&versioned_dir).unwrap();
        File::create(versioned_dir.join("CapCut.exe")).unwrap();

        // Only the unversioned launcher counts — a versioned-only install
        // (the launcher shim itself missing/not-yet-created) must not be
        // silently substituted.
        assert_eq!(executable_path(CapCutProduct::CapCut, &profile), None);
    }

    #[test]
    fn finds_a_jianying_profile_confirmed_by_root_meta_info_json() {
        let tmp = TempScratch::new("jianying-meta");
        let users_root = tmp.path.join("Users");
        let draft_dir = make_draft_dir(&users_root, "Alice", CapCutProduct::Jianying);
        File::create(draft_dir.join("root_meta_info.json")).unwrap();

        // A second profile with neither marker present must not match.
        make_draft_dir(&users_root, "Bob", CapCutProduct::Jianying);

        let results = scan_users_root(&users_root, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].product, CapCutProduct::Jianying);
        assert!(results[0].has_root_meta_info);
        assert!(!results[0].has_recycle_bin);
        assert!(results[0].user_profile.ends_with("Alice"));
    }

    #[test]
    fn finds_a_capcut_profile_confirmed_by_recycle_bin_directory() {
        let tmp = TempScratch::new("capcut-recycle");
        let users_root = tmp.path.join("Users");
        let draft_dir = make_draft_dir(&users_root, "Carol", CapCutProduct::CapCut);
        stdfs::create_dir_all(draft_dir.join(".recycle_bin")).unwrap();

        let results = scan_users_root(&users_root, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].product, CapCutProduct::CapCut);
        assert!(!results[0].has_root_meta_info);
        assert!(results[0].has_recycle_bin);
    }

    #[test]
    fn a_profile_with_neither_marker_produces_no_match() {
        let tmp = TempScratch::new("neither-marker");
        let users_root = tmp.path.join("Users");
        make_draft_dir(&users_root, "Dave", CapCutProduct::Jianying);
        make_draft_dir(&users_root, "Dave", CapCutProduct::CapCut);

        let results = scan_users_root(&users_root, None);
        assert!(results.is_empty());
    }

    #[test]
    fn finds_both_jianying_and_capcut_when_both_are_confirmed_on_the_same_machine() {
        let tmp = TempScratch::new("both-products");
        let users_root = tmp.path.join("Users");
        let jy_dir = make_draft_dir(&users_root, "Erin", CapCutProduct::Jianying);
        File::create(jy_dir.join("root_meta_info.json")).unwrap();
        let cc_dir = make_draft_dir(&users_root, "Erin", CapCutProduct::CapCut);
        stdfs::create_dir_all(cc_dir.join(".recycle_bin")).unwrap();

        let results = scan_users_root(&users_root, None);
        assert_eq!(results.len(), 2);
        let products: HashSet<CapCutProduct> = results.iter().map(|r| r.product).collect();
        assert!(products.contains(&CapCutProduct::Jianying));
        assert!(products.contains(&CapCutProduct::CapCut));
    }

    #[test]
    fn a_nonexistent_users_root_produces_no_matches_without_panicking() {
        let tmp = TempScratch::new("missing-users-root");
        let missing = tmp.path.join("does-not-exist");
        let results = scan_users_root(&missing, None);
        assert!(results.is_empty());
    }

    #[test]
    fn a_users_root_entry_that_is_a_file_not_a_directory_is_skipped_not_errored() {
        let tmp = TempScratch::new("file-not-dir");
        let users_root = tmp.path.join("Users");
        stdfs::create_dir_all(&users_root).unwrap();
        // A stray file directly under Users (e.g. desktop.ini-like litter).
        File::create(users_root.join("not-a-profile.txt")).unwrap();

        let results = scan_users_root(&users_root, None);
        assert!(results.is_empty());
    }

    #[test]
    fn an_empty_users_root_produces_no_matches() {
        let tmp = TempScratch::new("empty-users-root");
        let users_root = tmp.path.join("Users");
        stdfs::create_dir_all(&users_root).unwrap();
        let results = scan_users_root(&users_root, None);
        assert!(results.is_empty());
    }

    #[test]
    fn fast_path_home_is_checked_even_when_it_is_outside_users_root() {
        let tmp = TempScratch::new("fast-path-home");
        let users_root = tmp.path.join("Users"); // deliberately never created
        let home = tmp.path.join("SomeOtherHomeDir");
        let draft_dir = product_draft_dir_for_test(&home, CapCutProduct::CapCut);
        stdfs::create_dir_all(draft_dir.join(".recycle_bin")).unwrap();

        let results = scan_users_root(&users_root, Some(&home));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].product, CapCutProduct::CapCut);
    }

    fn product_draft_dir_for_test(home: &Path, product: CapCutProduct) -> PathBuf {
        let dir = product.draft_root_under(home);
        stdfs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn duplicate_fast_path_and_scanned_profile_is_reported_once() {
        let tmp = TempScratch::new("dedup");
        let users_root = tmp.path.join("Users");
        let draft_dir = make_draft_dir(&users_root, "Alice", CapCutProduct::Jianying);
        File::create(draft_dir.join("root_meta_info.json")).unwrap();

        let home = users_root.join("Alice");
        let results = scan_users_root(&users_root, Some(&home));
        assert_eq!(results.len(), 1);
    }

    /// Registry scanning is Windows-only and can't run for real on this
    /// dev/test Linux host — the non-Windows stub is what actually runs in
    /// `cargo test` here, and this asserts it stays a true no-op (never
    /// breaks/blocks filesystem detection, per this module's "additive,
    /// best-effort" requirement).
    #[test]
    fn registry_scan_stub_is_a_harmless_empty_result_on_this_platform() {
        let hints = scan_uninstall_registry();
        #[cfg(not(target_os = "windows"))]
        assert!(hints.is_empty());
        // On Windows this would be a real (possibly non-empty) scan; nothing
        // to assert generically about its contents here.
        let _ = hints;
    }
}
