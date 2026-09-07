//! The real per-file pipeline: Analyzing -> Transcribing (optional) ->
//! Editing (silence removal / captions / template settings) -> Rendering.
//! No Tauri dependency at all (`PipelineIo` is plain paths, `run_pipeline`
//! takes no `AppHandle`) — exactly the same "the real synchronous core has
//! no Tauri types in its signature" shape `render::job::run_render_job`
//! already established, which is what makes this module directly unit-
//! testable and directly reusable from `batch::manager`'s worker thread.
//!
//! ## Which existing synchronous cores this stage orchestration reuses
//!
//! - **Analyzing**: `media::probe::probe` (Phase 3), unchanged.
//! - **Transcribing**: `transcription::WhisperProvider::load`/
//!   `transcribe_with_progress` (Phase 7) — the exact same real, synchronous
//!   whisper.cpp entry point `commands::transcription::transcribe_media`'s
//!   own spawned thread calls; no extraction needed, it was already a plain
//!   function with no Tauri type in its signature.
//! - **Editing / silence removal**: `vad::SileroVadProvider::score_chunks` +
//!   `vad::segments_from_scores` + `vad::build_cuts_from_speech_segments` +
//!   `timeline::silence::apply_cuts_to_track` (Phase 5) — unchanged.
//! - **Editing / captions**: `captions::generate::generate_captions_from_transcript`
//!   (Phase 8), fed a transcript re-timed across the post-cut timeline by
//!   [`remap_transcript_across_fragments`] below, which itself reuses (not
//!   duplicates) `shorts::captions::slice_transcript_for_span`'s per-span
//!   clip-and-retime logic, called once per surviving clip fragment.
//! - **Editing / template settings**: `templates::all_templates`/
//!   `templates::io::list_custom_templates` (Phase 11) for template lookup;
//!   canvas/caption-style/silence-settings-default/export-preset-fallback
//!   application is new orchestration glue (there is no existing "apply a
//!   template to a project" function to reuse — `templates::save_as_template_from_project`
//!   goes the other direction, project -> template).
//! - **Rendering**: `render::build_render_graph`, `render::build_ffmpeg_plan`
//!   and `render::run_render_job` (Phase 6) — the exact real synchronous
//!   chain `commands::render::start_render_job`'s own spawned thread calls,
//!   reused unchanged, including its own existing cancellation and
//!   partial-output cleanup discipline.
//!
//! Nothing here was "buried too deep to reuse" — every stage's real logic
//! already lived in a plain function with no Tauri/async-job type in its
//! signature, so no extraction from an existing command wrapper was needed
//! (see `IMPLEMENTATION_PLAN.md` Phase 11's batch-processing bullet for the
//! full writeup of this decision).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::assets::{io as assets_io, Asset};
use crate::audio::pcm;
use crate::captions::generate as captions_generate;
use crate::media::import::classify_extension;
use crate::media::probe::{self, ProbedMedia};
use crate::project::{
    AudioClipSettings, AudioRole, CanvasV1, Clip, ClipSettings, MediaItem, MediaKind, ProjectV1,
    Track, TrackKind, TranscriptEntry,
};
use crate::render;
use crate::templates::{self, io as template_io, Template, WatermarkPosition};
use crate::timeline::silence as timeline_silence;
use crate::transcription;
use crate::vad::{self, VadError, VadParams, VadProvider};

use super::error::BatchError;
use super::types::{BatchJobStatus, BatchPipelineConfig};

/// Everything `run_pipeline` needs to locate real, already-resolved
/// filesystem resources — resolved once by `batch::manager` (which does
/// need an `AppHandle`, to ask Tauri for `app_local_data_dir`/`resource_dir`)
/// and handed down here as plain paths, so this module itself stays
/// Tauri-free.
pub struct PipelineIo<'a> {
    pub ffmpeg: &'a Path,
    pub ffprobe: &'a Path,
    pub models_dir: &'a Path,
    pub templates_dir: &'a Path,
    /// Real Asset Library directory (`commands::assets::assets_dir`) —
    /// STUDIO_PLAN.md Phase S2's own template-application step
    /// (`apply_intro_outro`/`apply_watermark`/`apply_background_music`)
    /// resolves a resolved template's `intro`/`outro`/`watermark`/
    /// `background_music` asset-id references against this exact directory.
    pub assets_dir: &'a Path,
}

/// `pub(crate)`, not private: `batch::dry_run` (upgrade-plan §18) reuses this
/// exact error-shaping helper rather than duplicating it — the same
/// "reuse the shared sub-piece, don't reimplement it" discipline that
/// module's own doc comment follows for `resolve_template`/
/// `default_output_path` below.
pub(crate) fn stage_failed(stage: &str, details: impl std::fmt::Display) -> BatchError {
    BatchError::StageFailed {
        stage: stage.to_string(),
        details: details.to_string(),
    }
}

/// Stage progress weights (Analyzing/Transcribing/Editing/Rendering), summing
/// to `1.0`. Transcribing carries zero weight (and is skipped entirely) when
/// no downstream stage needs a transcript — see `BatchPipelineConfig::captions`
/// doc comment. These are deliberately rough (this is a progress *estimate*
/// for a UI progress bar, not a scheduling guarantee) — Rendering gets the
/// largest share since a real encode is normally the slowest stage.
#[derive(Debug, Clone, Copy)]
struct StageWeights {
    analyzing: f32,
    transcribing: f32,
    editing: f32,
    rendering: f32,
}

impl StageWeights {
    fn new(needs_transcript: bool) -> Self {
        if needs_transcript {
            Self {
                analyzing: 0.05,
                transcribing: 0.30,
                editing: 0.15,
                rendering: 0.50,
            }
        } else {
            Self {
                analyzing: 0.05,
                transcribing: 0.0,
                editing: 0.15,
                rendering: 0.80,
            }
        }
    }

    fn offset(&self, stage: BatchJobStatus) -> f32 {
        match stage {
            BatchJobStatus::Analyzing => 0.0,
            BatchJobStatus::Transcribing => self.analyzing,
            BatchJobStatus::Editing => self.analyzing + self.transcribing,
            BatchJobStatus::Rendering => self.analyzing + self.transcribing + self.editing,
            _ => 1.0,
        }
    }

    fn weight(&self, stage: BatchJobStatus) -> f32 {
        match stage {
            BatchJobStatus::Analyzing => self.analyzing,
            BatchJobStatus::Transcribing => self.transcribing,
            BatchJobStatus::Editing => self.editing,
            BatchJobStatus::Rendering => self.rendering,
            _ => 0.0,
        }
    }

    /// Overall `[0.0, 1.0]` batch-job progress for being `fraction` of the
    /// way through `stage`.
    fn overall(&self, stage: BatchJobStatus, fraction: f32) -> f32 {
        (self.offset(stage) + self.weight(stage) * fraction.clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }
}

/// Checked before every stage boundary (master prompt §42's "pause takes
/// effect at the next stage boundary" — see `batch` module doc comment for
/// why this, not true mid-operation pause, is this pass's honest "resume
/// where technically possible" interpretation). Cancellation always wins
/// over a pause: a cancelled-while-paused job unblocks as `Cancelled`, never
/// stays parked forever. While actually parked, emits exactly one `Paused`
/// progress update (not one per poll tick) so the frontend sees the pause
/// without an event-spam loop.
fn checkpoint(
    cancel: &AtomicBool,
    pause: &AtomicBool,
    stage_label: &str,
    progress: f32,
    on_progress: &Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync>,
) -> Result<(), BatchError> {
    if cancel.load(Ordering::SeqCst) {
        return Err(BatchError::Cancelled);
    }
    if pause.load(Ordering::SeqCst) {
        on_progress(BatchJobStatus::Paused, stage_label.to_string(), progress);
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if cancel.load(Ordering::SeqCst) {
                return Err(BatchError::Cancelled);
            }
            if !pause.load(Ordering::SeqCst) {
                break;
            }
        }
    }
    Ok(())
}

/// Looks up `template_id` first against the built-in catalog, then against
/// `templates_dir`'s custom templates — the exact same two-tier lookup
/// `commands::templates::export_template` already does.
///
/// `pub(crate)`, not private: `batch::dry_run` (upgrade-plan §18) resolves
/// the real template a dry run would apply through this exact function —
/// never a second, parallel lookup.
pub(crate) fn resolve_template(
    templates_dir: &Path,
    template_id: &str,
) -> Result<Template, BatchError> {
    if let Some(t) = templates::all_templates()
        .into_iter()
        .find(|t| t.id == template_id)
    {
        return Ok(t);
    }
    let custom = template_io::list_custom_templates(templates_dir)
        .map_err(|e| stage_failed("Analyzing", e))?;
    custom
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| BatchError::UnknownTemplate {
            template_id: template_id.to_string(),
        })
}

/// Narrow, `pub(crate)` sliver of [`resolve_template`] for
/// `batch::manager::start_multi_template_batch` (upgrade-plan §11): resolves
/// just the real, human-readable `name` a template id maps to (built-in or
/// custom, same two-tier lookup), reusing this module's one real lookup
/// rather than a second one living in `manager`. Used both to label each
/// fanned-out job (`"video01.mp4 -> TikTok"`) and, via
/// [`slugify_template_name`], to derive its output filename suffix.
pub(crate) fn resolve_template_name(
    templates_dir: &Path,
    template_id: &str,
) -> Result<String, BatchError> {
    resolve_template(templates_dir, template_id).map(|t| t.name)
}

/// Sibling of [`resolve_template_name`] for `history::HistoryEntry::template_version`
/// (upgrade-plan §21): resolves just a template id's current real `version`
/// (built-in-then-custom two-tier lookup, same one real lookup reused, not a
/// second one). `batch::manager` calls this once a job reaches a terminal
/// state, to record which version of its template was current at that
/// moment — see `HistoryEntry::template_version`'s own doc comment for the
/// narrow, honestly-documented race this implies (resolved at
/// history-write time, not re-threaded out of this same `resolve_template`
/// call already made once, earlier, inside `run_pipeline` itself).
pub(crate) fn resolve_template_version(
    templates_dir: &Path,
    template_id: &str,
) -> Result<u32, BatchError> {
    resolve_template(templates_dir, template_id).map(|t| t.version)
}

/// Turns a template's real display `name` (e.g. `"YouTube Shorts"`, or a
/// user-authored custom template's name — arbitrary text, spaces/punctuation
/// included) into the exact filesystem-safe slug §11's own worked example
/// uses for output naming (`video01_tiktok.mp4`): lowercased, every non-
/// ASCII-alphanumeric character collapsed to a single `_` (consecutive
/// separators never produce a run of underscores), with leading/trailing
/// underscores trimmed. Falls back to `"template"` for the degenerate case
/// of a name with no alphanumeric characters at all, so a job's output path
/// is never left with an empty suffix (`<stem>_.<ext>`).
pub(crate) fn slugify_template_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_sep = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep && !slug.is_empty() {
            slug.push('_');
            last_was_sep = true;
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    if slug.is_empty() {
        "template".to_string()
    } else {
        slug
    }
}

/// A real, single-clip-per-media-kind `ProjectV1` spanning the whole source
/// file — the batch equivalent of `shorts::build::build_short_project`'s "one
/// real project per candidate" convention, generalized to the whole media
/// (no span selection) and to a separate audio track/clip (needed so a
/// render actually carries sound — `render::graph::build_render_graph`'s own
/// module doc comment/tests: video and audio content are resolved from
/// separate track kinds, never implicitly muxed from one clip).
#[derive(Debug)]
struct BuiltProject {
    project: ProjectV1,
    media_id: String,
    video_track_id: Option<String>,
    audio_track_id: Option<String>,
    caption_track_id: String,
}

fn build_whole_media_project(
    media_path: &Path,
    probed: &ProbedMedia,
    canvas_override: Option<&CanvasV1>,
) -> Result<BuiltProject, BatchError> {
    if !probed.has_video && !probed.has_audio {
        return Err(BatchError::UnsupportedMedia {
            path: media_path.display().to_string(),
        });
    }

    let name = media_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("batch item");
    let mut project = ProjectV1::new(format!("Batch - {name}"));
    if let Some(canvas) = canvas_override {
        project.canvas = canvas.clone();
    }

    let media_id = Uuid::new_v4().to_string();
    project.media.push(MediaItem {
        id: media_id.clone(),
        kind: if probed.has_video {
            MediaKind::Video
        } else {
            MediaKind::Audio
        },
        source_path: media_path.to_string_lossy().to_string(),
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
        thumbnail_path: None,
    });

    let mut tracks = Vec::new();
    let mut clips = Vec::new();

    let video_track_id = if probed.has_video {
        let track_id = Uuid::new_v4().to_string();
        let clip_id = Uuid::new_v4().to_string();
        tracks.push(Track {
            id: track_id.clone(),
            kind: TrackKind::Video,
            name: "Video".to_string(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec![clip_id.clone()],
        });
        clips.push(Clip {
            id: clip_id,
            track_id: track_id.clone(),
            media_id: Some(media_id.clone()),
            source_in_us: 0,
            source_out_us: probed.duration_us,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
        Some(track_id)
    } else {
        None
    };

    let audio_track_id = if probed.has_audio {
        let track_id = Uuid::new_v4().to_string();
        let clip_id = Uuid::new_v4().to_string();
        tracks.push(Track {
            id: track_id.clone(),
            kind: TrackKind::Audio,
            name: "Audio".to_string(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec![clip_id.clone()],
        });
        clips.push(Clip {
            id: clip_id,
            track_id: track_id.clone(),
            media_id: Some(media_id.clone()),
            source_in_us: 0,
            source_out_us: probed.duration_us,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
        Some(track_id)
    } else {
        None
    };

    let caption_track_id = Uuid::new_v4().to_string();
    tracks.push(Track {
        id: caption_track_id.clone(),
        kind: TrackKind::Caption,
        name: "Captions".to_string(),
        render_index: 1,
        locked: false,
        hidden: false,
        muted: false,
        solo: false,
        clip_ids: Vec::new(),
    });

    project.tracks = tracks;
    project.clips = clips;

    Ok(BuiltProject {
        project,
        media_id,
        video_track_id,
        audio_track_id,
        caption_track_id,
    })
}

// ---------------------------------------------------------------------------
// STUDIO_PLAN.md Phase S2: applying a resolved template's asset references
// (`intro`/`outro`/`watermark`/`background_music`) to a `BuiltProject` —
// closes the "validates but does nothing" gap `templates::mod`'s own module
// doc comment documented for these four fields. Each function below is a
// no-op (`Ok(())`) when its own template field is `None`, so calling all
// three unconditionally on every resolved template is always safe.
// ---------------------------------------------------------------------------

/// Resolves one of a template's asset-by-id references against the real
/// Asset Library (`assets::io::load_asset`) — the same on-disk catalog
/// `templates::validate_asset_references` already checked the id exists
/// against at template-save time; this is the later, real resolution to an
/// actual `Asset` (and, from there, a real file on disk) a batch job
/// building its project from that template needs.
fn resolve_asset(assets_dir: &Path, asset_id: &str) -> Result<Asset, BatchError> {
    assets_io::load_asset(assets_dir, asset_id).map_err(|e| stage_failed("Editing", e))
}

/// The real, current end position (max across every clip on `built`'s own
/// main video/audio tracks) — "how long is the built project's main content
/// right now". Recomputed fresh every call (never cached) since silence
/// removal and intro/outro splicing both change it; every clip in this
/// pipeline always has `speed == 1.0`, so `source_out_us - source_in_us` is
/// exactly that clip's on-timeline duration (no `render::graph`-style
/// speed-adjustment needed here).
fn built_content_duration_us(built: &BuiltProject) -> i64 {
    built
        .project
        .clips
        .iter()
        .filter(|c| {
            Some(&c.track_id) == built.video_track_id.as_ref()
                || Some(&c.track_id) == built.audio_track_id.as_ref()
        })
        .map(|c| c.position_us + (c.source_out_us - c.source_in_us).max(0))
        .max()
        .unwrap_or(0)
}

/// Shifts every clip on `built`'s own main video/audio tracks, and every
/// caption (+ its word timings), later by `delta_us` — the real timeline
/// math a real intro insertion needs so nothing already placed silently
/// overlaps the newly-spliced-in intro.
fn shift_existing_content(built: &mut BuiltProject, delta_us: i64) {
    if delta_us == 0 {
        return;
    }
    for clip in built.project.clips.iter_mut() {
        if Some(&clip.track_id) == built.video_track_id.as_ref()
            || Some(&clip.track_id) == built.audio_track_id.as_ref()
        {
            clip.position_us += delta_us;
        }
    }
    for caption in built.project.captions.iter_mut() {
        caption.start_us += delta_us;
        caption.end_us += delta_us;
        for word in caption.words.iter_mut() {
            word.start_us += delta_us;
            word.end_us += delta_us;
        }
    }
}

/// Adds one real clip for `asset` at `position_us`, onto whichever of
/// `built`'s own video/audio tracks actually exist AND the asset itself
/// really has that kind of content (`probed.has_video`/`has_audio`, real
/// `media::probe::probe` output — never assumed) — e.g. an audio-only intro
/// asset spliced into a video+audio project only gets an audio clip, never a
/// silent black video clip. A single new `MediaItem` is registered once and
/// referenced by both clips when both apply (the same "one `MediaItem`, N
/// clips" convention `ProjectV1` already uses everywhere else). Onto the
/// SAME tracks `build_whole_media_project` already created, not a new
/// track — an intro/outro is ordinary main content, just spliced before/
/// after the rest, exactly as a human editor would drag a clip onto the
/// existing timeline (module doc comment for `apply_intro_outro` below).
fn splice_clip(built: &mut BuiltProject, asset: &Asset, probed: &ProbedMedia, position_us: i64) {
    if (!probed.has_video || built.video_track_id.is_none())
        && (!probed.has_audio || built.audio_track_id.is_none())
    {
        return; // nothing this asset can contribute to either existing track
    }
    let media_id = Uuid::new_v4().to_string();
    built.project.media.push(MediaItem {
        id: media_id.clone(),
        kind: if probed.has_video {
            MediaKind::Video
        } else {
            MediaKind::Audio
        },
        source_path: asset.file_path.clone(),
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
        thumbnail_path: None,
    });

    let add_clip_to = |built: &mut BuiltProject, track_id: String| {
        let clip_id = Uuid::new_v4().to_string();
        built.project.clips.push(Clip {
            id: clip_id.clone(),
            track_id: track_id.clone(),
            media_id: Some(media_id.clone()),
            source_in_us: 0,
            source_out_us: probed.duration_us,
            position_us,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
        if let Some(track) = built.project.tracks.iter_mut().find(|t| t.id == track_id) {
            track.clip_ids.push(clip_id);
        }
    };

    if probed.has_video {
        if let Some(video_track_id) = built.video_track_id.clone() {
            add_clip_to(built, video_track_id);
        }
    }
    if probed.has_audio {
        if let Some(audio_track_id) = built.audio_track_id.clone() {
            add_clip_to(built, audio_track_id);
        }
    }
}

/// STUDIO_PLAN.md Phase S2: splices a template's `intro`/`outro` asset onto
/// the built project's own main video/audio tracks — real µs-accurate
/// timeline math driven by the asset's own real probed duration
/// (`media::probe::probe`), never an assumed/hardcoded one, matching every
/// other duration-aware code path in this codebase.
///
/// Design: onto the SAME video/audio tracks `build_whole_media_project`
/// already created (not a new track) — see `splice_clip`'s own doc comment.
/// There is no existing "insert a clip and shift everything after it" helper
/// anywhere in this codebase to reuse (`timeline::command`'s real command
/// set is trim/cut/delete/split, not insert-with-shift), so this is a small,
/// direct, well-tested addition rather than a second competing mechanism.
///
/// Outro is spliced first — appended at the pre-intro content's own end, so
/// no existing clip needs to move for that — then intro, which shifts EVERY
/// existing clip (including the just-added outro clip) and every caption
/// later by the intro's own real duration. This ordering just keeps each
/// step's math independent and simple; it has no semantic significance
/// (final positions come out identical either way).
fn apply_intro_outro(
    io: &PipelineIo,
    built: &mut BuiltProject,
    template: &Template,
) -> Result<(), BatchError> {
    if let Some(outro) = &template.outro {
        let asset = resolve_asset(io.assets_dir, &outro.asset_id)?;
        let probed = probe::probe(io.ffprobe, Path::new(&asset.file_path))
            .map_err(|e| stage_failed("Editing", e))?;
        let start_us = built_content_duration_us(built);
        splice_clip(built, &asset, &probed, start_us);
    }
    if let Some(intro) = &template.intro {
        let asset = resolve_asset(io.assets_dir, &intro.asset_id)?;
        let probed = probe::probe(io.ffprobe, Path::new(&asset.file_path))
            .map_err(|e| stage_failed("Editing", e))?;
        let intro_duration_us = probed.duration_us.max(0);
        shift_existing_content(built, intro_duration_us);
        splice_clip(built, &asset, &probed, 0);
    }
    Ok(())
}

/// Inverts `render::plan::build_video_clip_filter`'s own `overlay_x`/
/// `overlay_y` pixel-offset formula (that module's doc comment: half-canvas-
/// unit `transform_x`/`transform_y`, `transform_y` positive-up, negated to
/// ffmpeg's y-down pixel space) to find the `(transform_x, transform_y)` that
/// lands a `logo_w`x`logo_h` image at `position`'s corner, inset
/// `project::types::SafeMargins::default()`'s own 5%-of-canvas margin away
/// from the edge — the same safe-margin fraction captions already use,
/// reused here rather than inventing a second margin convention.
fn watermark_transform(
    position: WatermarkPosition,
    canvas_w: u32,
    canvas_h: u32,
    logo_w: u32,
    logo_h: u32,
) -> (f64, f64) {
    let margin_x = 0.05 * canvas_w as f64;
    let margin_y = 0.05 * canvas_h as f64;
    let (overlay_x, overlay_y) = match position {
        WatermarkPosition::TopLeft => (margin_x, margin_y),
        WatermarkPosition::TopRight => (canvas_w as f64 - logo_w as f64 - margin_x, margin_y),
        WatermarkPosition::BottomLeft => (margin_x, canvas_h as f64 - logo_h as f64 - margin_y),
        WatermarkPosition::BottomRight => (
            canvas_w as f64 - logo_w as f64 - margin_x,
            canvas_h as f64 - logo_h as f64 - margin_y,
        ),
        WatermarkPosition::Center => (
            (canvas_w as f64 - logo_w as f64) / 2.0,
            (canvas_h as f64 - logo_h as f64) / 2.0,
        ),
    };
    let center_x = (canvas_w as f64 - logo_w as f64) / 2.0;
    let center_y = (canvas_h as f64 - logo_h as f64) / 2.0;
    let transform_x = if canvas_w > 0 {
        (overlay_x - center_x) / (canvas_w as f64 / 2.0)
    } else {
        0.0
    };
    // `overlay_y = center_y - transform_y * canvas_h/2` (module doc comment's
    // y-up/y-down negation) solved for `transform_y`.
    let transform_y = if canvas_h > 0 {
        (center_y - overlay_y) / (canvas_h as f64 / 2.0)
    } else {
        0.0
    };
    (transform_x, transform_y)
}

/// STUDIO_PLAN.md Phase S2's watermark decision: reuses the EXISTING real
/// video/image overlay-compositing engine this codebase already has for any
/// `TrackKind::Overlay` track — `render::graph::is_visual_kind`/
/// `render::plan::build_video_clip_filter` already walk an `Overlay` track
/// identically to a `Video` track (chained ffmpeg `overlay` filters, time-
/// windowed `enable=`), and `capcut::graph` does the same for CapCut export.
/// No new ffmpeg filter or CapCut segment type is needed — a watermark is
/// simply a real still-image clip on a new `Overlay`-kind track, positioned
/// by `WatermarkPosition` via `watermark_transform` above (the exact same
/// `ClipSettings::transform_x/y` half-canvas-unit convention every other
/// visual clip already uses), spanning the built project's own final
/// content duration. Applied AFTER `apply_intro_outro`, so a watermark also
/// covers a spliced intro/outro, not just the main content.
///
/// Only a real still-image asset is supported
/// (`media::import::classify_extension` must resolve to `MediaKind::Image`)
/// — a video/animated watermark would need its own loop/trim handling this
/// pass does not add (a documented, honest scope limit, not a silent no-op):
/// an unsupported (non-image) watermark asset fails the job with a clear
/// `StageFailed` rather than silently doing nothing.
fn apply_watermark(
    io: &PipelineIo,
    built: &mut BuiltProject,
    template: &Template,
) -> Result<(), BatchError> {
    let Some(watermark) = &template.watermark else {
        return Ok(());
    };
    let asset = resolve_asset(io.assets_dir, &watermark.asset_id)?;
    let asset_path = Path::new(&asset.file_path);
    if classify_extension(asset_path) != Some(MediaKind::Image) {
        return Err(stage_failed(
            "Editing",
            format!(
                "watermark asset {} ({}) is not a supported still-image file — only a real \
                 image watermark can be composited today",
                asset.id, asset.file_path
            ),
        ));
    }
    let probed = probe::probe(io.ffprobe, asset_path).map_err(|e| stage_failed("Editing", e))?;
    let duration_us = built_content_duration_us(built);
    if duration_us <= 0 {
        return Ok(()); // nothing to watermark
    }

    let canvas_w = built.project.canvas.width;
    let canvas_h = built.project.canvas.height;
    let (logo_w, logo_h) = if probed.width == 0 || probed.height == 0 {
        // Matches `render::plan::build_video_clip_filter`'s own fallback for
        // unknown media dimensions: fill the canvas rather than requesting
        // an invalid 0x0 scale.
        (canvas_w, canvas_h)
    } else {
        (probed.width, probed.height)
    };
    let (transform_x, transform_y) =
        watermark_transform(watermark.position, canvas_w, canvas_h, logo_w, logo_h);

    let media_id = Uuid::new_v4().to_string();
    built.project.media.push(MediaItem {
        id: media_id.clone(),
        kind: MediaKind::Image,
        source_path: asset.file_path.clone(),
        duration_us,
        width: probed.width,
        height: probed.height,
        fps: built.project.canvas.fps,
        codec: probed.codec.clone(),
        bitrate: probed.bitrate,
        audio_channels: 0,
        sample_rate: 0,
        rotation_deg: probed.rotation_deg,
        created_at: probed.created_at.clone(),
        proxy_path: None,
        thumbnail_path: None,
    });

    // Highest render_index among the built project's own visual (Video/
    // Image/Overlay) tracks, so the watermark always composites on top —
    // the `Caption` track's own `render_index: 1` (`build_whole_media_project`)
    // is deliberately excluded here, since `render::graph::is_visual_kind`
    // never treats it as a compositing layer in the first place.
    let render_index = built
        .project
        .tracks
        .iter()
        .filter(|t| {
            matches!(
                t.kind,
                TrackKind::Video | TrackKind::Image | TrackKind::Overlay
            )
        })
        .map(|t| t.render_index)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    let track_id = Uuid::new_v4().to_string();
    let clip_id = Uuid::new_v4().to_string();
    built.project.tracks.push(Track {
        id: track_id.clone(),
        kind: TrackKind::Overlay,
        name: "Watermark".to_string(),
        render_index,
        locked: false,
        hidden: false,
        muted: false,
        solo: false,
        clip_ids: vec![clip_id.clone()],
    });
    built.project.clips.push(Clip {
        id: clip_id,
        track_id,
        media_id: Some(media_id),
        source_in_us: 0,
        source_out_us: duration_us,
        position_us: 0,
        speed: 1.0,
        enabled: true,
        group_id: None,
        clip_settings: ClipSettings {
            transform_x,
            transform_y,
            ..ClipSettings::default()
        },
    });
    Ok(())
}

/// STUDIO_PLAN.md Phase S2: inserts a template's `background_music` asset as
/// a real new `AudioRole::Music` track spanning the built project's own
/// final content duration (applied AFTER `apply_intro_outro`, so it also
/// covers a spliced intro/outro), using the asset's own real file and the
/// template's own `volume` as a linear gain
/// (`BackgroundMusicReference::volume`'s own doc comment convention) via
/// `ProjectV1::audio_clip_settings` — the exact existing real per-clip
/// audio-feature overlay `render::plan` already turns into a real ffmpeg
/// `volume` filter, never a second volume mechanism.
///
/// No looping: if the music asset's own real probed duration is shorter than
/// the project, the track simply plays once and then falls silent for the
/// remainder — a documented, honest simplification (this pass does not add
/// loop-splicing).
///
/// Ducking (`render::audio_filters::ducking_filter_chain`, already real and
/// used elsewhere) is only wired up when the resolved template also carries
/// `sports_overlay` — its `music_ducking` field is the one real,
/// already-populated `DuckingSettings` a template exposes today
/// (`background_music`/`sports_overlay` are independent optional fields, so
/// a template with music but no `sports_overlay` simply mixes the music in
/// at its configured volume, unducked): the main content's own audio track
/// is marked `AudioRole::Voice` (giving `compute_voice_speech_segments` a
/// real signal to score against) and the new music track gets a
/// `track_ducking` entry from `music_ducking`.
fn apply_background_music(
    io: &PipelineIo,
    built: &mut BuiltProject,
    template: &Template,
) -> Result<(), BatchError> {
    let Some(bg) = &template.background_music else {
        return Ok(());
    };
    let project_duration_us = built_content_duration_us(built);
    if project_duration_us <= 0 {
        return Ok(());
    }
    let asset = resolve_asset(io.assets_dir, &bg.asset_id)?;
    let probed = probe::probe(io.ffprobe, Path::new(&asset.file_path))
        .map_err(|e| stage_failed("Editing", e))?;
    let clip_duration_us = probed.duration_us.max(0).min(project_duration_us);
    if clip_duration_us <= 0 {
        return Ok(());
    }

    let media_id = Uuid::new_v4().to_string();
    built.project.media.push(MediaItem {
        id: media_id.clone(),
        kind: MediaKind::Audio,
        source_path: asset.file_path.clone(),
        duration_us: probed.duration_us,
        width: 0,
        height: 0,
        fps: built.project.canvas.fps,
        codec: probed.codec.clone(),
        bitrate: probed.bitrate,
        audio_channels: probed.audio_channels,
        sample_rate: probed.sample_rate,
        rotation_deg: 0,
        created_at: probed.created_at.clone(),
        proxy_path: None,
        thumbnail_path: None,
    });

    let track_id = Uuid::new_v4().to_string();
    let clip_id = Uuid::new_v4().to_string();
    built.project.tracks.push(Track {
        id: track_id.clone(),
        kind: TrackKind::Audio,
        name: "Background Music".to_string(),
        render_index: 0,
        locked: false,
        hidden: false,
        muted: false,
        solo: false,
        clip_ids: vec![clip_id.clone()],
    });
    built.project.clips.push(Clip {
        id: clip_id.clone(),
        track_id: track_id.clone(),
        media_id: Some(media_id),
        source_in_us: 0,
        source_out_us: clip_duration_us,
        position_us: 0,
        speed: 1.0,
        enabled: true,
        group_id: None,
        clip_settings: ClipSettings::default(),
    });
    built.project.audio_clip_settings.insert(
        clip_id,
        AudioClipSettings {
            volume: bg.volume,
            ..AudioClipSettings::default()
        },
    );
    built
        .project
        .audio_track_roles
        .insert(track_id.clone(), AudioRole::Music);

    if let Some(overlay) = &template.sports_overlay {
        if let Some(voice_track_id) = &built.audio_track_id {
            built
                .project
                .audio_track_roles
                .insert(voice_track_id.clone(), AudioRole::Voice);
        }
        built
            .project
            .track_ducking
            .insert(track_id, overlay.music_ducking);
    }
    Ok(())
}

/// Generalizes `shorts::captions::slice_transcript_for_span`'s "clip and
/// retime relative to one span" logic across *every surviving clip fragment*
/// left after silence cuts split/trimmed the original whole-media clip —
/// exactly what "Remove silence -> Generate captions" (master prompt §42's
/// own pipeline order) requires: a transcript word at source time `T` that
/// survived (its fragment's `[source_in_us, source_out_us)` still contains
/// it) must land at `fragment.position_us + (T - fragment.source_in_us)` on
/// the now-shorter edited timeline, not still at its original absolute `T`;
/// a word that fell inside a *removed* gap (no fragment covers it) is
/// correctly dropped, since `slice_transcript_for_span` already drops
/// anything outside the span it's given.
///
/// `fragments` must be `(source_in_us, source_out_us, position_us)` triples,
/// already sorted by `position_us` (the order they appear on the edited
/// timeline) — callers get this from a track's real `Clip`s, sorted by
/// `position_us` (never trusted from `Track::clip_ids`' own order, which
/// `render::graph::build_render_graph` itself does not trust either).
fn remap_transcript_across_fragments(
    transcript: &[TranscriptEntry],
    fragments: &[(i64, i64, i64)],
) -> Vec<TranscriptEntry> {
    let mut out = Vec::new();
    for &(source_in_us, source_out_us, position_us) in fragments {
        if source_out_us <= source_in_us {
            continue;
        }
        let mut sliced = crate::shorts::captions::slice_transcript_for_span(
            transcript,
            source_in_us,
            source_out_us,
        );
        for entry in &mut sliced {
            entry.start_us += position_us;
            entry.end_us += position_us;
            for word in &mut entry.words {
                word.start_us += position_us;
                word.end_us += position_us;
            }
        }
        out.extend(sliced);
    }
    out.sort_by_key(|e| e.start_us);
    out
}

/// `suffix` is the `<stem>_<suffix>.<ext>` naming convention's own suffix —
/// `"edited"` for this pipeline's original single-template default, or a
/// per-template slug (`slugify_template_name`) for a multi-template batch's
/// job (`BatchPipelineConfig::output_suffix` doc comment covers the full
/// precedence/rationale).
///
/// `pub(crate)`, not private: `batch::dry_run` (upgrade-plan §18) computes a
/// dry run's real predicted output path through this exact function — the
/// same real naming logic a real batch job would use, never a re-derived
/// guess.
pub(crate) fn default_output_path(
    source: &Path,
    settings: &render::RenderSettings,
    suffix: &str,
) -> Result<PathBuf, BatchError> {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let out_dir = parent.join("batch_output");
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        stage_failed(
            "Rendering",
            format!("creating output directory {}: {e}", out_dir.display()),
        )
    })?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    Ok(out_dir.join(format!(
        "{stem}_{suffix}.{}",
        settings.container.extension()
    )))
}

/// Runs the full per-file pipeline against `media_path`, honoring `config`.
/// `cancel`/`pause` are the same `Arc<AtomicBool>` cooperative-cancellation
/// primitive `render::job`/`transcription` already use — checked at every
/// stage boundary (`checkpoint`) and threaded through to each reused stage
/// core's own existing cancellation support (VAD's `score_chunks`, whisper's
/// `transcribe_with_progress`, `render::run_render_job`) so an
/// already-in-flight stage can also abort promptly, not just between stages.
///
/// `on_progress` receives `(status, stage_label, overall_progress)` on every
/// meaningful step; it is `Arc<dyn Fn>` (not `FnMut`) specifically because
/// whisper-rs's own progress/abort callbacks require a `'static`-owned
/// closure (`transcription::whisper::WhisperProvider::transcribe_with_progress`'s
/// own doc comment) — an `Arc` clone satisfies that while still letting
/// every call site (including `render::run_render_job`'s own non-`'static`
/// callback parameter) share the same one callback value.
pub fn run_pipeline(
    io: &PipelineIo,
    media_path: &Path,
    config: &BatchPipelineConfig,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync>,
) -> Result<PathBuf, BatchError> {
    // ---- Upfront validation: fail fast, before any real work starts ----
    let template = match &config.template_id {
        Some(id) => Some(resolve_template(io.templates_dir, id)?),
        None => None,
    };
    let needs_transcript = config.captions.is_some();
    if needs_transcript && config.transcription_model_id.is_none() {
        return Err(BatchError::TranscriptionModelRequired);
    }
    let export_preset_id = config
        .export_preset_id
        .clone()
        .or_else(|| template.as_ref().map(|t| t.export_preset_id.clone()))
        .ok_or(BatchError::ExportPresetRequired)?;
    let preset =
        render::find_preset(&export_preset_id).map_err(|e| stage_failed("Rendering", e))?;

    let weights = StageWeights::new(needs_transcript);

    // ---- Analyzing ----
    checkpoint(&cancel, &pause, "Queued", 0.0, &on_progress)?;
    on_progress(
        BatchJobStatus::Analyzing,
        "Analyzing media".to_string(),
        weights.overall(BatchJobStatus::Analyzing, 0.0),
    );
    if !media_path.exists() {
        return Err(BatchError::MediaNotFound {
            path: media_path.display().to_string(),
        });
    }
    let probed = probe::probe(io.ffprobe, media_path).map_err(|e| stage_failed("Analyzing", e))?;
    let mut built =
        build_whole_media_project(media_path, &probed, template.as_ref().map(|t| &t.canvas))?;
    on_progress(
        BatchJobStatus::Analyzing,
        "Analyzing media".to_string(),
        weights.overall(BatchJobStatus::Analyzing, 1.0),
    );

    // ---- Transcribing (only if captioning needs it) ----
    let mut transcript: Vec<TranscriptEntry> = Vec::new();
    if needs_transcript {
        checkpoint(
            &cancel,
            &pause,
            "Analyzing",
            weights.overall(BatchJobStatus::Analyzing, 1.0),
            &on_progress,
        )?;
        on_progress(
            BatchJobStatus::Transcribing,
            "Transcribing".to_string(),
            weights.overall(BatchJobStatus::Transcribing, 0.0),
        );

        let model_id_str = config
            .transcription_model_id
            .as_ref()
            .expect("checked Some above");
        let model_id = transcription::ModelId::from_str_id(model_id_str).map_err(|_| {
            BatchError::UnknownTranscriptionModel {
                model_id: model_id_str.clone(),
            }
        })?;
        if !transcription::is_installed(io.models_dir, model_id) {
            return Err(BatchError::TranscriptionModelNotInstalled {
                model_id: model_id_str.clone(),
            });
        }
        let model_path = io
            .models_dir
            .join(transcription::catalog_entry(model_id).filename);

        let samples =
            pcm::extract_pcm(io.ffmpeg, media_path).map_err(|e| stage_failed("Transcribing", e))?;
        let provider = transcription::WhisperProvider::load(&model_path)
            .map_err(|e| stage_failed("Transcribing", e))?;

        let media_id_for_entries = built.media_id.clone();
        let progress_cb = on_progress.clone();
        let segments = provider
            .transcribe_with_progress(
                &samples,
                pcm::PCM_SAMPLE_RATE,
                config.transcription_language.as_deref(),
                Some(cancel.clone()),
                move |percent: i32| {
                    progress_cb(
                        BatchJobStatus::Transcribing,
                        "Transcribing".to_string(),
                        weights.overall(BatchJobStatus::Transcribing, percent as f32 / 100.0),
                    );
                },
            )
            .map_err(|e| {
                if matches!(e, transcription::TranscriptionError::Cancelled) {
                    BatchError::Cancelled
                } else {
                    stage_failed("Transcribing", e)
                }
            })?;

        transcript = segments
            .into_iter()
            .map(|s| TranscriptEntry {
                id: Uuid::new_v4().to_string(),
                media_id: media_id_for_entries.clone(),
                text: s.text,
                start_us: s.start_us,
                end_us: s.end_us,
                confidence: s.confidence,
                words: s.words,
                is_filler: false,
            })
            .collect();
        on_progress(
            BatchJobStatus::Transcribing,
            "Transcribing".to_string(),
            weights.overall(BatchJobStatus::Transcribing, 1.0),
        );
    }

    // ---- Editing: silence removal, then captions, then template settings ----
    checkpoint(
        &cancel,
        &pause,
        if needs_transcript {
            "Transcribing"
        } else {
            "Analyzing"
        },
        weights.overall(
            if needs_transcript {
                BatchJobStatus::Transcribing
            } else {
                BatchJobStatus::Analyzing
            },
            1.0,
        ),
        &on_progress,
    )?;
    on_progress(
        BatchJobStatus::Editing,
        "Removing silence".to_string(),
        weights.overall(BatchJobStatus::Editing, 0.0),
    );

    let effective_cut_params = config
        .remove_silence
        .or_else(|| template.as_ref().map(|t| t.silence_settings));
    if let Some(cut_params) = effective_cut_params {
        let samples =
            pcm::extract_pcm(io.ffmpeg, media_path).map_err(|e| stage_failed("Editing", e))?;
        let chunks = vad::SileroVadProvider
            .score_chunks(&samples, pcm::PCM_SAMPLE_RATE, Some(cancel.as_ref()))
            .map_err(|e| {
                if matches!(e, VadError::Cancelled) {
                    BatchError::Cancelled
                } else {
                    stage_failed("Editing", e)
                }
            })?;
        let segments = vad::segments_from_scores(&chunks, VadParams::default(), 0);
        let cuts = vad::build_cuts_from_speech_segments(
            &segments,
            &built.media_id,
            probed.duration_us,
            cut_params,
        );

        if let Some(video_track_id) = &built.video_track_id {
            let cmd = timeline_silence::apply_cuts_to_track(&built.project, video_track_id, &cuts)
                .map_err(|e| stage_failed("Editing", e))?;
            cmd.apply(&mut built.project)
                .map_err(|e| stage_failed("Editing", e))?;
        }
        if let Some(audio_track_id) = &built.audio_track_id {
            let cmd = timeline_silence::apply_cuts_to_track(&built.project, audio_track_id, &cuts)
                .map_err(|e| stage_failed("Editing", e))?;
            cmd.apply(&mut built.project)
                .map_err(|e| stage_failed("Editing", e))?;
        }
    }
    on_progress(
        BatchJobStatus::Editing,
        "Removing silence".to_string(),
        weights.overall(BatchJobStatus::Editing, 0.4),
    );

    if needs_transcript {
        checkpoint(
            &cancel,
            &pause,
            "Editing",
            weights.overall(BatchJobStatus::Editing, 0.4),
            &on_progress,
        )?;
        on_progress(
            BatchJobStatus::Editing,
            "Generating captions".to_string(),
            weights.overall(BatchJobStatus::Editing, 0.4),
        );

        let primary_track_id = built
            .video_track_id
            .clone()
            .or_else(|| built.audio_track_id.clone())
            .expect("build_whole_media_project already rejects no-video-and-no-audio media");
        let mut fragments: Vec<(i64, i64, i64)> = built
            .project
            .clips
            .iter()
            .filter(|c| c.track_id == primary_track_id)
            .map(|c| (c.source_in_us, c.source_out_us, c.position_us))
            .collect();
        fragments.sort_by_key(|f| f.2);

        let remapped = remap_transcript_across_fragments(&transcript, &fragments);
        let caption_settings = config.captions.expect("checked needs_transcript above");
        let mut captions =
            captions_generate::generate_captions_from_transcript(&remapped, &caption_settings);

        let style_id = template.as_ref().map(|t| {
            if !built
                .project
                .caption_styles
                .iter()
                .any(|s| s.id == t.caption_style.id)
            {
                built.project.caption_styles.push(t.caption_style.clone());
            }
            t.caption_style.id.clone()
        });
        for caption in &mut captions {
            caption.track_id = built.caption_track_id.clone();
            caption.style_id = style_id.clone();
        }
        built.project.captions = captions;
    }
    // ---- Template asset references: intro/outro splice, watermark overlay,
    //      background music (STUDIO_PLAN.md Phase S2) — a no-op per field
    //      when the resolved template doesn't set it.
    if let Some(t) = &template {
        apply_intro_outro(io, &mut built, t)?;
        apply_watermark(io, &mut built, t)?;
        apply_background_music(io, &mut built, t)?;
    }
    on_progress(
        BatchJobStatus::Editing,
        "Editing complete".to_string(),
        weights.overall(BatchJobStatus::Editing, 1.0),
    );

    // ---- Rendering ----
    checkpoint(
        &cancel,
        &pause,
        "Editing",
        weights.overall(BatchJobStatus::Editing, 1.0),
        &on_progress,
    )?;
    on_progress(
        BatchJobStatus::Rendering,
        "Rendering".to_string(),
        weights.overall(BatchJobStatus::Rendering, 0.0),
    );

    let detected = render::detect_encoders(io.ffmpeg).map_err(|e| stage_failed("Rendering", e))?;
    let mut settings = preset.settings;
    settings.hardware_encoder = Some(render::resolve_backend_for_render(None, &detected));
    settings
        .validate()
        .map_err(|e| stage_failed("Rendering", e))?;

    let graph =
        render::build_render_graph(&built.project).map_err(|e| stage_failed("Rendering", e))?;
    let output_suffix = config.output_suffix.as_deref().unwrap_or("edited");
    let output_path = default_output_path(media_path, &settings, output_suffix)?;
    // Real voice-presence signal driving `apply_background_music`'s own
    // ducking wiring (STUDIO_PLAN.md Phase S2): only a template with both
    // `background_music` and `sports_overlay` set ever marks the main
    // content's own audio track `AudioRole::Voice` — every other batch job
    // (still the common case) has no `Voice`-role track at all, so this
    // stays the same zero-cost `&[]` this pipeline always used before,
    // without re-deriving `compute_voice_speech_segments`'s own real
    // PCM-extraction + VAD-scoring logic a second time (it's reused
    // directly, unchanged, from `commands::render` — see that function's
    // own doc comment).
    let voice_speech_segments = if built
        .project
        .audio_track_roles
        .values()
        .any(|role| *role == AudioRole::Voice)
    {
        crate::commands::render::compute_voice_speech_segments(io.ffmpeg, &built.project)
            .map_err(|e| stage_failed("Rendering", e.message))?
    } else {
        Vec::new()
    };
    let plan = render::build_ffmpeg_plan(&graph, &settings, &output_path, &voice_speech_segments)
        .map_err(|e| stage_failed("Rendering", e))?;

    let render_progress_cb = on_progress.clone();
    render::run_render_job(
        io.ffmpeg,
        &plan,
        &output_path,
        Some(cancel.as_ref()),
        move |p: render::RenderJobProgress| {
            if let Some(fraction) = p.fraction {
                render_progress_cb(
                    BatchJobStatus::Rendering,
                    "Rendering".to_string(),
                    weights.overall(BatchJobStatus::Rendering, fraction as f32),
                );
            }
        },
    )
    .map_err(|e| {
        if matches!(e, render::RenderError::Cancelled) {
            BatchError::Cancelled
        } else {
            stage_failed("Rendering", e)
        }
    })?;

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::AssetKind;
    use crate::project::{CanvasRatioPreset, DuckingSettings, Rational, Word};
    use crate::templates::{
        AssetReference, BackgroundMusicReference, SportsOverlaySettings, WatermarkReference,
    };
    use crate::vad::CutParams;

    fn entry(
        id: &str,
        text: &str,
        start_us: i64,
        end_us: i64,
        words: Vec<Word>,
    ) -> TranscriptEntry {
        TranscriptEntry {
            id: id.to_string(),
            media_id: "m1".to_string(),
            text: text.to_string(),
            start_us,
            end_us,
            confidence: 0.9,
            words,
            is_filler: false,
        }
    }

    // -- StageWeights ---------------------------------------------------

    #[test]
    fn stage_weights_sum_to_one_with_and_without_transcription() {
        for needs_transcript in [true, false] {
            let w = StageWeights::new(needs_transcript);
            let total = w.analyzing + w.transcribing + w.editing + w.rendering;
            assert!((total - 1.0).abs() < 1e-6, "{needs_transcript}: {total}");
        }
    }

    #[test]
    fn stage_offsets_are_monotonically_increasing() {
        let w = StageWeights::new(true);
        let a = w.overall(BatchJobStatus::Analyzing, 1.0);
        let t = w.overall(BatchJobStatus::Transcribing, 0.0);
        let e = w.overall(BatchJobStatus::Editing, 0.0);
        let r = w.overall(BatchJobStatus::Rendering, 0.0);
        assert!(a <= t && t <= e && e <= r, "{a} {t} {e} {r}");
        assert!((w.overall(BatchJobStatus::Rendering, 1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn transcribing_has_zero_weight_when_not_needed() {
        let w = StageWeights::new(false);
        assert_eq!(w.transcribing, 0.0);
        // Editing's offset must immediately follow Analyzing's end with no
        // gap when Transcribing is skipped.
        assert_eq!(
            w.overall(BatchJobStatus::Analyzing, 1.0),
            w.offset(BatchJobStatus::Editing)
        );
    }

    // -- remap_transcript_across_fragments -------------------------------

    #[test]
    fn a_word_inside_a_surviving_fragment_is_remapped_to_its_new_position() {
        // Original media: silence [0,2s) cut out; surviving fragment is
        // source [2s,10s) now placed at timeline position 0.
        let transcript = vec![entry(
            "e1",
            "hello",
            3_000_000,
            4_000_000,
            vec![Word {
                text: "hello".into(),
                start_us: 3_000_000,
                end_us: 4_000_000,
                confidence: 0.9,
            }],
        )];
        let fragments = vec![(2_000_000, 10_000_000, 0)];
        let remapped = remap_transcript_across_fragments(&transcript, &fragments);
        assert_eq!(remapped.len(), 1);
        // 3s source -> (3s - 2s) + 0 timeline position = 1s.
        assert_eq!(remapped[0].start_us, 1_000_000);
        assert_eq!(remapped[0].end_us, 2_000_000);
    }

    #[test]
    fn a_word_inside_a_removed_gap_is_dropped() {
        let transcript = vec![entry("e1", "gone", 500_000, 900_000, vec![])];
        // Two surviving fragments: source [0,500000) at position 0, and
        // source [900000, 2000000) placed right after at position 500000 —
        // the word's own span [500000,900000) is exactly the removed gap.
        let fragments = vec![(0, 500_000, 0), (900_000, 2_000_000, 500_000)];
        let remapped = remap_transcript_across_fragments(&transcript, &fragments);
        assert!(remapped.is_empty(), "{remapped:?}");
    }

    #[test]
    fn multiple_surviving_fragments_each_remap_independently_and_stay_time_ordered() {
        let transcript = vec![
            entry("e1", "first", 100_000, 200_000, vec![]),
            entry("e2", "second", 1_100_000, 1_200_000, vec![]),
        ];
        // Fragment A: source [0,300000) -> position 0.
        // Fragment B: source [1000000,1300000) -> position 300000 (right
        // after fragment A on the edited timeline).
        let fragments = vec![(0, 300_000, 0), (1_000_000, 1_300_000, 300_000)];
        let remapped = remap_transcript_across_fragments(&transcript, &fragments);
        assert_eq!(remapped.len(), 2);
        assert_eq!(remapped[0].start_us, 100_000); // unchanged: fragment A starts at position 0
        assert_eq!(remapped[1].start_us, 300_000 + (1_100_000 - 1_000_000));
        let starts: Vec<i64> = remapped.iter().map(|e| e.start_us).collect();
        let mut sorted = starts.clone();
        sorted.sort();
        assert_eq!(starts, sorted);
    }

    #[test]
    fn a_degenerate_zero_length_fragment_is_skipped_without_panicking() {
        let transcript = vec![entry("e1", "x", 0, 100_000, vec![])];
        let fragments = vec![(50_000, 50_000, 0)];
        assert!(remap_transcript_across_fragments(&transcript, &fragments).is_empty());
    }

    // -- build_whole_media_project ----------------------------------------

    fn probed(has_video: bool, has_audio: bool) -> ProbedMedia {
        ProbedMedia {
            duration_us: 5_000_000,
            width: if has_video { 1920 } else { 0 },
            height: if has_video { 1080 } else { 0 },
            fps: Rational::new(30, 1),
            codec: "h264".to_string(),
            bitrate: 1_000_000,
            audio_channels: if has_audio { 2 } else { 0 },
            sample_rate: if has_audio { 48_000 } else { 0 },
            rotation_deg: 0,
            created_at: None,
            has_video,
            has_audio,
        }
    }

    #[test]
    fn video_and_audio_media_gets_both_a_video_and_an_audio_track() {
        let built =
            build_whole_media_project(Path::new("clip.mp4"), &probed(true, true), None).unwrap();
        assert!(built.video_track_id.is_some());
        assert!(built.audio_track_id.is_some());
        assert_eq!(built.project.clips.len(), 2);
        assert_eq!(built.project.media.len(), 1);
    }

    #[test]
    fn audio_only_media_gets_only_an_audio_track() {
        let built =
            build_whole_media_project(Path::new("clip.mp3"), &probed(false, true), None).unwrap();
        assert!(built.video_track_id.is_none());
        assert!(built.audio_track_id.is_some());
        assert_eq!(built.project.clips.len(), 1);
    }

    #[test]
    fn media_with_neither_video_nor_audio_is_rejected() {
        let err = build_whole_media_project(Path::new("clip.bin"), &probed(false, false), None)
            .unwrap_err();
        assert!(matches!(err, BatchError::UnsupportedMedia { .. }));
    }

    #[test]
    fn a_canvas_override_is_applied_to_the_built_project() {
        let canvas = CanvasV1 {
            width: 1080,
            height: 1920,
            fps: Rational::new(30, 1),
            ratio_preset: CanvasRatioPreset::Ratio9x16,
        };
        let built =
            build_whole_media_project(Path::new("clip.mp4"), &probed(true, true), Some(&canvas))
                .unwrap();
        assert_eq!(built.project.canvas.width, 1080);
        assert_eq!(built.project.canvas.height, 1920);
    }

    // -- resolve_template ---------------------------------------------------

    #[test]
    fn resolve_template_finds_a_built_in_by_id() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmpl-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = resolve_template(&dir, "tmpl_tiktok").expect("built-in should resolve");
        assert_eq!(t.id, "tmpl_tiktok");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_template_errors_on_an_unknown_id() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmpl-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = resolve_template(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, BatchError::UnknownTemplate { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_template_finds_a_saved_custom_template() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmpl-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut custom = templates::all_templates().remove(0);
        custom.id = "custom_test123".to_string();
        custom.is_built_in = false;
        template_io::save_custom_template(&dir, &custom).expect("save custom template");

        let found = resolve_template(&dir, "custom_test123").expect("custom should resolve");
        assert_eq!(found.id, "custom_test123");
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- resolve_template_name -----------------------------------------------

    #[test]
    fn resolve_template_name_returns_the_real_display_name_for_a_built_in() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmplname-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = resolve_template_name(&dir, "tmpl_youtube_shorts")
            .expect("built-in should resolve a name");
        assert_eq!(name, "YouTube Shorts");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_template_name_errors_on_an_unknown_id() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmplname-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = resolve_template_name(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, BatchError::UnknownTemplate { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- resolve_template_version (upgrade-plan §21's `HistoryEntry::template_version`) --

    #[test]
    fn resolve_template_version_is_1_for_a_built_in() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmplver-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let version = resolve_template_version(&dir, "tmpl_tiktok")
            .expect("built-in should resolve a version");
        assert_eq!(version, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_template_version_errors_on_an_unknown_id() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmplver-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = resolve_template_version(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, BatchError::UnknownTemplate { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_template_version_reflects_a_saved_custom_templates_real_version() {
        let dir = std::env::temp_dir().join(format!("ave-batch-tmplver-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut custom = templates::all_templates().remove(0);
        custom.id = "custom_v3test".to_string();
        custom.is_built_in = false;
        custom.version = 3;
        template_io::save_custom_template(&dir, &custom).expect("save custom template");

        let version = resolve_template_version(&dir, "custom_v3test")
            .expect("custom should resolve a version");
        assert_eq!(version, 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- slugify_template_name (§11's exact <stem>_<slug> naming convention) --

    #[test]
    fn slugify_template_name_lowercases_a_simple_name() {
        assert_eq!(slugify_template_name("TikTok"), "tiktok");
    }

    #[test]
    fn slugify_template_name_replaces_spaces_with_a_single_underscore() {
        assert_eq!(slugify_template_name("YouTube Shorts"), "youtube_shorts");
    }

    #[test]
    fn slugify_template_name_collapses_runs_of_special_characters() {
        // Punctuation/whitespace runs collapse to exactly one `_`, never a
        // run of them — "Facebook Reel!!  (v2)" must not produce
        // "facebook_reel____v2_".
        assert_eq!(
            slugify_template_name("Facebook Reel!!  (v2)"),
            "facebook_reel_v2"
        );
    }

    #[test]
    fn slugify_template_name_trims_leading_and_trailing_separators() {
        assert_eq!(slugify_template_name("  My Template!!  "), "my_template");
    }

    #[test]
    fn slugify_template_name_handles_unicode_letters_by_dropping_them_as_separators() {
        // This slug function only special-cases ASCII alphanumerics (§11's
        // own worked examples are all plain ASCII); non-ASCII letters (e.g.
        // Vietnamese diacritics) collapse to underscores like any other
        // non-alphanumeric character rather than producing a non-filesystem
        // -safe raw Unicode slug.
        assert_eq!(slugify_template_name("Việt Nam"), "vi_t_nam");
    }

    #[test]
    fn slugify_template_name_falls_back_to_a_default_for_an_all_punctuation_name() {
        assert_eq!(slugify_template_name("!!!"), "template");
        assert_eq!(slugify_template_name(""), "template");
    }

    #[test]
    fn slugify_template_name_is_stable_and_idempotent() {
        let slug = slugify_template_name("Original");
        assert_eq!(slug, "original");
        assert_eq!(slugify_template_name(&slug), slug);
    }

    // -- default_output_path -------------------------------------------------

    #[test]
    fn default_output_path_lands_in_a_sibling_batch_output_folder() {
        let dir = std::env::temp_dir().join(format!("ave-batch-outpath-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("my clip.mp4");
        let settings = render::find_preset("p1080").unwrap().settings;
        let out = default_output_path(&source, &settings, "edited").expect("builds a path");
        assert_eq!(out.parent().unwrap(), dir.join("batch_output"));
        assert_eq!(
            out.file_name().unwrap().to_str().unwrap(),
            "my clip_edited.mp4"
        );
        assert!(dir.join("batch_output").is_dir());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_output_path_with_a_template_slug_suffix_matches_11s_naming_convention() {
        // §11's own worked example: video01.mp4 through the TikTok template
        // -> video01_tiktok.mp4.
        let dir =
            std::env::temp_dir().join(format!("ave-batch-outpath-slug-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("video01.mp4");
        let settings = render::find_preset("p1080").unwrap().settings;
        let slug = slugify_template_name("TikTok");
        let out = default_output_path(&source, &settings, &slug).expect("builds a path");
        assert_eq!(
            out.file_name().unwrap().to_str().unwrap(),
            "video01_tiktok.mp4"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- §88 Windows path edge cases for the batch output-path derivation --

    #[test]
    fn default_output_path_handles_a_real_vietnamese_and_unicode_source_filename() {
        let dir =
            std::env::temp_dir().join(format!("ave-batch-outpath-unicode-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("Việt Nam - Xin chào 🎬.mp4");
        let settings = render::find_preset("p1080").unwrap().settings;
        let out = default_output_path(&source, &settings, "edited").expect("builds a path");
        assert_eq!(out.parent().unwrap(), dir.join("batch_output"));
        assert_eq!(
            out.file_name().unwrap().to_str().unwrap(),
            "Việt Nam - Xin chào 🎬_edited.mp4"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_output_path_handles_a_very_long_source_path() {
        // Windows' classic MAX_PATH is 260 characters — this exercises this
        // function's own path-joining/stem-extraction logic, not Windows'
        // real enforcement of that limit (WSL2's filesystem doesn't
        // reproduce it; a real Windows build still needs separate
        // verification, including whether `\\?\` is required).
        let dir =
            std::env::temp_dir().join(format!("ave-batch-outpath-long-test-{}", Uuid::new_v4()));
        let mut nested = dir.clone();
        for i in 0..6 {
            nested = nested.join(format!(
                "a-very-long-nested-directory-segment-number-{i}-to-approach-the-windows-max-path-limit"
            ));
        }
        std::fs::create_dir_all(&nested).unwrap();
        let source = nested.join("clip.mp4");
        assert!(source.to_string_lossy().len() > 260);

        let settings = render::find_preset("p1080").unwrap().settings;
        let out = default_output_path(&source, &settings, "edited")
            .expect("builds a path even for a long source path");
        assert_eq!(out.parent().unwrap(), nested.join("batch_output"));
        assert_eq!(
            out.file_name().unwrap().to_str().unwrap(),
            "clip_edited.mp4"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_output_path_handles_a_unc_shaped_source_path_string() {
        // UNC paths are a Windows-specific string convention; real network
        // I/O against one can only be verified on real Windows. This
        // proves the stem-extraction/join logic doesn't mis-parse a
        // UNC-shaped string on this POSIX test environment, where the
        // backslashes are literal filename characters rather than
        // separators — `source.parent()`/`.file_stem()` must still behave
        // sanely rather than assuming forward-slash-only splitting.
        let dir =
            std::env::temp_dir().join(format!("ave-batch-outpath-unc-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Built via `format!`, not `dir.join(r"\\server\share\...")` —
        // `Path::join` replaces the whole path when its argument looks
        // absolute (a real behavior difference this test wants to avoid
        // triggering by accident; clippy's `join_absolute_paths` catches
        // exactly this). This keeps the UNC-shaped component nested inside
        // the real temp dir as a single filename, matching what this test
        // actually wants to exercise.
        let source = PathBuf::from(format!("{}/{}", dir.display(), r"\\server\share\clip.mp4"));
        let settings = render::find_preset("p1080").unwrap().settings;
        let out = default_output_path(&source, &settings, "edited")
            .expect("builds a path for a UNC-shaped name");
        assert_eq!(out.parent().unwrap(), dir.join("batch_output"));
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- run_pipeline: real end-to-end (export only, plus a dedicated
    //    deterministic silence-removal test below) ---------------------------

    fn synth_source(ffmpeg: &Path, dir: &Path) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let source = dir.join("in.mp4");
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=3",
                "-shortest",
            ])
            .path(&source);
        run_checked(ffmpeg, &args).expect("synthesizing test source");
        source
    }

    fn no_op_io<'a>(
        ffmpeg: &'a Path,
        ffprobe: &'a Path,
        models_dir: &'a Path,
        templates_dir: &'a Path,
        assets_dir: &'a Path,
    ) -> PipelineIo<'a> {
        PipelineIo {
            ffmpeg,
            ffprobe,
            models_dir,
            templates_dir,
            assets_dir,
        }
    }

    /// `remove_silence: None` deliberately — a synthetic sine-tone "speech"
    /// track is not reliably classified as real speech by the real Silero
    /// VAD model (same honest uncertainty
    /// `commands::render::compute_voice_speech_segments_finds_real_speech_on_a_voice_track`'s
    /// own test comment already documents), so enabling silence removal here
    /// would make whether the whole clip gets removed (and rendering
    /// correctly fails with `EmptyTimeline`) nondeterministic. This test's
    /// job is to prove *orchestration* correctness (state transitions,
    /// progress, Completed outcome) — `remove_silence`'s own real
    /// speech-detection-driven behavior is exercised deterministically by
    /// [`silence_removal_that_finds_no_speech_correctly_fails_the_job`]
    /// below (real silence, not a tone, so the VAD verdict is deterministic)
    /// and by `vad`/`timeline::silence`'s own extensive existing test suites.
    fn minimal_config() -> BatchPipelineConfig {
        BatchPipelineConfig {
            remove_silence: None,
            captions: None,
            transcription_model_id: None,
            transcription_language: None,
            template_id: None,
            export_preset_id: Some("fast_preview".to_string()),
            output_suffix: None,
        }
    }

    #[test]
    fn real_end_to_end_pipeline_completes_and_produces_a_real_output_file() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-e2e-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let assets_dir = dir.join("assets");

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let config = minimal_config();

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let seen: Arc<std::sync::Mutex<Vec<BatchJobStatus>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_for_cb = seen.clone();
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(move |status, _stage, _progress| {
                seen_for_cb.lock().unwrap().push(status);
            });

        let output = run_pipeline(&io, &source, &config, cancel, pause, on_progress)
            .expect("real end-to-end pipeline should complete");

        assert!(output.exists(), "expected a real rendered output file");
        let probed = crate::media::probe::probe(&ffprobe, &output).expect("probing output");
        assert!(probed.has_video);

        let statuses = seen.lock().unwrap().clone();
        assert!(statuses.contains(&BatchJobStatus::Analyzing));
        assert!(statuses.contains(&BatchJobStatus::Editing));
        assert!(statuses.contains(&BatchJobStatus::Rendering));
        assert!(!statuses.contains(&BatchJobStatus::Transcribing));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_pre_cancelled_job_stops_immediately_and_leaves_no_output_file() {
        // Same "already cancelled before the job starts" discipline
        // `render::job`/`media::proxy`'s own cancellation tests use.
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-cancel-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let assets_dir = dir.join("assets");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let config = minimal_config();

        let cancel = Arc::new(AtomicBool::new(true));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let result = run_pipeline(&io, &source, &config, cancel, pause, on_progress);
        assert!(matches!(result, Err(BatchError::Cancelled)));

        let settings = render::find_preset("fast_preview").unwrap().settings;
        let expected_output = default_output_path(&source, &settings, "edited").unwrap();
        assert!(
            !expected_output.exists(),
            "a cancelled job must never leave a partial output file behind"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Deterministic counterpart to `minimal_config`'s own doc comment: real
    /// *silence* (an `anullsrc` audio track, not a sine tone) is reliably
    /// scored as non-speech by the real Silero VAD, so `remove_silence`
    /// legitimately removes the entire timeline here — proving the real
    /// VAD -> cutlist -> `apply_cuts_to_track` chain actually ran (not a
    /// no-op), and that emptying the timeline this way is correctly
    /// surfaced as a real `Failed` job (via `RenderError::EmptyTimeline`),
    /// never silently swallowed or mistaken for success.
    #[test]
    fn silence_removal_that_finds_no_speech_correctly_fails_the_job() {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};

        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-silence-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("silent.mp4");
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=mono:duration=3",
                "-shortest",
            ])
            .path(&source);
        run_checked(&ffmpeg, &args).expect("synthesizing a real silent test source");

        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let assets_dir = dir.join("assets");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let mut config = minimal_config();
        config.remove_silence = Some(CutParams::default());

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let result = run_pipeline(&io, &source, &config, cancel, pause, on_progress);
        match result {
            Err(BatchError::StageFailed { stage, details }) => {
                assert_eq!(stage, "Rendering");
                assert!(
                    details.contains("no visible video/audio content"),
                    "{details}"
                );
            }
            other => panic!("expected a real Rendering StageFailed error, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_media_file_fails_the_analyzing_stage_with_a_real_error() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-missing-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("does-not-exist.mp4");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let assets_dir = dir.join("assets");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let config = minimal_config();

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let result = run_pipeline(&io, &missing, &config, cancel, pause, on_progress);
        assert!(matches!(result, Err(BatchError::MediaNotFound { .. })));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn captions_requested_without_a_transcription_model_id_errors_up_front() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-noModel-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");
        let assets_dir = dir.join("assets");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);

        let mut config = minimal_config();
        config.captions = Some(crate::captions::generate::CaptionGenerationSettings {
            max_words_per_line: 4,
            max_chars_per_line: 24,
            grouping: crate::captions::generate::CaptionGroupingMode::Word,
        });
        config.transcription_model_id = None;

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let result = run_pipeline(&io, &source, &config, cancel, pause, on_progress);
        assert!(matches!(
            result,
            Err(BatchError::TranscriptionModelRequired)
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- STUDIO_PLAN.md Phase S2: template asset references (intro/outro,
    //    watermark, background music) wired into the real build pipeline ----

    fn synth_video_with_duration(
        ffmpeg: &Path,
        dir: &Path,
        filename: &str,
        duration_secs: f64,
        freq: u32,
    ) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let path = dir.join(filename);
        let args = FfmpegArgs::new()
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!(
                "testsrc=duration={duration_secs}:size=320x240:rate=10"
            ))
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=frequency={freq}:duration={duration_secs}"))
            .arg("-shortest")
            .path(&path);
        run_checked(ffmpeg, &args)
            .expect("synthesizing a real test source with a specific duration");
        path
    }

    /// A tremolo-modulated 220Hz tone — the same real-Silero-VAD-detectable-
    /// as-one-confident-speech-segment-spanning-the-whole-clip fixture
    /// `batch::manager`'s own `synth_named_source` uses (see that function's
    /// doc comment: "verified directly ... before writing the tests below,
    /// not guessed"). Needed for every real end-to-end test below that
    /// selects a template: `BatchPipelineConfig::template_id`'s own doc
    /// comment means ANY selected template forces `remove_silence` on (a
    /// template's `silence_settings` is a plain, non-optional `CutParams`,
    /// always resolved as the fallback) — a plain sine tone would be
    /// unpredictably classified as non-speech and get the whole clip cut,
    /// making a real-duration assertion flaky; this fixture keeps the whole
    /// main clip intact so intro/outro/watermark/background-music splicing
    /// math stays checkable against exact expected durations.
    fn synth_speech_like_source(
        ffmpeg: &Path,
        dir: &Path,
        filename: &str,
        duration_secs: f64,
    ) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let path = dir.join(filename);
        let args = FfmpegArgs::new()
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!(
                "testsrc=duration={duration_secs}:size=320x240:rate=10"
            ))
            .args(["-f", "lavfi", "-i"])
            .arg(format!(
                "sine=frequency=220:duration={duration_secs},tremolo=f=4:d=0.9"
            ))
            .arg("-shortest")
            .path(&path);
        run_checked(ffmpeg, &args).expect("synthesizing a real speech-like test source");
        path
    }

    fn synth_image(ffmpeg: &Path, dir: &Path, filename: &str, size: &str) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let path = dir.join(filename);
        let args = FfmpegArgs::new()
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!("color=c=red:size={size}"))
            .args(["-frames:v", "1"])
            .path(&path);
        run_checked(ffmpeg, &args).expect("synthesizing a real watermark image");
        path
    }

    fn synth_audio_with_duration(
        ffmpeg: &Path,
        dir: &Path,
        filename: &str,
        duration_secs: f64,
    ) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let path = dir.join(filename);
        let args = FfmpegArgs::new()
            .args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(format!("sine=frequency=330:duration={duration_secs}"))
            .path(&path);
        run_checked(ffmpeg, &args).expect("synthesizing a real music test asset");
        path
    }

    fn register_asset(assets_dir: &Path, kind: AssetKind, name: &str, file_path: PathBuf) -> Asset {
        let asset = crate::assets::new_asset(
            kind,
            name.to_string(),
            file_path.to_string_lossy().to_string(),
        )
        .expect("new_asset");
        assets_io::save_asset(assets_dir, &asset).expect("save_asset");
        asset
    }

    /// A real custom template (based on the real `tmpl_talking_head`
    /// built-in, same "clone a real catalog entry, override the id" pattern
    /// `resolve_template_finds_a_saved_custom_template` above already uses)
    /// with a cheap, deterministic `export_preset_id` — callers set
    /// `intro`/`outro`/`watermark`/`background_music`/`sports_overlay`
    /// themselves.
    fn base_custom_template(id: &str) -> Template {
        let mut t = templates::all_templates()
            .into_iter()
            .find(|t| t.id == "tmpl_talking_head")
            .expect("tmpl_talking_head exists");
        t.id = id.to_string();
        t.is_built_in = false;
        t.export_preset_id = "fast_preview".to_string();
        t
    }

    // -- resolve_asset --------------------------------------------------------

    #[test]
    fn resolve_asset_errors_on_an_unknown_asset_id() {
        let dir = std::env::temp_dir().join(format!("ave-batch-asset-unknown-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = resolve_asset(&dir, "does_not_exist").unwrap_err();
        assert!(matches!(err, BatchError::StageFailed { stage, .. } if stage == "Editing"));
        std::fs::remove_dir_all(&dir).ok();
    }

    // -- apply_intro_outro ----------------------------------------------------

    #[test]
    fn apply_intro_outro_is_a_no_op_when_the_template_has_neither_intro_nor_outro_set() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-batch-introoutro-noop-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 1.0, 440);
        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let clip_count_before = built.project.clips.len();

        let template = base_custom_template("custom_no_intro_outro");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_intro_outro(&io, &mut built, &template).expect("no-op apply_intro_outro");

        assert_eq!(built.project.clips.len(), clip_count_before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_intro_outro_splices_real_probed_intro_and_outro_around_the_main_content_in_order() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-batch-introoutro-real-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 3.0, 440);
        let intro_source = synth_video_with_duration(&ffmpeg, &dir, "intro.mp4", 1.0, 220);
        let outro_source = synth_video_with_duration(&ffmpeg, &dir, "outro.mp4", 1.5, 660);

        let intro_asset =
            register_asset(&assets_dir, AssetKind::Intro, "Intro", intro_source.clone());
        let outro_asset =
            register_asset(&assets_dir, AssetKind::Outro, "Outro", outro_source.clone());

        let mut template = base_custom_template("custom_introoutro_real");
        template.intro = Some(AssetReference {
            asset_id: intro_asset.id.clone(),
        });
        template.outro = Some(AssetReference {
            asset_id: outro_asset.id.clone(),
        });

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let probed_intro = probe::probe(&ffprobe, &intro_source).unwrap();
        let probed_outro = probe::probe(&ffprobe, &outro_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_intro_outro(&io, &mut built, &template).expect("apply_intro_outro");

        let video_track_id = built
            .video_track_id
            .clone()
            .expect("main has a video track");
        let mut video_clips: Vec<_> = built
            .project
            .clips
            .iter()
            .filter(|c| c.track_id == video_track_id)
            .collect();
        video_clips.sort_by_key(|c| c.position_us);
        assert_eq!(
            video_clips.len(),
            3,
            "intro + main + outro clips: {video_clips:?}"
        );

        assert_eq!(video_clips[0].position_us, 0, "intro starts at 0");
        assert_eq!(video_clips[0].source_out_us, probed_intro.duration_us);

        assert_eq!(
            video_clips[1].position_us, probed_intro.duration_us,
            "main content shifted later by exactly the intro's real duration"
        );
        assert_eq!(video_clips[1].source_out_us, probed_main.duration_us);

        let expected_outro_position = probed_intro.duration_us + probed_main.duration_us;
        assert_eq!(
            video_clips[2].position_us, expected_outro_position,
            "outro placed right after the (now-shifted) main content"
        );
        assert_eq!(video_clips[2].source_out_us, probed_outro.duration_us);

        // The audio track gets the exact same real splice (both synthesized
        // sources have real audio too).
        let audio_track_id = built
            .audio_track_id
            .clone()
            .expect("main has an audio track");
        let audio_clip_count = built
            .project
            .clips
            .iter()
            .filter(|c| c.track_id == audio_track_id)
            .count();
        assert_eq!(audio_clip_count, 3);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_template_with_intro_and_outro_produces_a_real_rendered_output_whose_duration_reflects_all_three_parts_in_order(
    ) {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-introoutro-e2e-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_speech_like_source(&ffmpeg, &dir, "main.mp4", 3.0);
        let intro_source = synth_video_with_duration(&ffmpeg, &dir, "intro.mp4", 1.0, 220);
        let outro_source = synth_video_with_duration(&ffmpeg, &dir, "outro.mp4", 1.0, 660);

        let intro_asset = register_asset(&assets_dir, AssetKind::Intro, "Intro", intro_source);
        let outro_asset = register_asset(&assets_dir, AssetKind::Outro, "Outro", outro_source);

        let mut template = base_custom_template("custom_introoutro_e2e");
        template.intro = Some(AssetReference {
            asset_id: intro_asset.id,
        });
        template.outro = Some(AssetReference {
            asset_id: outro_asset.id,
        });
        template_io::save_custom_template(&templates_dir, &template).expect("save custom template");

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let mut config = minimal_config();
        config.template_id = Some(template.id.clone());
        config.export_preset_id = None; // falls back to the template's own "fast_preview"

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let output = run_pipeline(&io, &main_source, &config, cancel, pause, on_progress)
            .expect("real end-to-end pipeline with intro/outro should complete");

        let probed_output = probe::probe(&ffprobe, &output).expect("probing output");
        // 1s intro + 3s main (the speech-like source keeps its whole
        // duration — see `synth_speech_like_source`'s doc comment) + 1s
        // outro = ~5s.
        let expected_us = 5_000_000;
        assert!(
            (probed_output.duration_us - expected_us).abs() < 400_000,
            "expected ~{expected_us}us (intro+main+outro), got {}",
            probed_output.duration_us
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- apply_watermark --------------------------------------------------------

    #[test]
    fn apply_watermark_is_a_no_op_when_the_template_has_no_watermark_set() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-watermark-noop-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 1.0, 440);
        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let track_count_before = built.project.tracks.len();

        let template = base_custom_template("custom_no_watermark");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_watermark(&io, &mut built, &template).expect("no-op apply_watermark");

        assert_eq!(built.project.tracks.len(), track_count_before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_watermark_rejects_a_non_image_asset_with_a_clear_error() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-batch-watermark-notimage-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 1.0, 440);
        let not_an_image = synth_video_with_duration(&ffmpeg, &dir, "not_a_logo.mp4", 1.0, 300);
        let bad_asset = register_asset(&assets_dir, AssetKind::Watermark, "Bad Logo", not_an_image);

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();

        let mut template = base_custom_template("custom_watermark_bad_asset");
        template.watermark = Some(WatermarkReference {
            asset_id: bad_asset.id,
            position: WatermarkPosition::TopRight,
        });

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let err = apply_watermark(&io, &mut built, &template).unwrap_err();
        assert!(
            matches!(&err, BatchError::StageFailed { stage, .. } if stage == "Editing"),
            "{err:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_watermark_composites_a_real_overlay_at_the_documented_margin_via_the_real_render_plan()
    {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-watermark-real-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 2.0, 440);
        let logo_source = synth_image(&ffmpeg, &dir, "logo.png", "64x64");
        let logo_asset = register_asset(
            &assets_dir,
            AssetKind::Watermark,
            "Logo",
            logo_source.clone(),
        );

        let mut template = base_custom_template("custom_watermark_real");
        template.watermark = Some(WatermarkReference {
            asset_id: logo_asset.id,
            position: WatermarkPosition::TopRight,
        });

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let video_track_render_index = built
            .project
            .tracks
            .iter()
            .find(|t| Some(&t.id) == built.video_track_id.as_ref())
            .unwrap()
            .render_index;

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_watermark(&io, &mut built, &template).expect("apply_watermark");

        let overlay_track = built
            .project
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Overlay)
            .expect("a real Overlay track was added");
        assert!(
            overlay_track.render_index > video_track_render_index,
            "watermark must composite above the main video track"
        );
        let overlay_media = built
            .project
            .media
            .iter()
            .find(|m| m.kind == MediaKind::Image)
            .expect("a real Image MediaItem was registered");
        assert_eq!(
            overlay_media.source_path,
            logo_source.to_string_lossy().to_string()
        );

        // Feed the real built project through the REAL render plan builder
        // (pure, no ffmpeg subprocess) and check the actual overlay pixel
        // position it would composite at — this exercises `watermark_transform`
        // against the SAME formula `render::plan::build_video_clip_filter`
        // (private to that module) really uses, not a self-consistent
        // reimplementation.
        let graph = render::build_render_graph(&built.project).expect("graph builds");
        let settings = render::find_preset("fast_preview").unwrap().settings;
        let out_path = dir.join("wm_plan_test.mp4");
        let plan =
            render::build_ffmpeg_plan(&graph, &settings, &out_path, &[]).expect("plan builds");
        let args_str: String = plan
            .args
            .as_slice()
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");

        // 1920x1080 default canvas, 64x64 logo, TopRight, 5% margin:
        // overlay_x = 1920 - 64 - 96 = 1760; overlay_y = 54.
        assert!(
            args_str.contains("overlay=1760:54"),
            "expected the watermark's own overlay at (1760,54): {args_str}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_template_with_a_real_image_watermark_still_renders_a_real_completed_output() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-watermark-e2e-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_speech_like_source(&ffmpeg, &dir, "main.mp4", 2.0);
        let logo_source = synth_image(&ffmpeg, &dir, "logo.png", "64x64");
        let logo_asset = register_asset(&assets_dir, AssetKind::Watermark, "Logo", logo_source);

        let mut template = base_custom_template("custom_watermark_e2e");
        template.watermark = Some(WatermarkReference {
            asset_id: logo_asset.id,
            position: WatermarkPosition::BottomLeft,
        });
        template_io::save_custom_template(&templates_dir, &template).expect("save custom template");

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let mut config = minimal_config();
        config.template_id = Some(template.id.clone());
        config.export_preset_id = None;

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let output = run_pipeline(&io, &main_source, &config, cancel, pause, on_progress)
            .expect("real end-to-end pipeline with a watermark should complete");
        assert!(output.exists());
        let probed_output = probe::probe(&ffprobe, &output).expect("probing output");
        assert!(probed_output.has_video);

        std::fs::remove_dir_all(&dir).ok();
    }

    // -- apply_background_music ------------------------------------------------

    #[test]
    fn apply_background_music_is_a_no_op_when_the_template_has_no_background_music_set() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-bgmusic-noop-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 1.0, 440);
        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let track_count_before = built.project.tracks.len();

        let template = base_custom_template("custom_no_bgmusic");
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_background_music(&io, &mut built, &template).expect("no-op apply_background_music");

        assert_eq!(built.project.tracks.len(), track_count_before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_background_music_adds_a_real_music_track_with_the_templates_volume_and_no_ducking_by_default(
    ) {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-bgmusic-real-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 3.0, 440);
        let music_source = synth_audio_with_duration(&ffmpeg, &dir, "music.wav", 2.0);
        let music_asset =
            register_asset(&assets_dir, AssetKind::Music, "Music", music_source.clone());

        let mut template = base_custom_template("custom_bgmusic_real");
        template.background_music = Some(BackgroundMusicReference {
            asset_id: music_asset.id,
            volume: 0.3,
        });

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let probed_music = probe::probe(&ffprobe, &music_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let main_audio_track_id = built
            .audio_track_id
            .clone()
            .expect("main has an audio track");

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_background_music(&io, &mut built, &template).expect("apply_background_music");

        let music_track = built
            .project
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio && t.id != main_audio_track_id)
            .expect("a new music track was added");
        assert_eq!(
            built.project.audio_track_roles.get(&music_track.id),
            Some(&AudioRole::Music)
        );
        assert!(
            !built.project.track_ducking.contains_key(&music_track.id),
            "no sports_overlay on this template -> no ducking configured"
        );
        assert!(
            !built
                .project
                .audio_track_roles
                .contains_key(&main_audio_track_id),
            "the main content's own audio track stays Standard (no Voice role) without sports_overlay"
        );

        let clip = built
            .project
            .clips
            .iter()
            .find(|c| c.track_id == music_track.id)
            .expect("the music track has a real clip");
        assert_eq!(clip.position_us, 0);
        assert_eq!(
            clip.source_out_us,
            probed_music.duration_us.min(probed_main.duration_us)
        );
        let audio_settings = built
            .project
            .audio_clip_settings
            .get(&clip.id)
            .expect("real AudioClipSettings were recorded for the music clip");
        assert_eq!(audio_settings.volume, 0.3);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_background_music_wires_up_real_ducking_when_the_template_has_sports_overlay() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-batch-bgmusic-ducking-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 3.0, 440);
        let music_source = synth_audio_with_duration(&ffmpeg, &dir, "music.wav", 5.0);
        let music_asset = register_asset(&assets_dir, AssetKind::Music, "Music", music_source);

        let ducking = DuckingSettings {
            duck_level: 0.25,
            attack_us: 100_000,
            release_us: 200_000,
        };
        let mut template = base_custom_template("custom_bgmusic_ducking");
        template.background_music = Some(BackgroundMusicReference {
            asset_id: music_asset.id,
            volume: 0.4,
        });
        template.sports_overlay = Some(SportsOverlaySettings {
            score_overlay_suggested: false,
            music_role: AudioRole::Music,
            music_ducking: ducking,
        });

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let main_audio_track_id = built
            .audio_track_id
            .clone()
            .expect("main has an audio track");

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_background_music(&io, &mut built, &template).expect("apply_background_music");

        let music_track = built
            .project
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio && t.id != main_audio_track_id)
            .expect("a new music track was added");
        assert_eq!(
            built.project.track_ducking.get(&music_track.id),
            Some(&ducking)
        );
        assert_eq!(
            built.project.audio_track_roles.get(&main_audio_track_id),
            Some(&AudioRole::Voice),
            "sports_overlay wires the main content's own audio track up as the ducking trigger"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_background_music_clamps_a_longer_music_asset_to_the_projects_own_duration() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-bgmusic-clamp-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        // Main content is much shorter than the music asset.
        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 1.0, 440);
        let music_source = synth_audio_with_duration(&ffmpeg, &dir, "music.wav", 4.0);
        let music_asset = register_asset(&assets_dir, AssetKind::Music, "Music", music_source);

        let mut template = base_custom_template("custom_bgmusic_clamp");
        template.background_music = Some(BackgroundMusicReference {
            asset_id: music_asset.id,
            volume: 1.0,
        });

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let main_audio_track_id = built.audio_track_id.clone().unwrap();

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_background_music(&io, &mut built, &template).expect("apply_background_music");

        let music_track = built
            .project
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Audio && t.id != main_audio_track_id)
            .unwrap();
        let clip = built
            .project
            .clips
            .iter()
            .find(|c| c.track_id == music_track.id)
            .unwrap();
        assert_eq!(
            clip.source_out_us, probed_main.duration_us,
            "the music clip must be clamped to the (shorter) project duration, not the asset's own"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_template_with_background_music_produces_a_real_ffmpeg_plan_that_actually_mixes_it_in() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-bgmusic-plan-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_video_with_duration(&ffmpeg, &dir, "main.mp4", 3.0, 440);
        let music_source = synth_audio_with_duration(&ffmpeg, &dir, "music.wav", 3.0);
        let music_asset = register_asset(&assets_dir, AssetKind::Music, "Music", music_source);

        let mut template = base_custom_template("custom_bgmusic_plan");
        template.background_music = Some(BackgroundMusicReference {
            asset_id: music_asset.id,
            volume: 0.35,
        });

        let probed_main = probe::probe(&ffprobe, &main_source).unwrap();
        let mut built = build_whole_media_project(&main_source, &probed_main, None).unwrap();
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        apply_background_music(&io, &mut built, &template).expect("apply_background_music");

        // Same real "does the plan actually contain the filter" convention
        // `render::plan`'s own noise_reduction/normalize/ducking tests use —
        // pure, no ffmpeg subprocess, but built from this pass's own real
        // `apply_background_music` output (real probed durations, real
        // registered asset).
        let graph = render::build_render_graph(&built.project).expect("graph builds");
        let settings = render::find_preset("fast_preview").unwrap().settings;
        let out_path = dir.join("bgmusic_plan_test.mp4");
        let plan =
            render::build_ffmpeg_plan(&graph, &settings, &out_path, &[]).expect("plan builds");
        let args_str: String = plan
            .args
            .as_slice()
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(
            args_str.contains("amix=inputs=2"),
            "expected the main narration track and the new music track to both feed a real amix: {args_str}"
        );
        assert!(
            args_str.contains("volume=0.3500"),
            "expected the template's own linear-gain volume to reach the real ffmpeg args: {args_str}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_template_with_background_music_produces_a_real_completed_rendered_output_with_audio() {
        // Isolating "music alone produced this audio" via a silent/no-audio
        // main track isn't possible here: any selected template always
        // forces real VAD-driven silence removal (`BatchPipelineConfig::
        // template_id`'s own doc comment), which needs a real audio track to
        // score — a video-only source would fail PCM extraction, and a truly
        // silent one would have its whole timeline correctly cut as
        // `EmptyTimeline` (see `silence_removal_that_finds_no_speech_correctly_fails_the_job`
        // above). This test instead proves the OTHER real signal: the whole
        // pipeline actually completes end-to-end with a real Overlay-free,
        // music-carrying project and produces a real playable file with a
        // real audio stream — the companion pure-plan test above
        // (`a_template_with_background_music_produces_a_real_ffmpeg_plan_that_actually_mixes_it_in`)
        // is what proves the music itself was actually mixed in, via this
        // codebase's own established "check the real ffmpeg filter string"
        // convention.
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir = std::env::temp_dir().join(format!("ave-batch-bgmusic-e2e-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let assets_dir = dir.join("assets");
        let models_dir = dir.join("models");
        let templates_dir = dir.join("templates");

        let main_source = synth_speech_like_source(&ffmpeg, &dir, "main.mp4", 3.0);
        let music_source = synth_audio_with_duration(&ffmpeg, &dir, "music.wav", 3.0);
        let music_asset = register_asset(&assets_dir, AssetKind::Music, "Music", music_source);

        let mut template = base_custom_template("custom_bgmusic_e2e");
        template.background_music = Some(BackgroundMusicReference {
            asset_id: music_asset.id,
            volume: 0.4,
        });
        template_io::save_custom_template(&templates_dir, &template).expect("save custom template");

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir, &assets_dir);
        let mut config = minimal_config();
        config.template_id = Some(template.id.clone());
        config.export_preset_id = None;

        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let on_progress: Arc<dyn Fn(BatchJobStatus, String, f32) + Send + Sync> =
            Arc::new(|_, _, _| {});

        let output = run_pipeline(&io, &main_source, &config, cancel, pause, on_progress)
            .expect("real end-to-end pipeline with background music should complete");
        assert!(output.exists());
        let probed_output = probe::probe(&ffprobe, &output).expect("probing output");
        assert!(probed_output.has_audio);

        std::fs::remove_dir_all(&dir).ok();
    }
}
