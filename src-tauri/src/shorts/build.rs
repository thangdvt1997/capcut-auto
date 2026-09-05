//! Composes one candidate span (already ranked + duration-adjusted, see
//! `shorts::ranking`) into a real, single-clip, immediately-editable
//! `ProjectV1` — master prompt §22's "Clip Extraction -> Reframe -> Captions
//! -> Optional Zoom" pipeline stages, all landing on the same clip.
//!
//! ## "Remain editable" (master prompt §22's explicit requirement)
//!
//! The output of this module is a real `project::ProjectV1` — the exact same
//! schema `project::io::save_atomic`/`load`/the timeline editor already work
//! with — not a rendered video file. It can be opened, trimmed, re-captioned,
//! or re-rendered by this app's own existing tools, consistent with the
//! whole codebase's non-destructive-editing philosophy (`docs/architecture.md`).
//!
//! ## Reframe: crop-via-scale, not a literal `crop` filter
//!
//! `reframe::crop`'s own module doc comment states the real, current gap
//! this composition has to work around: `render::plan`/`render::graph` have
//! no time-varying per-clip filter-parameter mechanism yet, so a literal
//! `crop=` filter isn't something a per-time-position value can drive today.
//! What `render::plan` *does* already support is a single static per-clip
//! `ClipSettings::scale_x/y`/`transform_x/y` (module doc comment "Coordinate
//! convention"). This module reproduces the *visual effect* of a target-
//! aspect crop using exactly that existing mechanism:
//!
//! 1. **Static baseline** (`cover_scale`): scale the full source frame up
//!    uniformly so it fully covers the new (typically narrower/taller)
//!    canvas — the standard "background-size: cover" formula,
//!    `scale = max(canvas_width/source_width, canvas_height/source_height)`.
//!    Centered (`transform_x/y = 0.0`), this is equivalent to
//!    `reframe::crop::compute_crop_window`'s own centered crop window,
//!    expressed as a scale-and-center instead of a pixel crop rectangle —
//!    the same real math, just inverted into the mechanism `render::plan`
//!    can already evaluate once per clip.
//! 2. **Time-varying tracking on top**: the real
//!    `reframe::motion::MotionTrackingSubjectTracker` + `reframe::smoothing`
//!    pipeline (already fully built, Phase 11's `SubjectTracker` bullet)
//!    still produces its own `position_x`/`position_y` `Keyframe` entries,
//!    attached to the clip via [`reframe_keyframes_for_span`]. These are
//!    real, correctly-timed, directly reusable by a future render-pipeline
//!    pass or by the CapCut export adapter (`capcut::keyframe` already
//!    consumes `Keyframe` for real) — the same honestly-documented
//!    "structural today, wired into FFmpeg render once that mechanism
//!    exists" status `zoom`'s own `"scale"`-property keyframes already have
//!    (`timeline/zoom.rs` module doc comment, `IMPLEMENTATION_PLAN.md`
//!    Phase 11's Auto-Zoom bullet).
//!
//! ## Keyframe coexistence
//!
//! Reframe keyframes use `property: "position_x"`/`"position_y"`; zoom
//! keyframes (`zoom::generate_zoom_keyframes`) use `property: "scale"`.
//! `project::types::Keyframe::property` is a plain `String` precisely so
//! multiple independent animated properties can coexist on one clip's
//! keyframe list (`Keyframe` doc comment) — this module simply appends both
//! `Vec<Keyframe>`s onto the same clip's collection; neither producer knows
//! or cares about the other, and nothing here overwrites or filters by
//! property, so both survive untouched.

use uuid::Uuid;

use crate::captions::generate::{CaptionGenerationSettings, CaptionGroupingMode};
use crate::highlights::types::Highlight;
use crate::project::{
    Caption, Clip, ClipSettings, Keyframe, MediaItem, MediaKind, ProjectV1, Track, TrackKind,
    TranscriptEntry,
};
use crate::reframe::provider::SubjectPosition;
use crate::reframe::smoothing::{
    keyframes_from_smoothed, smooth_positions, DEFAULT_SMOOTHING_TAU_US,
};
use crate::zoom::{self, EmphasisWindow, ZoomIntensity};

use super::captions::slice_transcript_for_span;
use super::settings::{ShortsSettings, SHORT_CANVAS_FPS};

/// Zoom-emphasis analysis window length (module-internal — chunking a
/// short's own audio into fixed windows before scoring each one's real RMS
/// energy, `highlights::signals::windowed_rms_energy`, matching the
/// resolution `highlights::combine`'s own local-signal scoring already
/// operates at for candidate windows this size).
const ZOOM_EMPHASIS_WINDOW_US: i64 = 1_000_000;

/// Fixed zoom intensity this pipeline applies when `apply_zoom` is set
/// (`commands::shorts::generate_shorts` doc comment: no separate
/// per-short intensity setting exists in this pass's required signature).
/// `Medium` is `zoom::ZoomIntensity`'s own documented "normal amount"
/// (`zoom` module doc comment) — a reasonable default until a future pass
/// exposes it as its own setting.
pub const SHORT_ZOOM_INTENSITY: ZoomIntensity = ZoomIntensity::Medium;

/// Caption generation settings this pipeline uses for every short: short,
/// TikTok-style continuous word-by-word captions (`CaptionGroupingMode::Word`)
/// rather than whole-sentence captions — the common look for this exact
/// content type (master prompt §22's own target platforms), and a reasonable
/// default given this pass's required signature has no separate
/// caption-style setting.
fn short_caption_settings() -> CaptionGenerationSettings {
    CaptionGenerationSettings {
        max_words_per_line: 4,
        max_chars_per_line: 24,
        grouping: CaptionGroupingMode::Word,
    }
}

/// "Background-size: cover" scale factor: the smallest uniform scale that
/// makes a `source_width`x`source_height` frame fully cover a
/// `canvas_width`x`canvas_height` canvas once centered (module doc comment).
pub fn cover_scale(
    canvas_width: u32,
    canvas_height: u32,
    source_width: u32,
    source_height: u32,
) -> f64 {
    if source_width == 0 || source_height == 0 {
        return 1.0;
    }
    let scale_for_width = canvas_width as f64 / source_width as f64;
    let scale_for_height = canvas_height as f64 / source_height as f64;
    scale_for_width.max(scale_for_height)
}

/// Turns whole-media `SubjectPosition` samples (source-file-relative,
/// `reframe::provider::SubjectPosition` doc comment) into `position_x`/
/// `position_y` `Keyframe`s for one candidate's own span: samples are
/// filtered to `[span_start_us, span_end_us)`, re-based to be relative to
/// `span_start_us` (so time `0` is the new clip's own start, matching its
/// `position_us: 0` placement), smoothed, then converted via
/// `reframe::smoothing::keyframes_from_smoothed` — the exact same
/// half-canvas-unit conversion `auto_reframe_media` already uses, reused
/// unchanged rather than re-derived.
pub fn reframe_keyframes_for_span(
    raw_positions: &[SubjectPosition],
    span_start_us: i64,
    span_end_us: i64,
    clip_id: &str,
) -> Vec<Keyframe> {
    let span_positions: Vec<SubjectPosition> = raw_positions
        .iter()
        .filter(|p| p.time_us >= span_start_us && p.time_us < span_end_us)
        .map(|p| SubjectPosition {
            time_us: p.time_us - span_start_us,
            target_x: p.target_x,
            target_y: p.target_y,
        })
        .collect();
    if span_positions.is_empty() {
        return Vec::new();
    }
    let smoothed = smooth_positions(&span_positions, DEFAULT_SMOOTHING_TAU_US);
    keyframes_from_smoothed(&smoothed, clip_id, 0)
}

/// Builds real "emphasis" candidate windows for one span from already-
/// extracted whole-media PCM samples (`audio::pcm::extract_pcm`), scores
/// each with the real `highlights::signals::windowed_rms_energy` signal
/// (reused unchanged), and runs them through the existing
/// `zoom::emphasis_triggers` + `zoom::generate_zoom_keyframes` pipeline —
/// the same real, already-built zoom-generation machinery Phase 11's
/// Auto-Zoom bullet shipped, applied here to one short's own span rather
/// than a whole timeline clip.
pub fn zoom_keyframes_for_span(
    pcm_samples: &[i16],
    pcm_sample_rate: u32,
    span_start_us: i64,
    span_end_us: i64,
    clip_id: &str,
    intensity: ZoomIntensity,
) -> Vec<Keyframe> {
    if span_end_us <= span_start_us {
        return Vec::new();
    }
    let mut windows = Vec::new();
    let mut t = span_start_us;
    while t < span_end_us {
        let window_end = (t + ZOOM_EMPHASIS_WINDOW_US).min(span_end_us);
        let energy = crate::highlights::signals::windowed_rms_energy(
            pcm_samples,
            pcm_sample_rate,
            t,
            window_end,
        );
        windows.push(EmphasisWindow {
            start_us: t - span_start_us,
            end_us: window_end - span_start_us,
            energy,
        });
        t = window_end;
    }
    let triggers = zoom::emphasis_triggers(&windows);
    zoom::generate_zoom_keyframes(&triggers, intensity, clip_id)
}

/// Everything [`build_short_project`] needs about the *source* media and
/// pre-computed whole-media analysis results, gathered once by the pipeline
/// (`commands::shorts::run_generate_shorts`) and shared across every
/// candidate rather than re-computed per candidate.
pub struct ShortSourceContext<'a> {
    pub source_media_path: &'a str,
    pub source_width: u32,
    pub source_height: u32,
    pub transcript: &'a [TranscriptEntry],
    /// Whole-media subject-tracking samples (source-file-relative time),
    /// already computed once via `reframe::motion::MotionTrackingSubjectTracker`.
    pub raw_subject_positions: &'a [SubjectPosition],
    /// Whole-media PCM samples (`audio::pcm::extract_pcm`), only actually
    /// used when `apply_zoom` is true — empty otherwise.
    pub pcm_samples: &'a [i16],
    pub pcm_sample_rate: u32,
    pub apply_zoom: bool,
}

/// Builds one real, single-clip, immediately-editable `ProjectV1` for
/// `highlight`'s already-ranked-and-duration-adjusted `span`
/// (`[span.0, span.1)`, source-media-relative microseconds) — master prompt
/// §22's "Clip Extraction -> Reframe -> Captions -> Optional Zoom" stages,
/// all composed onto one clip (module doc comment).
pub fn build_short_project(
    highlight: &Highlight,
    span: (i64, i64),
    settings: &ShortsSettings,
    ctx: &ShortSourceContext,
) -> ProjectV1 {
    let (span_start_us, span_end_us) = span;
    let (canvas_width, canvas_height) = settings.aspect.canvas_dimensions();

    let mut project = ProjectV1::new(format!("Short - {}", highlight.title));
    project.canvas.width = canvas_width;
    project.canvas.height = canvas_height;
    project.canvas.fps = SHORT_CANVAS_FPS;
    project.canvas.ratio_preset = settings.aspect.ratio_preset();

    let media_id = Uuid::new_v4().to_string();
    project.media.push(MediaItem {
        id: media_id.clone(),
        kind: MediaKind::Video,
        source_path: ctx.source_media_path.to_string(),
        duration_us: span_end_us - span_start_us,
        width: ctx.source_width,
        height: ctx.source_height,
        fps: crate::project::Rational::default(),
        codec: String::new(),
        bitrate: 0,
        audio_channels: 0,
        sample_rate: 0,
        rotation_deg: 0,
        created_at: None,
        proxy_path: None,
        thumbnail_path: None,
    });

    let video_track_id = Uuid::new_v4().to_string();
    let caption_track_id = Uuid::new_v4().to_string();
    let clip_id = Uuid::new_v4().to_string();

    let scale = cover_scale(
        canvas_width,
        canvas_height,
        ctx.source_width,
        ctx.source_height,
    );
    let clip = Clip {
        id: clip_id.clone(),
        track_id: video_track_id.clone(),
        media_id: Some(media_id),
        source_in_us: span_start_us,
        source_out_us: span_end_us,
        position_us: 0,
        speed: 1.0,
        enabled: true,
        group_id: None,
        clip_settings: ClipSettings {
            scale_x: scale,
            scale_y: scale,
            ..ClipSettings::default()
        },
    };

    project.tracks.push(Track {
        id: video_track_id,
        kind: TrackKind::Video,
        name: "Video".to_string(),
        render_index: 0,
        locked: false,
        hidden: false,
        muted: false,
        solo: false,
        clip_ids: vec![clip_id.clone()],
    });
    project.tracks.push(Track {
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
    project.clips.push(clip);

    // Reframe: real subject-tracking keyframes for this span alone.
    project.keyframes.extend(reframe_keyframes_for_span(
        ctx.raw_subject_positions,
        span_start_us,
        span_end_us,
        &clip_id,
    ));

    // Optional Zoom (master prompt §22's own pipeline stage name).
    if ctx.apply_zoom {
        project.keyframes.extend(zoom_keyframes_for_span(
            ctx.pcm_samples,
            ctx.pcm_sample_rate,
            span_start_us,
            span_end_us,
            &clip_id,
            SHORT_ZOOM_INTENSITY,
        ));
    }

    // Captions: transcript slice re-timed to this project's own timeline.
    let sliced = slice_transcript_for_span(ctx.transcript, span_start_us, span_end_us);
    let mut captions: Vec<Caption> = crate::captions::generate::generate_captions_from_transcript(
        &sliced,
        &short_caption_settings(),
    );
    for caption in &mut captions {
        caption.track_id = caption_track_id.clone();
    }
    project.captions = captions;

    project
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{TrackKind, Word};
    use crate::shorts::settings::ShortsAspect;

    fn highlight() -> Highlight {
        Highlight {
            id: "h1".to_string(),
            start_us: 10_000_000,
            end_us: 14_000_000,
            score: 90.0,
            title: "Great moment".to_string(),
            reason: "test".to_string(),
        }
    }

    fn settings() -> ShortsSettings {
        ShortsSettings {
            duration: super::super::settings::DurationSetting::Fixed15,
            aspect: ShortsAspect::Vertical9x16,
            clip_count: 1,
        }
    }

    fn transcript() -> Vec<TranscriptEntry> {
        vec![TranscriptEntry {
            id: "e1".to_string(),
            media_id: "m1".to_string(),
            text: "hello world".to_string(),
            start_us: 11_000_000,
            end_us: 12_000_000,
            confidence: 0.9,
            words: vec![
                Word {
                    text: "hello".to_string(),
                    start_us: 11_000_000,
                    end_us: 11_500_000,
                    confidence: 0.9,
                },
                Word {
                    text: "world".to_string(),
                    start_us: 11_500_000,
                    end_us: 12_000_000,
                    confidence: 0.9,
                },
            ],
            is_filler: false,
        }]
    }

    #[test]
    fn cover_scale_picks_the_larger_of_the_two_axis_ratios() {
        // 1920x1080 source -> 1080x1920 canvas: width ratio 1080/1920=0.5625,
        // height ratio 1920/1080=1.777..; cover must use the larger.
        let scale = cover_scale(1080, 1920, 1920, 1080);
        assert!((scale - (1920.0 / 1080.0)).abs() < 1e-9);
    }

    #[test]
    fn cover_scale_is_defensive_against_zero_source_dimensions() {
        assert_eq!(cover_scale(1080, 1920, 0, 1080), 1.0);
    }

    #[test]
    fn build_short_project_sets_the_correct_canvas_for_the_chosen_aspect() {
        let ctx = ShortSourceContext {
            source_media_path: "in.mp4",
            source_width: 1920,
            source_height: 1080,
            transcript: &[],
            raw_subject_positions: &[],
            pcm_samples: &[],
            pcm_sample_rate: 16_000,
            apply_zoom: false,
        };
        let project =
            build_short_project(&highlight(), (10_000_000, 14_000_000), &settings(), &ctx);
        assert_eq!(project.canvas.width, 1080);
        assert_eq!(project.canvas.height, 1920);
        assert_eq!(
            project.canvas.ratio_preset,
            crate::project::CanvasRatioPreset::Ratio9x16
        );
    }

    #[test]
    fn build_short_project_sets_correct_clip_source_range_and_position() {
        let ctx = ShortSourceContext {
            source_media_path: "in.mp4",
            source_width: 1920,
            source_height: 1080,
            transcript: &[],
            raw_subject_positions: &[],
            pcm_samples: &[],
            pcm_sample_rate: 16_000,
            apply_zoom: false,
        };
        let project =
            build_short_project(&highlight(), (10_000_000, 14_000_000), &settings(), &ctx);
        assert_eq!(project.clips.len(), 1);
        let clip = &project.clips[0];
        assert_eq!(clip.source_in_us, 10_000_000);
        assert_eq!(clip.source_out_us, 14_000_000);
        assert_eq!(clip.position_us, 0);
        assert!(clip.clip_settings.scale_x > 1.0);
        assert_eq!(clip.clip_settings.scale_x, clip.clip_settings.scale_y);

        let video_track = project
            .tracks
            .iter()
            .find(|t| t.kind == TrackKind::Video)
            .unwrap();
        assert_eq!(video_track.clip_ids, vec![clip.id.clone()]);
    }

    #[test]
    fn build_short_project_retimes_captions_relative_to_the_new_timeline() {
        let ctx = ShortSourceContext {
            source_media_path: "in.mp4",
            source_width: 1920,
            source_height: 1080,
            transcript: &transcript(),
            raw_subject_positions: &[],
            pcm_samples: &[],
            pcm_sample_rate: 16_000,
            apply_zoom: false,
        };
        // Span [10s, 14s); transcript entry at absolute [11s, 12s) must land
        // at relative [1s, 2s) in the new project, NOT still at [11s, 12s).
        let project =
            build_short_project(&highlight(), (10_000_000, 14_000_000), &settings(), &ctx);
        assert!(!project.captions.is_empty());
        for caption in &project.captions {
            assert!(
                caption.start_us < 4_000_000 && caption.end_us <= 4_000_000,
                "caption not retimed relative to the short's own [0, 4s) timeline: {caption:?}"
            );
        }
        // The whole text landed somewhere inside [0s, 4s) - specifically
        // around [1s, 2s) given the source entry's own absolute timing.
        let earliest = project.captions.iter().map(|c| c.start_us).min().unwrap();
        assert_eq!(earliest, 1_000_000);
    }

    #[test]
    fn reframe_and_zoom_keyframes_coexist_without_clobbering_each_other() {
        let raw_positions = vec![
            SubjectPosition {
                time_us: 10_000_000,
                target_x: 0.3,
                target_y: 0.5,
            },
            SubjectPosition {
                time_us: 12_000_000,
                target_x: 0.7,
                target_y: 0.5,
            },
        ];
        // Loud PCM covering the *whole media* (samples are absolute-time-
        // indexed from 0, same as `audio::pcm::extract_pcm`'s own output) —
        // must extend past the span's own [10s, 14s) absolute range for
        // `windowed_rms_energy` to find anything there at all.
        let pcm_samples = vec![32_000i16; 16_000 * 14];
        let ctx = ShortSourceContext {
            source_media_path: "in.mp4",
            source_width: 1920,
            source_height: 1080,
            transcript: &[],
            raw_subject_positions: &raw_positions,
            pcm_samples: &pcm_samples,
            pcm_sample_rate: 16_000,
            apply_zoom: true,
        };
        let project =
            build_short_project(&highlight(), (10_000_000, 14_000_000), &settings(), &ctx);

        let position_keyframes: Vec<_> = project
            .keyframes
            .iter()
            .filter(|k| k.property == "position_x" || k.property == "position_y")
            .collect();
        let scale_keyframes: Vec<_> = project
            .keyframes
            .iter()
            .filter(|k| k.property == "scale")
            .collect();

        assert!(!position_keyframes.is_empty(), "expected reframe keyframes");
        assert!(!scale_keyframes.is_empty(), "expected zoom keyframes");
        // Every keyframe belongs to the one real clip, and the two kinds
        // don't overwrite or reduce each other's count.
        let clip_id = &project.clips[0].id;
        assert!(project.keyframes.iter().all(|k| &k.clip_id == clip_id));
        assert_eq!(
            project.keyframes.len(),
            position_keyframes.len() + scale_keyframes.len()
        );
    }

    #[test]
    fn zoom_keyframes_are_absent_when_apply_zoom_is_false() {
        let raw_positions = vec![SubjectPosition {
            time_us: 10_000_000,
            target_x: 0.5,
            target_y: 0.5,
        }];
        let pcm_samples = vec![32_000i16; 16_000 * 4];
        let ctx = ShortSourceContext {
            source_media_path: "in.mp4",
            source_width: 1920,
            source_height: 1080,
            transcript: &[],
            raw_subject_positions: &raw_positions,
            pcm_samples: &pcm_samples,
            pcm_sample_rate: 16_000,
            apply_zoom: false,
        };
        let project =
            build_short_project(&highlight(), (10_000_000, 14_000_000), &settings(), &ctx);
        assert!(project.keyframes.iter().all(|k| k.property != "scale"));
    }
}
