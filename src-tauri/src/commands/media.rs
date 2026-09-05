//! Media engine Tauri command surface: probing, import (files/folder),
//! thumbnails (generated as a side effect of import), proxy generation
//! (with real progress events), waveform computation, and media-library
//! search/listing. Thin per master prompt §66 — all real logic lives in
//! `crate::media`/`crate::audio`/`crate::db`/`crate::ffmpeg`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::{self, MediaLibrary, MediaLibraryEntry};
use crate::error::AppErrorPayload;
use crate::ffmpeg::binaries;
use crate::media::error::MediaError;
use crate::media::proxy::ProxyMode;
use crate::media::{import, probe, proxy, thumbnail};
use crate::project::{MediaItem, MediaKind, Rational};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn app_local_data_dir(app: &AppHandle) -> Result<PathBuf, MediaError> {
    app.path()
        .app_local_data_dir()
        .map_err(|e| MediaError::DatabaseError {
            details: format!("resolving app local data dir: {e}"),
        })
}

/// Where generated thumbnails/proxies live: `{app_local_data}/media_cache/`,
/// deliberately outside `$APPLOCALDATA/projects/**` (that path is
/// project-file storage) and covered by its own `tauri.conf.json`
/// `assetProtocol.scope` entry so thumbnails render without a per-file
/// runtime scope grant.
/// `pub(crate)`, not private: `commands::diagnostics::get_system_information`
/// (Phase 12, master prompt §78) reports this same cache-directory path/disk
/// usage in the System Information panel, rather than duplicating the
/// `"media_cache"` join.
pub(crate) fn media_cache_dir(app: &AppHandle) -> Result<PathBuf, MediaError> {
    Ok(app_local_data_dir(app)?.join("media_cache"))
}

fn media_db_path(app: &AppHandle) -> Result<PathBuf, MediaError> {
    Ok(app_local_data_dir(app)?.join("media_library.sqlite3"))
}

/// Called once from `lib.rs`'s `run()` setup hook to open/attach the shared
/// media library database as managed Tauri state.
pub fn init_media_library(app: &AppHandle) -> Result<(), MediaError> {
    let db_path = media_db_path(app)?;
    let conn = db::open(&db_path)?;
    app.manage(MediaLibrary(std::sync::Mutex::new(conn)));
    Ok(())
}

/// `pub(crate)`, not private: `commands::vad` reuses this exact resolution
/// logic to extract PCM for VAD scoring, rather than duplicating it.
pub(crate) fn resolve_ffmpeg(app: &AppHandle) -> Result<PathBuf, MediaError> {
    let resource_dir = app.path().resource_dir().ok();
    binaries::ffmpeg_path(resource_dir.as_deref()).map_err(|e| MediaError::BinaryNotFound {
        tool: "ffmpeg".into(),
        details: e.to_string(),
    })
}

/// `pub(crate)`, not private: `commands::diagnostics::get_system_information`
/// reuses this exact resolution logic rather than duplicating it (the way
/// `commands::shorts` already deliberately duplicates it — see that module's
/// comment — this one instead widens visibility since diagnostics has no
/// other reason to diverge).
pub(crate) fn resolve_ffprobe(app: &AppHandle) -> Result<PathBuf, MediaError> {
    let resource_dir = app.path().resource_dir().ok();
    binaries::ffprobe_path(resource_dir.as_deref()).map_err(|e| MediaError::BinaryNotFound {
        tool: "ffprobe".into(),
        details: e.to_string(),
    })
}

/// Extend the `asset:` protocol scope so the frontend can load this exact
/// file via `convertFileSrc`, one file at a time, instead of broadening
/// `tauri.conf.json`'s static scope to an unscoped `["**"]` the way
/// autocut's config did (flagged in `docs/architecture-audit.md` §2 as a
/// pattern not to inherit). Imported source media can live anywhere on the
/// user's disk — that's inherent to a non-destructive editor that references
/// files in place (master prompt §68) — so the scope has to be grown
/// per-file at the moment a file is actually chosen by the user (import) or
/// re-surfaced from the library (search/list), rather than pre-authorized
/// for an entire drive. This is a security/usability tradeoff made
/// deliberately here, not an oversight: see IMPLEMENTATION_PLAN.md Phase 3.
fn allow_asset_path(app: &AppHandle, path: &Path) {
    let _ = app.asset_protocol_scope().allow_file(path);
}

fn now_rfc3339() -> String {
    crate::project::now_rfc3339()
}

fn default_probe() -> probe::ProbedMedia {
    probe::ProbedMedia {
        duration_us: 0,
        width: 0,
        height: 0,
        fps: Rational::new(30, 1),
        codec: "unknown".to_string(),
        bitrate: 0,
        audio_channels: 0,
        sample_rate: 0,
        rotation_deg: 0,
        created_at: None,
        has_video: false,
        has_audio: false,
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FfmpegDiagnostics {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    /// Honest provenance note (master prompt §59/§78) — see
    /// `crate::ffmpeg::binaries` module doc comment for the full story.
    pub source_note: String,
}

#[tauri::command]
#[specta::specta]
pub fn ffmpeg_diagnostics(app: AppHandle) -> Result<FfmpegDiagnostics, AppErrorPayload> {
    (|| -> Result<FfmpegDiagnostics, MediaError> {
        let ffmpeg = resolve_ffmpeg(&app)?;
        let ffprobe = resolve_ffprobe(&app)?;
        let ffmpeg_version = binaries::version_string(&ffmpeg).unwrap_or_else(|e| format!("unknown ({e})"));
        let ffprobe_version = binaries::version_string(&ffprobe).unwrap_or_else(|e| format!("unknown ({e})"));
        Ok(FfmpegDiagnostics {
            ffmpeg_path: ffmpeg.display().to_string(),
            ffprobe_path: ffprobe.display().to_string(),
            ffmpeg_version,
            ffprobe_version,
            source_note: "No Windows ffmpeg/ffprobe binaries are bundled yet — resolved via the dev/test PATH fallback \
                (see crate::ffmpeg::binaries). The shipped-binary source/version/checksum decision is deferred to Phase 12."
                .to_string(),
        })
    })()
    .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Probe (read-only, does not touch the media library db)
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn probe_media_file(app: AppHandle, path: String) -> Result<MediaItem, AppErrorPayload> {
    build_media_item(&app, Path::new(&path), None)
        .map(|(item, _)| item)
        .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct ImportResult {
    pub source_path: String,
    pub media: Option<MediaItem>,
    pub error: Option<AppErrorPayload>,
}

fn build_media_item(
    app: &AppHandle,
    path: &Path,
    cache_dir: Option<&Path>,
) -> Result<(MediaItem, probe::ProbedMedia), MediaError> {
    if !path.exists() {
        return Err(MediaError::PathNotFound {
            path: path.display().to_string(),
        });
    }
    let kind = import::classify_extension(path).ok_or_else(|| MediaError::UnsupportedFormat {
        path: path.display().to_string(),
        extension: path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default(),
    })?;

    let ffprobe = resolve_ffprobe(app)?;
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let probed = match probe::probe(&ffprobe, &absolute) {
        Ok(p) => p,
        // Some still-image formats (notably certain WEBP variants) aren't
        // reliably probed by every ffmpeg build; a missing width/height
        // shouldn't block importing a picture, so fall back to zeros rather
        // than failing the whole import. Video/audio probe failures are not
        // swallowed — that's a real, actionable error (corrupt file,
        // unsupported codec).
        Err(e) if kind == MediaKind::Image => {
            eprintln!(
                "non-fatal: probing image {} failed, continuing with defaults: {e}",
                path.display()
            );
            default_probe()
        }
        Err(e) => return Err(e),
    };

    let id = uuid::Uuid::new_v4().to_string();
    let thumbnail_path = cache_dir.and_then(|cache_dir| {
        let out_dir = cache_dir.join(&id);
        let out_path = out_dir.join("thumb.jpg");
        let ffmpeg = resolve_ffmpeg(app).ok()?;
        let result = match kind {
            MediaKind::Video => {
                let seek_us = thumbnail::pick_thumbnail_timestamp_us(probed.duration_us);
                thumbnail::generate_video_thumbnail(&ffmpeg, &absolute, &out_path, seek_us)
            }
            MediaKind::Image => thumbnail::generate_image_thumbnail(&ffmpeg, &absolute, &out_path),
            MediaKind::Audio => return None,
        };
        match result {
            Ok(()) => {
                allow_asset_path(app, &out_path);
                Some(out_path.to_string_lossy().to_string())
            }
            Err(e) => {
                eprintln!(
                    "non-fatal: thumbnail generation failed for {}: {e}",
                    path.display()
                );
                None
            }
        }
    });

    allow_asset_path(app, &absolute);

    let item = MediaItem {
        id,
        kind,
        source_path: absolute.to_string_lossy().to_string(),
        duration_us: probed.duration_us,
        width: probed.width,
        height: probed.height,
        fps: probed.fps,
        codec: probed.codec.clone(),
        bitrate: probed.bitrate,
        audio_channels: probed.audio_channels,
        sample_rate: probed.sample_rate,
        rotation_deg: probed.rotation_deg,
        created_at: probed.created_at.clone(),
        proxy_path: None,
        thumbnail_path,
    };
    Ok((item, probed))
}

fn persist_and_kick_off_proxy(
    app: &AppHandle,
    library: &MediaLibrary,
    item: &MediaItem,
    proxy_mode: ProxyMode,
) -> Result<(), MediaError> {
    let filename = Path::new(&item.source_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| item.source_path.clone());

    let entry = MediaLibraryEntry {
        id: item.id.clone(),
        filename,
        path: item.source_path.clone(),
        kind: item.kind,
        duration_us: item.duration_us,
        width: item.width,
        height: item.height,
        tags: Vec::new(),
        created_at: item.created_at.clone(),
        imported_at: now_rfc3339(),
        thumbnail_path: item.thumbnail_path.clone(),
        proxy_path: None,
    };
    {
        let conn = library.0.lock().expect("media library mutex poisoned");
        db::upsert_media(&conn, &entry)?;
    }

    if item.kind == MediaKind::Video && proxy::should_generate_proxy(proxy_mode, item.height) {
        spawn_proxy_job(
            app.clone(),
            item.id.clone(),
            item.source_path.clone(),
            item.duration_us,
        );
    }
    Ok(())
}

fn import_one(
    app: &AppHandle,
    library: &MediaLibrary,
    cache_dir: &Path,
    path: &Path,
    proxy_mode: ProxyMode,
) -> ImportResult {
    let source_path = path.display().to_string();
    match build_media_item(app, path, Some(cache_dir)) {
        Ok((item, _)) => match persist_and_kick_off_proxy(app, library, &item, proxy_mode) {
            Ok(()) => ImportResult {
                source_path,
                media: Some(item),
                error: None,
            },
            Err(e) => ImportResult {
                source_path,
                media: Some(item),
                error: Some(AppErrorPayload::from(&e)),
            },
        },
        Err(e) => ImportResult {
            source_path,
            media: None,
            error: Some(AppErrorPayload::from(&e)),
        },
    }
}

/// Import an explicit list of files (drag & drop, multi-select file picker).
/// Never aborts on the first bad file — each path gets its own
/// success/error result so a multi-file drop of 50 files with one corrupt
/// clip still imports the other 49 (master prompt §7 "multi-file import").
#[tauri::command]
#[specta::specta]
pub fn import_media_paths(
    app: AppHandle,
    library: State<'_, MediaLibrary>,
    paths: Vec<String>,
    proxy_mode: ProxyMode,
) -> Result<Vec<ImportResult>, AppErrorPayload> {
    let cache_dir = media_cache_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    Ok(paths
        .into_iter()
        .map(|p| import_one(&app, &library, &cache_dir, Path::new(&p), proxy_mode))
        .collect())
}

/// Recursively import every supported file under `folder` (master prompt §7
/// "folder import").
#[tauri::command]
#[specta::specta]
pub fn import_media_folder(
    app: AppHandle,
    library: State<'_, MediaLibrary>,
    folder: String,
    proxy_mode: ProxyMode,
) -> Result<Vec<ImportResult>, AppErrorPayload> {
    let folder_path = Path::new(&folder);
    if !folder_path.is_dir() {
        return Err(AppErrorPayload::from(&MediaError::PathNotFound {
            path: folder,
        }));
    }
    let cache_dir = media_cache_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let files = import::scan_folder(folder_path);
    Ok(files
        .into_iter()
        .map(|p| import_one(&app, &library, &cache_dir, &p, proxy_mode))
        .collect())
}

// ---------------------------------------------------------------------------
// Proxy generation + progress events
// ---------------------------------------------------------------------------

/// Payload for the `media:proxy-progress` Tauri event. Emitted at least once
/// with `done: true` for every proxy job, success or failure — the frontend
/// doesn't need to poll a job list (no general Job Manager exists yet, that
/// is Phase 43/§43 scope) to know when a proxy finished.
#[derive(Debug, Clone, Serialize, Type)]
pub struct ProxyProgressEvent {
    pub media_id: String,
    pub fraction: Option<f64>,
    pub done: bool,
    pub proxy_path: Option<String>,
    pub error: Option<String>,
}

const PROXY_PROGRESS_EVENT: &str = "media:proxy-progress";

fn spawn_proxy_job(app: AppHandle, media_id: String, source_path: String, duration_us: i64) {
    tauri::async_runtime::spawn_blocking(move || {
        let outcome = (|| -> Result<PathBuf, MediaError> {
            let ffmpeg = resolve_ffmpeg(&app)?;
            let cache_dir = media_cache_dir(&app)?;
            let out_path = cache_dir.join(&media_id).join("proxy.mp4");
            let media_id_for_progress = media_id.clone();
            let app_for_progress = app.clone();
            proxy::generate_proxy(
                &ffmpeg,
                Path::new(&source_path),
                &out_path,
                duration_us,
                None,
                move |p| {
                    let _ = app_for_progress.emit(
                        PROXY_PROGRESS_EVENT,
                        ProxyProgressEvent {
                            media_id: media_id_for_progress.clone(),
                            fraction: p.fraction,
                            done: false,
                            proxy_path: None,
                            error: None,
                        },
                    );
                },
            )?;
            Ok(out_path)
        })();

        match outcome {
            Ok(out_path) => {
                allow_asset_path(&app, &out_path);
                if let Some(library) = app.try_state::<MediaLibrary>() {
                    if let Ok(conn) = library.0.lock() {
                        let _ =
                            db::set_proxy_path(&conn, &media_id, Some(&out_path.to_string_lossy()));
                    }
                }
                let _ = app.emit(
                    PROXY_PROGRESS_EVENT,
                    ProxyProgressEvent {
                        media_id,
                        fraction: Some(1.0),
                        done: true,
                        proxy_path: Some(out_path.to_string_lossy().to_string()),
                        error: None,
                    },
                );
            }
            Err(e) => {
                let _ = app.emit(
                    PROXY_PROGRESS_EVENT,
                    ProxyProgressEvent {
                        media_id,
                        fraction: None,
                        done: true,
                        proxy_path: None,
                        error: Some(e.to_string()),
                    },
                );
            }
        }
    });
}

/// On-demand proxy (re)generation, e.g. a user flipping Proxy mode from Off
/// to Always after already importing. Runs synchronously from the caller's
/// point of view (returns the final path or error) while still emitting the
/// same `media:proxy-progress` events a background import-triggered job
/// would, so one UI progress bar handles both cases.
#[tauri::command]
#[specta::specta]
pub async fn generate_media_proxy(
    app: AppHandle,
    media_id: String,
    source_path: String,
    mode: ProxyMode,
) -> Result<Option<String>, AppErrorPayload> {
    let source = PathBuf::from(&source_path);
    if !source.exists() {
        return Err(AppErrorPayload::from(&MediaError::PathNotFound {
            path: source_path,
        }));
    }
    let ffprobe = resolve_ffprobe(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let probed = probe::probe(&ffprobe, &source).map_err(|e| AppErrorPayload::from(&e))?;
    if !proxy::should_generate_proxy(mode, probed.height) {
        return Ok(None);
    }

    let app_for_task = app.clone();
    let media_id_for_task = media_id.clone();
    let duration_us = probed.duration_us;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg = resolve_ffmpeg(&app_for_task)?;
        let cache_dir = media_cache_dir(&app_for_task)?;
        let out_path = cache_dir.join(&media_id_for_task).join("proxy.mp4");
        let app_for_progress = app_for_task.clone();
        let media_id_for_progress = media_id_for_task.clone();
        proxy::generate_proxy(&ffmpeg, &source, &out_path, duration_us, None, move |p| {
            let _ = app_for_progress.emit(
                PROXY_PROGRESS_EVENT,
                ProxyProgressEvent {
                    media_id: media_id_for_progress.clone(),
                    fraction: p.fraction,
                    done: false,
                    proxy_path: None,
                    error: None,
                },
            );
        })?;
        Ok::<PathBuf, MediaError>(out_path)
    })
    .await
    .map_err(|e| {
        AppErrorPayload::from(&MediaError::ProxyFailed {
            path: source_path.clone(),
            details: format!("proxy task panicked: {e}"),
        })
    })?;

    match result {
        Ok(out_path) => {
            allow_asset_path(&app, &out_path);
            if let Some(library) = app.try_state::<MediaLibrary>() {
                if let Ok(conn) = library.0.lock() {
                    let _ = db::set_proxy_path(&conn, &media_id, Some(&out_path.to_string_lossy()));
                }
            }
            let path_str = out_path.to_string_lossy().to_string();
            let _ = app.emit(
                PROXY_PROGRESS_EVENT,
                ProxyProgressEvent {
                    media_id,
                    fraction: Some(1.0),
                    done: true,
                    proxy_path: Some(path_str.clone()),
                    error: None,
                },
            );
            Ok(Some(path_str))
        }
        Err(e) => {
            let _ = app.emit(
                PROXY_PROGRESS_EVENT,
                ProxyProgressEvent {
                    media_id,
                    fraction: None,
                    done: true,
                    proxy_path: None,
                    error: Some(e.to_string()),
                },
            );
            Err(AppErrorPayload::from(&e))
        }
    }
}

// ---------------------------------------------------------------------------
// Waveform
// ---------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn compute_media_waveform(
    app: AppHandle,
    path: String,
    bins: u32,
) -> Result<crate::audio::waveform::WaveformResult, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg = resolve_ffmpeg(&app)?;
        let source = PathBuf::from(&path);
        if !source.exists() {
            return Err(MediaError::PathNotFound { path });
        }
        let samples = crate::audio::pcm::extract_pcm(&ffmpeg, &source)?;
        crate::audio::waveform::waveform_from_samples(
            &samples,
            bins as usize,
            crate::audio::pcm::PCM_SAMPLE_RATE,
            None,
        )
    })
    .await
    .map_err(|e| {
        AppErrorPayload::from(&MediaError::WaveformFailed {
            path: String::new(),
            details: format!("waveform task panicked: {e}"),
        })
    })?
    .map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// Thumbnail strip (timeline clip filmstrip)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
pub struct ThumbnailStripFrame {
    pub timestamp_us: i64,
    pub path: String,
}

/// Generates `count` evenly-spaced frame thumbnails across `[0, duration_us)`
/// for the Phase 4 timeline's clip filmstrip display (master prompt §10
/// "thumbnail strip"). Thin IPC wiring only, per this task's narrow-command
/// exception: reuses the exact same `media::thumbnail::generate_video_thumbnail`
/// single-frame extractor Phase 3 already implemented and tested for the
/// media-library card thumbnail — this command just calls it `count` times
/// at different timestamps. Results are cached under this media id's cache
/// directory (`{media_cache}/{media_id}/strip/{index}.jpg`) so a repeat call
/// for the same media/count (e.g. re-requesting the strip after a zoom
/// change) skips regenerating frames that already exist on disk.
#[tauri::command]
#[specta::specta]
pub async fn generate_thumbnail_strip(
    app: AppHandle,
    media_id: String,
    source_path: String,
    duration_us: i64,
    count: u32,
) -> Result<Vec<ThumbnailStripFrame>, AppErrorPayload> {
    let count = i64::from(count.clamp(1, 64));
    let source = PathBuf::from(&source_path);
    if !source.exists() {
        return Err(AppErrorPayload::from(&MediaError::PathNotFound {
            path: source_path,
        }));
    }
    let cache_dir = media_cache_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let strip_dir = cache_dir.join(&media_id).join("strip");

    let app_for_task = app.clone();
    let source_path_for_task = source_path.clone();
    let frames = tauri::async_runtime::spawn_blocking(
        move || -> Result<Vec<ThumbnailStripFrame>, MediaError> {
            let ffmpeg = resolve_ffmpeg(&app_for_task)?;
            std::fs::create_dir_all(&strip_dir).map_err(|e| MediaError::ThumbnailFailed {
                path: source_path_for_task.clone(),
                details: format!("creating thumbnail strip dir {}: {e}", strip_dir.display()),
            })?;

            let span = duration_us.max(1);
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count {
                // Evenly spaced sample points, offset half a slot in so the
                // first/last frame isn't exactly the clip's first/last
                // instant (same "don't sample frame zero" reasoning as
                // `thumbnail::pick_thumbnail_timestamp_us`).
                let timestamp_us = ((i * 2 + 1) * span) / (count * 2);
                let out_path = strip_dir.join(format!("{i}.jpg"));
                if !out_path.exists() {
                    thumbnail::generate_video_thumbnail(&ffmpeg, &source, &out_path, timestamp_us)?;
                }
                out.push((timestamp_us, out_path));
            }
            Ok(out
                .into_iter()
                .map(|(timestamp_us, path)| ThumbnailStripFrame {
                    timestamp_us,
                    path: path.to_string_lossy().to_string(),
                })
                .collect())
        },
    )
    .await
    .map_err(|e| {
        AppErrorPayload::from(&MediaError::ThumbnailFailed {
            path: source_path.clone(),
            details: format!("thumbnail strip task panicked: {e}"),
        })
    })?
    .map_err(|e| AppErrorPayload::from(&e))?;

    for frame in &frames {
        allow_asset_path(&app, Path::new(&frame.path));
    }
    Ok(frames)
}

// ---------------------------------------------------------------------------
// Library search / listing / removal
// ---------------------------------------------------------------------------

fn extend_scope_for_entries(app: &AppHandle, entries: &[MediaLibraryEntry]) {
    for entry in entries {
        allow_asset_path(app, Path::new(&entry.path));
        if let Some(thumb) = &entry.thumbnail_path {
            allow_asset_path(app, Path::new(thumb));
        }
        if let Some(proxy) = &entry.proxy_path {
            allow_asset_path(app, Path::new(proxy));
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn search_media_library(
    app: AppHandle,
    library: State<'_, MediaLibrary>,
    query: Option<String>,
    kind: Option<MediaKind>,
    limit: u32,
) -> Result<Vec<MediaLibraryEntry>, AppErrorPayload> {
    let conn = library.0.lock().expect("media library mutex poisoned");
    let entries = db::search_media(&conn, query.as_deref(), kind, limit)
        .map_err(|e| AppErrorPayload::from(&e))?;
    // The static `assetProtocol.scope` in tauri.conf.json only covers this
    // app's own managed directories; original source files (which can live
    // anywhere) need their runtime-only grant re-issued every process start
    // — see `allow_asset_path`'s doc comment.
    extend_scope_for_entries(&app, &entries);
    Ok(entries)
}

#[tauri::command]
#[specta::specta]
pub fn list_media_library(
    app: AppHandle,
    library: State<'_, MediaLibrary>,
    limit: u32,
) -> Result<Vec<MediaLibraryEntry>, AppErrorPayload> {
    search_media_library(app, library, None, None, limit)
}

#[tauri::command]
#[specta::specta]
pub fn remove_media_from_library(
    library: State<'_, MediaLibrary>,
    id: String,
) -> Result<(), AppErrorPayload> {
    let conn = library.0.lock().expect("media library mutex poisoned");
    db::remove_media(&conn, &id).map_err(|e| AppErrorPayload::from(&e))
}

// ---------------------------------------------------------------------------
// AI-generated media tags (master prompt §35's "Optional AI-generated tags"
// enhancement, Part B of this pass) — thin wiring over `crate::ai::media_tags`
// (prompt/validation) and `crate::db` (the actual read/write). Two commands,
// mirroring every other AI-derived feature's "AI proposes, user approves"
// split (`ai::edit_plan`/`ai::smart_edit`/`crate::highlights`/`crate::broll`):
// `suggest_media_tags` only ever reads the library and calls the configured
// `AIProvider` — it never writes; `merge_media_tags` is the separate,
// explicit write step, only ever called on a caller's explicit acceptance.
// ---------------------------------------------------------------------------

/// **Suggest** (never writes): looks up `entry_id` in the local library,
/// builds a tag-suggestion prompt from its filename/metadata
/// (`ai::media_tags::build_media_tag_request`), calls the configured
/// provider, and validates the response into a strict `Vec<String>` — or a
/// clear error, never a partially populated result. Returned tags are a
/// *proposal* for a caller (frontend, later pass) to review; nothing here
/// mutates `entry_id`'s stored tags — see [`merge_media_tags`] for the
/// separate write step.
#[tauri::command]
#[specta::specta]
pub fn suggest_media_tags(
    library: State<'_, MediaLibrary>,
    settings: crate::commands::ai::AiProviderSettings,
    entry_id: String,
) -> Result<Vec<String>, AppErrorPayload> {
    let entry = {
        let conn = library.0.lock().expect("media library mutex poisoned");
        db::get_media_by_id(&conn, &entry_id).map_err(|e| AppErrorPayload::from(&e))?
    }
    .ok_or_else(|| {
        AppErrorPayload::from(&MediaError::PathNotFound {
            path: format!("media library id {entry_id}"),
        })
    })?;

    let api_key =
        crate::commands::ai::resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider = crate::commands::ai::build_provider(&settings, api_key)
        .map_err(|e| AppErrorPayload::from(&e))?;

    let request = crate::ai::media_tags::build_media_tag_request(
        &entry,
        settings.temperature,
        settings.timeout_ms,
    );
    let response = provider
        .complete(&request)
        .map_err(|e| AppErrorPayload::from(&e))?;
    crate::ai::media_tags::parse_and_validate(&response.text).map_err(|e| AppErrorPayload::from(&e))
}

/// **Merge** (the only actual write path): merges `tags` into `entry_id`'s
/// stored tags (`db::merge_media_tags` — existing tags are kept, an
/// already-present tag, any casing, is not duplicated) and returns the
/// updated entry. Only ever called on a caller's explicit acceptance of a
/// [`suggest_media_tags`] proposal (or any other caller-supplied tag list) —
/// never invoked automatically by suggestion itself.
#[tauri::command]
#[specta::specta]
pub fn merge_media_tags(
    library: State<'_, MediaLibrary>,
    entry_id: String,
    tags: Vec<String>,
) -> Result<MediaLibraryEntry, AppErrorPayload> {
    let conn = library.0.lock().expect("media library mutex poisoned");
    db::merge_media_tags(&conn, &entry_id, &tags).map_err(|e| AppErrorPayload::from(&e))
}
