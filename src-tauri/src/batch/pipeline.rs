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

use crate::audio::pcm;
use crate::captions::generate as captions_generate;
use crate::media::probe::{self, ProbedMedia};
use crate::project::{
    CanvasV1, Clip, ClipSettings, MediaItem, MediaKind, ProjectV1, Track, TrackKind,
    TranscriptEntry,
};
use crate::render;
use crate::templates::{self, io as template_io, Template};
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
    // No voice-ducking segments for batch scope (module doc comment in
    // `batch::types::BatchPipelineConfig::template_id` — batch-built
    // projects never assign an `AudioRole::Voice` track, so this would
    // always resolve to an empty `Vec` anyway; passing `&[]` directly avoids
    // re-deriving `commands::render::compute_voice_speech_segments`'s own
    // `AppHandle`-shaped resolution for a case that can never fire here).
    let plan = render::build_ffmpeg_plan(&graph, &settings, &output_path, &[])
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
    use crate::project::{CanvasRatioPreset, Rational, Word};
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
    ) -> PipelineIo<'a> {
        PipelineIo {
            ffmpeg,
            ffprobe,
            models_dir,
            templates_dir,
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

        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir);
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
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir);
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
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir);
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
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir);
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
        let io = no_op_io(&ffmpeg, &ffprobe, &models_dir, &templates_dir);

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
}
