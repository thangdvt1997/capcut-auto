//! Resolve `ffmpeg`/`ffprobe` sidecar paths at runtime.
//!
//! Adapted from `vendor/autocut/src-tauri/src/binaries.rs` (reuse permitted,
//! `docs/upstream.md`). The layered search order and `#[cfg]` target-triple
//! matrix are carried over largely as-is; the one behavioral addition is an
//! explicit **system-`PATH` fallback**, gated to debug/test builds only. That
//! fallback is what lets this module — and everything built on it — be
//! exercised on this project's headless Linux build server (which has no
//! bundled Windows sidecars, only an apt-installed `ffmpeg`/`ffprobe`) without
//! pretending the fallback is a real shipping strategy.
//!
//! **Honest status of binary provenance** (`docs/architecture-audit.md` §6
//! risk #7, master prompt §59): no Windows ffmpeg/ffprobe binaries are
//! bundled with this project yet, and no checksum/source/license decision has
//! been made for the eventual shipped binaries. That decision is explicitly
//! deferred to Phase 12 (Windows packaging) per the Phase 3 task brief — this
//! module only implements the *resolution logic* (where a sidecar would live
//! if it existed: next to the running exe, then the Tauri resource dir, both
//! with and without the target-triple suffix Tauri strips at bundle time),
//! plus the dev/test-only PATH fallback described above. Do not read the
//! presence of this fallback as "the ffmpeg sourcing decision is done" — it
//! isn't.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Ffmpeg,
    Ffprobe,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
        }
    }
}

// Same target matrix as autocut's binaries.rs: Windows x64 MSVC + GNU ABI,
// plus Linux/macOS so this crate itself builds and tests on the non-Windows
// hosts actually used for development (this project has no Windows machine
// with the toolchain installed — see IMPLEMENTATION_PLAN.md Phase 2 notes).
// No Windows ARM64 — matches autocut's own gap, not revisited here.
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
compile_error!("ffmpeg/ffprobe sidecar resolution is not configured for this target");

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const TARGET_TRIPLE: &str = "aarch64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const TARGET_TRIPLE: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const TARGET_TRIPLE: &str = "aarch64-unknown-linux-gnu";
#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
const TARGET_TRIPLE: &str = "x86_64-pc-windows-msvc";
#[cfg(all(
    target_os = "windows",
    target_arch = "x86_64",
    not(target_env = "msvc")
))]
const TARGET_TRIPLE: &str = "x86_64-pc-windows-gnu";

fn ext() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// Where a bundled sidecar would live, tried in priority order:
/// 1. next to the running executable (production layout after Tauri's
///    bundler strips the target-triple suffix), with and without the suffix
///    (dev-built binaries next to `cargo run`'s output keep the suffix);
/// 2. the Tauri resource dir, same two suffix variants;
/// 3. the build-time `binaries/ffmpeg-<triple>` dev path under
///    `CARGO_MANIFEST_DIR` (lets `cargo run`/`cargo test` find a
///    manually-placed dev binary without a full bundle step).
fn bundled_candidates(tool: Tool, resource_dir: Option<&Path>) -> Vec<PathBuf> {
    let leaf = tool.name();
    let suffix = ext();
    let triple = TARGET_TRIPLE;
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join(format!("{leaf}{suffix}")));
            candidates.push(parent.join(format!("{leaf}-{triple}{suffix}")));
        }
    }
    if let Some(rd) = resource_dir {
        candidates.push(rd.join(format!("{leaf}{suffix}")));
        candidates.push(rd.join(format!("{leaf}-{triple}{suffix}")));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("{leaf}-{triple}{suffix}")),
    );
    candidates
}

/// Dev/test-only fallback: ask the shell's `PATH` for the tool, the way a
/// developer's `apt install ffmpeg` or `choco install ffmpeg` would provide
/// it. Never consulted in a release build — a release binary with no bundled
/// sidecar found should fail loudly (packaging bug), not silently pick up
/// whatever the end user happens to have on `PATH` (which may be a
/// mismatched or absent version, and master prompt §59 wants one
/// deliberately-chosen, checksum-verified binary in the shipped product).
#[cfg(any(debug_assertions, test))]
fn path_fallback(tool: Tool) -> Option<PathBuf> {
    let name = tool.name();
    let probe = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let out = Command::new(probe).arg(name).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let first_line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    if first_line.is_empty() {
        None
    } else {
        Some(PathBuf::from(first_line))
    }
}

#[cfg(not(any(debug_assertions, test)))]
fn path_fallback(_tool: Tool) -> Option<PathBuf> {
    None
}

fn binary_path(tool: Tool, resource_dir: Option<&Path>) -> Result<PathBuf> {
    let candidates = bundled_candidates(tool, resource_dir);
    for c in &candidates {
        if c.exists() {
            return Ok(c.clone());
        }
    }

    if let Some(p) = path_fallback(tool) {
        return Ok(p);
    }

    let attempted = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(anyhow!(
        "could not locate {} binary. tried (bundled): {attempted}{}",
        tool.name(),
        if cfg!(any(debug_assertions, test)) {
            format!(
                "; also tried system PATH via `which {}`/`where {}`",
                tool.name(),
                tool.name()
            )
        } else {
            String::new()
        }
    ))
}

static FFMPEG_PATH: OnceLock<PathBuf> = OnceLock::new();
static FFPROBE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn cached_binary_path(
    cache: &OnceLock<PathBuf>,
    tool: Tool,
    resource_dir: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = cache.get() {
        return Ok(path.clone());
    }
    let path = binary_path(tool, resource_dir)?;
    let _ = cache.set(path.clone());
    Ok(path)
}

pub fn ffmpeg_path(resource_dir: Option<&Path>) -> Result<PathBuf> {
    cached_binary_path(&FFMPEG_PATH, Tool::Ffmpeg, resource_dir).context("resolving ffmpeg")
}

pub fn ffprobe_path(resource_dir: Option<&Path>) -> Result<PathBuf> {
    cached_binary_path(&FFPROBE_PATH, Tool::Ffprobe, resource_dir).context("resolving ffprobe")
}

/// `ffmpeg -version`/`ffprobe -version`'s first line, for the diagnostics
/// panel (master prompt §78) and for confirming which binary is actually in
/// use during dev/test (bundled vs. PATH fallback).
pub fn version_string(binary: &Path) -> Result<String> {
    let out = Command::new(binary)
        .arg("-version")
        .output()
        .with_context(|| format!("running {} -version", binary.display()))?;
    let first_line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    Ok(first_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_candidates_include_both_suffixed_and_bare_names_next_to_the_exe() {
        let candidates = bundled_candidates(Tool::Ffmpeg, None);
        assert!(candidates
            .iter()
            .any(|c| c.file_name().unwrap() == format!("ffmpeg{}", ext()).as_str()));
        assert!(candidates
            .iter()
            .any(|c| c.to_string_lossy().contains(TARGET_TRIPLE)));
    }

    #[test]
    fn bundled_candidates_include_the_resource_dir_when_given() {
        let rd = Path::new("/opt/app/resources");
        let candidates = bundled_candidates(Tool::Ffprobe, Some(rd));
        assert!(candidates.iter().any(|c| c.starts_with(rd)));
    }

    #[test]
    fn dev_build_falls_back_to_a_nonexistent_bundled_path_plus_path_lookup() {
        // On this dev/test machine there is no bundled sidecar at all, so the
        // only way `binary_path` can succeed is the PATH fallback — this
        // exercises the exact code path Phase 3's remote-server testing
        // relies on for every ffprobe/ffmpeg-touching test in this crate.
        let resolved = binary_path(Tool::Ffmpeg, None);
        assert!(
            resolved.is_ok(),
            "expected the dev/test PATH fallback to find an apt-installed ffmpeg: {resolved:?}"
        );
    }

    #[test]
    fn version_string_reports_a_non_empty_first_line() {
        let ffmpeg = ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let version = version_string(&ffmpeg).expect("ffmpeg -version runs");
        assert!(version.to_lowercase().contains("ffmpeg"), "{version}");
    }

    /// Phase 12 (Windows packaging) fixture test for the resolution logic
    /// `scripts/fetch-ffmpeg.ps1`'s output and `tauri.windows.conf.json`'s
    /// `bundle.externalBin` both depend on. Two of `bundled_candidates`'
    /// three lookup locations are *not* safely test-isolatable here:
    /// candidate #1 (next to `std::env::current_exe()`) and candidate #3
    /// (`CARGO_MANIFEST_DIR/binaries/...`, the fetch script's actual real
    /// output path) are both fixed, global, non-parameterized paths that
    /// every other ffmpeg/ffprobe-touching test in this crate *also*
    /// resolves against (many via the cached `ffmpeg_path`/`ffprobe_path`
    /// wrappers) — `cargo test`'s default multi-threaded parallelism means
    /// writing a fixture file to either would risk another, unrelated test
    /// racily resolving to (and caching!) this test's fixture instead of a
    /// real binary. Candidate #2 (the `resource_dir` parameter) is the one
    /// caller-supplied, per-call location `bundled_candidates` offers —
    /// exercising it here, with a unique temp directory no other test
    /// shares, validates the exact same filename/suffix-matching logic
    /// (bare name vs. `-<target-triple>` suffixed name, platform `.exe`
    /// extension handling) with zero cross-test interference, and is
    /// exactly the code path a bundled app resolves through in production
    /// (`commands::media::resolve_ffmpeg` passes the real Tauri
    /// `resource_dir()` here the same way).
    #[test]
    fn a_fixture_binary_placed_at_the_documented_resource_dir_naming_convention_is_found() {
        let resource_dir = std::env::temp_dir().join(format!(
            "ave-ffmpeg-binaries-fixture-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&resource_dir).expect("create fixture resource dir");
        let fixture_path = resource_dir.join(format!("ffprobe-{TARGET_TRIPLE}{}", ext()));
        std::fs::write(
            &fixture_path,
            b"not a real binary, just a resolution-logic fixture",
        )
        .expect("write fixture file");

        let resolved =
            binary_path(Tool::Ffprobe, Some(&resource_dir)).expect("fixture binary should resolve");
        assert_eq!(
            resolved, fixture_path,
            "expected binary_path to find the fixture placed at the documented \
             `<resource_dir>/<tool>-<target-triple><ext>` naming convention"
        );

        std::fs::remove_dir_all(&resource_dir).expect("clean up fixture dir");
    }
}
