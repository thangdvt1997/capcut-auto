//! Long-Video-to-Shorts pipeline Tauri command surface (master prompt §22,
//! `IMPLEMENTATION_PLAN.md` Phase 11's final backend bullet). Thin per
//! master prompt §66 — the real orchestration/ranking/composition logic
//! lives in `crate::shorts`; this module resolves binaries/probes the
//! source media once and delegates to `crate::shorts` plus the existing
//! `commands::highlights::run_detection`/`reframe::motion`/`zoom`/
//! `captions::generate` subsystems, exactly the same "one-line resolve +
//! delegate to a plain function" split `commands::highlights`/
//! `commands::reframe` already use.
//!
//! ## Transcription-dependency design decision (required by this pass's
//! brief, stated honestly)
//!
//! `commands::transcription::transcribe_media` is an async, job-id-
//! returning, event-emitting background job (spawns a
//! `tauri::async_runtime::spawn_blocking` thread, streams
//! `transcription:progress` events; the real result only arrives on that
//! event's final `done: true` payload) — not a synchronous function this
//! command could safely call and block on. Two honest options existed:
//! (a) require the caller to have already produced a transcript (via the
//! existing granular `transcribe_media` job) and pass it in directly, same
//! as `commands::highlights::detect_highlights` already requires; or (b)
//! reuse `transcription::WhisperProvider::transcribe`'s real synchronous
//! core (no progress callback, no job/event plumbing) directly inside this
//! command, blocking the IPC call for the transcription's own real
//! duration.
//!
//! This pass picks **(a)**: `generate_shorts` takes `transcript:
//! Vec<TranscriptEntry>` as a direct parameter, exactly like
//! `detect_highlights` already does, and returns a specific, clear
//! `ShortsError::TranscriptRequired` (never a silent skip or a fabricated
//! empty-caption fallback) when it's empty. Reasons: (1) it matches this
//! codebase's own existing precedent at the very next pipeline stage this
//! command composes with (`detect_highlights` already made exactly this
//! choice, for the same reason); (2) captions and highlight-detection's own
//! optional semantic signal both need the *whole* transcript up front
//! anyway, so there's no partial-pipeline benefit to transcribing
//! internally; (3) a model download/load + full-media transcription can
//! genuinely take minutes for a long source video — silently blocking a
//! Tauri command for that long, with no progress event at all, would be a
//! worse user experience than the frontend already has today (a job id +
//! progress events it can subscribe to and show). Option (b) was concretely
//! available — `WhisperProvider::transcribe` is real and callable — but was
//! rejected purely for the blocking-duration reason above, not because it
//! was architecturally impossible.
//!
//! ## Export-timing design decision
//!
//! This command stops at producing `Vec<ShortCandidate>` — real, valid,
//! immediately-loadable `ProjectV1`s — and never calls
//! `commands::render::start_render_job` itself. Master prompt §22's own
//! "Each generated short should remain editable" is the deciding reason:
//! auto-rendering every candidate the moment it's generated would produce N
//! video files before a user has had any chance to trim/re-caption/adjust
//! zoom on any of them, working against exactly the editability this
//! feature is required to preserve. Actually rendering a chosen short is
//! left as a separate, later, explicit user action through the existing,
//! unchanged `start_render_job` command — the same non-destructive-by-
//! default philosophy `docs/architecture.md` already documents for every
//! other edit in this app.

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::commands::ai::AiProviderSettings;
use crate::commands::highlights as highlight_detection;
use crate::commands::media::resolve_ffmpeg;
use crate::error::AppErrorPayload;
use crate::ffmpeg::binaries;
use crate::highlights::types::Highlight;
use crate::media::error::MediaError;
use crate::media::probe;
use crate::project::{ProjectV1, TranscriptEntry};
use crate::reframe::motion::MotionTrackingSubjectTracker;
use crate::reframe::provider::SubjectTracker;
use crate::shorts::{self, ShortSourceContext, ShortsError, ShortsSettings};

/// A generated short paired with the highlight metadata that produced it
/// (master prompt §21's own "Highlight #1, Score 92" UI mockup — a future
/// frontend can show that alongside this candidate's generated project).
#[derive(Debug, Clone, Serialize, Type)]
pub struct ShortCandidate {
    pub highlight: Highlight,
    pub project: ProjectV1,
}

/// How many raw highlight candidates to request from `run_detection` before
/// ranking/non-overlap-selecting down to `clip_count` — generous enough
/// that overlap-driven rejections (`shorts::ranking` module doc comment)
/// still leave enough real candidates to fill every requested slot.
const CANDIDATE_POOL_MULTIPLIER: usize = 4;
const MIN_CANDIDATE_POOL_SIZE: usize = 10;

/// Small, deliberate duplication of `commands::media::resolve_ffprobe`'s
/// body (same shape `commands::reframe::resolve_binaries`'s own doc comment
/// already justifies for the same reason: keeping this a one-line resolve
/// with no cross-module private-function coupling).
fn resolve_ffprobe(app: &AppHandle) -> Result<PathBuf, MediaError> {
    let resource_dir = app.path().resource_dir().ok();
    binaries::ffprobe_path(resource_dir.as_deref()).map_err(|e| MediaError::BinaryNotFound {
        tool: "ffprobe".into(),
        details: e.to_string(),
    })
}

/// The real pipeline, parameterized over already-resolved `ffmpeg`/
/// `ffprobe` paths (module doc comment's "resolve once, delegate to a plain
/// function" split) so tests can exercise it against a real synthesized
/// media file without standing up a Tauri `AppHandle`.
#[allow(clippy::too_many_arguments)]
fn run_generate_shorts(
    ffmpeg: &Path,
    ffprobe: &Path,
    media_path: &Path,
    transcript: &[TranscriptEntry],
    settings: ShortsSettings,
    apply_zoom: bool,
    ai_settings: Option<AiProviderSettings>,
) -> Result<Vec<ShortCandidate>, AppErrorPayload> {
    if transcript.is_empty() {
        return Err(AppErrorPayload::from(&ShortsError::TranscriptRequired));
    }
    if settings.clip_count == 0 {
        return Err(AppErrorPayload::from(&ShortsError::InvalidClipCount {
            clip_count: settings.clip_count,
        }));
    }

    let probed = probe::probe(ffprobe, media_path).map_err(|e| AppErrorPayload::from(&e))?;
    if probed.width == 0 || probed.height == 0 || probed.duration_us <= 0 {
        return Err(AppErrorPayload::from(&ShortsError::InvalidSourceMedia {
            path: media_path.display().to_string(),
        }));
    }

    // Highlight Detection: reuse the exact same real pipeline
    // `detect_highlights` runs (module doc comment) — never a second copy.
    let pool_size = (settings.clip_count as usize)
        .saturating_mul(CANDIDATE_POOL_MULTIPLIER)
        .max(MIN_CANDIDATE_POOL_SIZE);
    let detection = highlight_detection::run_detection(
        ffmpeg,
        media_path,
        transcript,
        probed.duration_us,
        ai_settings,
        pool_size,
    )?;

    // Candidate Ranking: real non-overlapping top-K selection, then
    // duration adjustment per selected span (`shorts::ranking`).
    let target_duration_us = settings.duration.target_duration_us();
    let selected =
        shorts::select_top_non_overlapping(&detection.highlights, settings.clip_count as usize);
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let spans: Vec<(i64, i64)> = selected
        .iter()
        .map(|h| {
            shorts::adjust_span_to_duration(
                h.start_us,
                h.end_us,
                target_duration_us,
                probed.duration_us,
            )
        })
        .collect();

    // Reframe's real subject-tracking, run once over the whole media and
    // shared across every candidate — this pipeline composes existing
    // subsystems, it doesn't re-run them per candidate needlessly.
    let tracker = MotionTrackingSubjectTracker;
    let raw_subject_positions = tracker
        .track(ffmpeg, ffprobe, media_path)
        .map_err(|e| AppErrorPayload::from(&e))?;

    // Optional Zoom's real RMS-energy signal needs PCM samples — extracted
    // once, only when actually needed.
    let pcm_samples = if apply_zoom {
        crate::audio::pcm::extract_pcm(ffmpeg, media_path).map_err(|e| AppErrorPayload::from(&e))?
    } else {
        Vec::new()
    };

    let media_path_str = media_path.to_string_lossy().to_string();
    let mut candidates = Vec::with_capacity(selected.len());
    for (highlight, span) in selected.into_iter().zip(spans) {
        let ctx = ShortSourceContext {
            source_media_path: &media_path_str,
            source_width: probed.width,
            source_height: probed.height,
            transcript,
            raw_subject_positions: &raw_subject_positions,
            pcm_samples: &pcm_samples,
            pcm_sample_rate: crate::audio::pcm::PCM_SAMPLE_RATE,
            apply_zoom,
        };
        let project = shorts::build_short_project(&highlight, span, &settings, &ctx);
        candidates.push(ShortCandidate { highlight, project });
    }

    Ok(candidates)
}

/// Runs the Long-Video-to-Shorts pipeline end to end for one real media file
/// (master prompt §22): highlight detection, non-overlapping candidate
/// ranking, and per-candidate clip extraction/reframe/captions/optional-zoom
/// composition into a real, editable `ProjectV1` each — see this module's
/// doc comment for the transcription-dependency and export-timing design
/// decisions.
#[tauri::command]
#[specta::specta]
pub fn generate_shorts(
    app: AppHandle,
    media_path: String,
    transcript: Vec<TranscriptEntry>,
    settings: ShortsSettings,
    apply_zoom: bool,
    ai_settings: Option<AiProviderSettings>,
) -> Result<Vec<ShortCandidate>, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let ffprobe = resolve_ffprobe(&app).map_err(|e| AppErrorPayload::from(&e))?;
    run_generate_shorts(
        &ffmpeg,
        &ffprobe,
        Path::new(&media_path),
        &transcript,
        settings,
        apply_zoom,
        ai_settings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::command::{run_checked, FfmpegArgs};
    use crate::project::Word;
    use crate::shorts::{DurationSetting, ShortsAspect};

    fn settings(seconds: u32, clip_count: u32) -> ShortsSettings {
        ShortsSettings {
            duration: DurationSetting::Custom { seconds },
            aspect: ShortsAspect::Vertical9x16,
            clip_count,
        }
    }

    fn transcript() -> Vec<TranscriptEntry> {
        vec![TranscriptEntry {
            id: "e1".to_string(),
            media_id: "m1".to_string(),
            text: "hello there".to_string(),
            start_us: 200_000,
            end_us: 900_000,
            confidence: 0.9,
            words: vec![
                Word {
                    text: "hello".to_string(),
                    start_us: 200_000,
                    end_us: 500_000,
                    confidence: 0.9,
                },
                Word {
                    text: "there".to_string(),
                    start_us: 500_000,
                    end_us: 900_000,
                    confidence: 0.9,
                },
            ],
            is_filler: false,
        }]
    }

    #[test]
    fn an_empty_transcript_is_rejected_before_any_ffmpeg_work() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let err = run_generate_shorts(
            &ffmpeg,
            &ffprobe,
            Path::new("does-not-matter.mp4"),
            &[],
            settings(15, 1),
            false,
            None,
        )
        .unwrap_err();
        assert_eq!(err.code, "SHORTS_TRANSCRIPT_REQUIRED");
    }

    #[test]
    fn a_zero_clip_count_is_rejected() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let err = run_generate_shorts(
            &ffmpeg,
            &ffprobe,
            Path::new("does-not-matter.mp4"),
            &transcript(),
            settings(15, 0),
            false,
            None,
        )
        .unwrap_err();
        assert_eq!(err.code, "SHORTS_INVALID_CLIP_COUNT");
    }

    /// A short (~2s) real synthetic clip — a red/blue hard cut plus a real
    /// tone on the audio track — matching `commands::highlights`' own test
    /// fixture exactly, so the real local-signal (no-AI) scene-change-driven
    /// highlight path this test relies on is proven to find candidates the
    /// same way that module's own tests already do.
    fn synth_media(ffmpeg: &Path, dir: &Path) -> std::path::PathBuf {
        let source = dir.join("in.mp4");
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=320x240:duration=1:rate=10",
                "-f",
                "lavfi",
                "-i",
                "color=blue:size=320x240:duration=1:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1:a=0[v]",
                "-map",
                "[v]",
                "-map",
                "2:a",
                "-shortest",
            ])
            .path(&source);
        run_checked(ffmpeg, &args).expect("synthesizing a test clip with audio + a hard cut");
        source
    }

    #[test]
    fn end_to_end_pipeline_produces_a_real_editable_project_for_a_synthesized_clip() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-shorts-e2e-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let candidates = run_generate_shorts(
            &ffmpeg,
            &ffprobe,
            &source,
            &transcript(),
            settings(1, 1),
            true,
            None,
        )
        .expect("the full shorts pipeline succeeds against a real synthesized clip");

        assert_eq!(candidates.len(), 1, "requested exactly one clip");
        let candidate = &candidates[0];

        // Canvas is the requested 9:16 aspect, not the source's own 4:3.
        assert_eq!(candidate.project.canvas.width, 1080);
        assert_eq!(candidate.project.canvas.height, 1920);

        // Exactly one real clip, trimmed to (approximately) the requested
        // 1s duration and placed at the start of the new timeline.
        assert_eq!(candidate.project.clips.len(), 1);
        let clip = &candidate.project.clips[0];
        assert_eq!(clip.position_us, 0);
        assert!(clip.source_out_us > clip.source_in_us);
        assert!(clip.source_in_us >= 0);
        assert!(clip.source_out_us <= 2_000_000);

        // The highlight metadata is real and paired with the project.
        assert!(candidate.highlight.score >= 0.0 && candidate.highlight.score <= 100.0);

        std::fs::remove_dir_all(&dir).ok();
    }
}
