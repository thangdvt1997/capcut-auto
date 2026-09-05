use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use sysinfo::{Disks, System};
use tauri::{AppHandle, Manager};

use crate::capcut::detect::{self, DetectedCapCutInstallation};
use crate::error::AppErrorPayload;
use crate::ffmpeg::binaries;
use crate::render::hwaccel::{self, DetectedEncoder};
use crate::transcription::{self, InstalledModel};

/// Real logs directory: Tauri's own `app_log_dir()` resolver, i.e.
/// `%LOCALAPPDATA%\<identifier>\logs\` on Windows (`dirs::data_local_dir()`
/// joined with the app identifier and `logs`, per that resolver's own
/// source — see `crate::logging` module doc comment). This deliberately
/// stays consistent with every *other* app-data directory in this codebase
/// (`models_dir`/`templates_dir`/`media_cache_dir` all use the same
/// identifier-scoped convention via `app_local_data_dir`) rather than
/// hardcoding the master prompt §54's literal example path
/// (`%LOCALAPPDATA%\AI Video Editor\logs`) — a deliberate, documented, minor
/// deviation for consistency, not an oversight.
pub(crate) fn logs_dir(app: &AppHandle) -> Result<PathBuf, AppErrorPayload> {
    app.path().app_log_dir().map_err(|e| {
        AppErrorPayload::new("PathResolutionFailed", format!("resolving logs dir: {e}"))
    })
}

/// Static, build-time facts about this build. No telemetry, no network
/// calls, nothing environment-dependent beyond `std::env::consts`. Exists in
/// Phase 2 to exercise the Rust -> specta -> TypeScript pipeline end-to-end
/// with a genuinely working command; the full Settings > About / "Copy
/// System Information" panel (master prompt §78) is [`get_system_information`]
/// below, added in Phase 12.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ShellInfo {
    pub app_version: String,
    pub tauri_version: String,
    pub os: String,
    pub arch: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_shell_info() -> ShellInfo {
    ShellInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

// ---------------------------------------------------------------------------
// System Information panel (master prompt §78)
// ---------------------------------------------------------------------------

/// Free/total space at one directory this app actually cares about. `path`
/// may not exist yet (e.g. a models dir before any model was ever
/// downloaded) — [`disk_space_for`] still reports real space for whichever
/// ancestor directory *does* exist, since the disk backing a not-yet-created
/// subdirectory is exactly the disk a "will there be room for this" check
/// needs to know about.
#[derive(Debug, Clone, Serialize, Type)]
pub struct DiskSpaceInfo {
    pub path: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// One real system diagnostics snapshot (master prompt §78), gathered from
/// this crate's own already-real detectors — nothing here is
/// reimplemented: FFmpeg/FFprobe resolution+version reuse
/// `commands::media::resolve_ffmpeg`/`resolve_ffprobe` +
/// `ffmpeg::binaries::version_string` (Phase 3), hardware-encoder detection
/// reuses `render::hwaccel::detect_encoders` (Phase 6), CapCut/Jianying
/// detection reuses `capcut::detect::detect_windows_installations` (Phase
/// 9), installed transcription models reuse
/// `transcription::list_installed` (Phase 7). Only CPU/RAM/OS-version/disk
/// space have no existing source elsewhere in this crate — those come from
/// the `sysinfo` crate, added for exactly this command.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SystemInformation {
    pub app_version: String,
    pub tauri_version: String,
    pub os: String,
    /// `sysinfo::System::long_os_version()` — on Windows this includes the
    /// build number (e.g. "Windows 11 Pro 23H2"), which is what master
    /// prompt §78's "Windows version" line actually wants; `None` if
    /// `sysinfo` couldn't determine it (never observed on this crate's own
    /// Linux dev host either — it reports a real Linux description there).
    pub os_version: Option<String>,
    pub arch: String,
    pub cpu_brand: Option<String>,
    pub cpu_core_count: usize,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    /// See `ffmpeg::binaries` module doc comment / `commands::media::FfmpegDiagnostics`
    /// — the same honest provenance note, not duplicated logic.
    pub ffmpeg_source_note: String,
    /// GPU-adjacent signal this crate actually has: which hardware encoder
    /// backends (NVENC/Quick Sync/AMF) are both registered in this FFmpeg
    /// build *and* pass a real smoke-test encode on this machine. There is
    /// no separate "GPU model name" detector anywhere in this codebase —
    /// reusing `render::hwaccel` rather than adding one gives the same real
    /// signal the render pipeline itself already uses to choose an encoder.
    pub hardware_encoders: Vec<DetectedEncoder>,
    pub active_encoder_label: String,
    /// Every confirmed CapCut/Jianying draft-root installation found on this
    /// machine (empty `Vec` is a legitimate "not installed" answer, not an
    /// error — see `capcut::detect` module doc comment). No "CapCut
    /// version" field exists here: this crate's detector has never had one
    /// (filesystem-marker-based detection, not a version read from
    /// CapCut/Jianying's own installed files) — honestly omitted rather
    /// than fabricated.
    pub capcut_installations: Vec<DetectedCapCutInstallation>,
    pub transcription_backend: String,
    pub installed_transcription_models: Vec<InstalledModel>,
    pub models_dir: String,
    pub templates_dir: String,
    pub media_cache_dir: String,
    /// Always `None` — there is no canonical default project-save directory
    /// anywhere in this codebase yet. `project::io::ProjectV1::save_atomic`/
    /// `load` take an arbitrary caller-chosen path with no default location
    /// convention (no Project Manager / recent-projects / "default project
    /// folder" setting has been built as of Phase 11 — see `HANDOFF.md`).
    /// Honestly omitted rather than inventing a location nothing in this
    /// app actually uses.
    pub project_directory: Option<String>,
    pub logs_dir: String,
    /// Disk space at every directory above that currently resolves to a
    /// real path (`models_dir`/`templates_dir`/`media_cache_dir`/
    /// `logs_dir` — never `project_directory`, which is always `None`).
    pub disk_space: Vec<DiskSpaceInfo>,
}

/// Finds the mounted disk backing `path` (the entry whose `mount_point` is
/// the longest matching prefix of `path`'s nearest existing ancestor) and
/// reports its total/available space. Returns `None` only if `sysinfo`
/// couldn't enumerate any disks at all (never observed in practice, but a
/// real possibility on an unusual/sandboxed host) — callers skip that entry
/// rather than fabricating zeros.
fn disk_space_for(disks: &Disks, path: &Path) -> Option<DiskSpaceInfo> {
    // `path` itself may not exist yet; walk up to the nearest existing
    // ancestor so a not-yet-created models/templates dir still reports the
    // real disk it *would* land on.
    let mut probe = path;
    loop {
        if probe.exists() {
            break;
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            None => break,
        }
    }

    disks
        .list()
        .iter()
        .filter(|d| probe.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| DiskSpaceInfo {
            path: path.display().to_string(),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
        })
}

#[tauri::command]
#[specta::specta]
pub fn get_system_information(app: AppHandle) -> Result<SystemInformation, AppErrorPayload> {
    let sys = System::new_all();
    let cpu_brand = sys.cpus().first().map(|c| c.brand().to_string());

    let resource_dir = app.path().resource_dir().ok();
    let ffmpeg_path = binaries::ffmpeg_path(resource_dir.as_deref());
    let ffprobe_path = binaries::ffprobe_path(resource_dir.as_deref());
    let ffmpeg_version = ffmpeg_path
        .as_ref()
        .ok()
        .and_then(|p| binaries::version_string(p).ok())
        .unwrap_or_else(|| "not found".to_string());
    let ffprobe_version = ffprobe_path
        .as_ref()
        .ok()
        .and_then(|p| binaries::version_string(p).ok())
        .unwrap_or_else(|| "not found".to_string());

    let hardware_encoders = ffmpeg_path
        .as_ref()
        .ok()
        .and_then(|p| hwaccel::detect_encoders(p).ok())
        .unwrap_or_default();
    let active_encoder_label = if hardware_encoders.is_empty() {
        "unknown (ffmpeg not resolvable)".to_string()
    } else {
        hwaccel::active_encoder_label(&hardware_encoders, crate::render::presets::VideoCodec::H264)
    };

    let models_dir =
        crate::commands::transcription::models_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let templates_dir =
        crate::commands::templates::templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let media_cache_dir =
        crate::commands::media::media_cache_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let log_dir = logs_dir(&app)?;

    let disks = Disks::new_with_refreshed_list();
    let disk_space = [&models_dir, &templates_dir, &media_cache_dir, &log_dir]
        .into_iter()
        .filter_map(|p| disk_space_for(&disks, p))
        .collect();

    Ok(SystemInformation {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        os_version: System::long_os_version(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_brand,
        cpu_core_count: sys.cpus().len(),
        total_memory_bytes: sys.total_memory(),
        used_memory_bytes: sys.used_memory(),
        ffmpeg_path: ffmpeg_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("not found ({e})")),
        ffprobe_path: ffprobe_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| format!("not found ({e})")),
        ffmpeg_version,
        ffprobe_version,
        ffmpeg_source_note: "No Windows ffmpeg/ffprobe binaries are bundled yet unless \
            scripts/fetch-ffmpeg.ps1 has been run (see THIRD_PARTY_NOTICES.md) — resolved here via \
            whatever this machine's binaries.rs resolution finds (bundled sidecar, or the dev/test \
            PATH fallback)."
            .to_string(),
        hardware_encoders,
        active_encoder_label,
        capcut_installations: detect::detect_windows_installations(),
        transcription_backend:
            "whisper.cpp (via the whisper-rs crate), CPU-only unless built with \
            the `cuda` feature — see transcription::whisper module doc comment"
                .to_string(),
        installed_transcription_models: transcription::list_installed(&models_dir),
        models_dir: models_dir.display().to_string(),
        templates_dir: templates_dir.display().to_string(),
        media_cache_dir: media_cache_dir.display().to_string(),
        project_directory: None,
        logs_dir: log_dir.display().to_string(),
        disk_space,
    })
}

// ---------------------------------------------------------------------------
// Crash handling / logs folder (master prompt §54/§86)
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn get_logs_folder_path(app: AppHandle) -> Result<String, AppErrorPayload> {
    let dir = logs_dir(&app)?;
    Ok(dir.display().to_string())
}

/// Opens the real logs folder in the OS file explorer (`explorer.exe` on
/// Windows — see `crate::logging::open_folder`). Creates the directory
/// first if it somehow doesn't exist yet (e.g. logging failed to
/// initialize at startup — see `lib.rs`), so the button never opens a
/// "path not found" dialog.
#[tauri::command]
#[specta::specta]
pub fn open_logs_folder(app: AppHandle) -> Result<(), AppErrorPayload> {
    let dir = logs_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppErrorPayload::new(
            "LogsFolderUnavailable",
            format!("creating {}: {e}", dir.display()),
        )
    })?;
    crate::logging::open_folder(&dir).map_err(|e| {
        AppErrorPayload::new(
            "LogsFolderUnavailable",
            format!("opening {}: {e}", dir.display()),
        )
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_last_session_status(
    state: tauri::State<'_, crate::logging::SessionStatus>,
) -> crate::logging::SessionStatus {
    state.inner().clone()
}
