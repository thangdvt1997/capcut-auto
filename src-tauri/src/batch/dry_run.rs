//! Preview / Dry Run (upgrade spec §18, `UPGRADE_PLAN.md` Phase U3): runs the
//! exact same resolution/decision logic `batch::pipeline::run_pipeline` runs
//! for one real media file — real probing, real template/export-preset
//! resolution, and (only when the resolved config would actually enable it)
//! real, cheap VAD analysis — and reports back a structured [`DryRunResult`]
//! describing what a real batch job would do, **without ever calling
//! `render::build_render_graph`/actually encoding anything**, and without
//! ever running a real (slow) Whisper transcription.
//!
//! Deliberately a **separate, read-only function** from `run_pipeline`
//! (`UPGRADE_PLAN.md` Phase U3's own task brief) rather than a `dry_run: bool`
//! flag threaded through the real pipeline — a shared boolean-gated code path
//! risks a dry-run branch silently skipping a real safety check the real path
//! needs (or vice versa). Instead, [`run_dry_run`] calls into the exact same
//! shared sub-pieces `run_pipeline` itself uses —
//! [`super::pipeline::resolve_template`], [`super::pipeline::default_output_path`],
//! the real `vad`/`media::probe` cores — so "what a dry run predicts" and
//! "what a real job actually does" can never silently drift apart from
//! duplicated logic.
//!
//! ## Which analysis steps this module actually runs for real vs. only
//! describes
//!
//! - **Probing** (`media::probe::probe`): always run for real — the same
//!   near-instant ffprobe call `run_pipeline`'s own Analyzing stage makes.
//!   Every other field in [`DryRunResult`] is derived from it.
//! - **Silence removal's VAD scoring/cutlist** (`vad::SileroVadProvider::
//!   score_chunks` + `vad::segments_from_scores` +
//!   `vad::build_cuts_from_speech_segments`): run for real, but **only**
//!   when both (a) the resolved config would actually enable silence
//!   removal (an explicit `remove_silence`, or a resolved template's own
//!   `silence_settings` — the exact same fallback `run_pipeline`'s own
//!   Editing stage uses) and (b) the source actually has an audio track to
//!   score. This is real, already-fast local-model inference — no network
//!   call, no GPU requirement, the exact same cost `run_pipeline`'s own
//!   Editing stage always pays for a real batch job (well under a second
//!   against a short clip in this project's own test suite) — and it is the
//!   one analysis step that lets [`DryRunExpectedOutput::predicted_duration_us`]
//!   be a real computed number instead of a guess. When there is no audio
//!   track, VAD is skipped entirely (no PCM even extracted) and
//!   [`SilenceRemovalPlan::predicted_removed_us`] stays `None` — an honest
//!   "not computed", never a fabricated `0`.
//! - **Scene-change detection** (`media::scene::detect_scene_changes`): real
//!   `run_pipeline` never calls this at all for its own editing decisions
//!   (`BatchPipelineConfig::template_id`'s own doc comment: zoom/transition/
//!   sports-overlay template settings are not applied by this pipeline, so
//!   there is no real batch-job decision here that scene detection would
//!   inform) — so this dry run does not call it directly either. It is
//!   still exercised indirectly whenever the caller opts into the AI Auto
//!   Template path below, since that already-real feature's own signal
//!   gathering includes it.
//! - **Transcription** (real Whisper inference): never actually run — a
//!   multi-second-to-minutes model inference is not something a *preview*
//!   should pay for. [`CaptionsPlan`] instead reports whether captioning
//!   (and, with it, the whole Transcribing stage) would run, and which real,
//!   already-resolved `transcription_model_id` it would use — a real
//!   resolved config value, never executed. This dry run does not even
//!   check whether that model is actually installed (unlike `run_pipeline`,
//!   which errors on an uninstalled model) — describing "would transcribe
//!   using model X" doesn't require the model to be present on disk yet.
//! - **AI Auto Template** (`ai::auto_template`, upgrade spec §7/Phase U2):
//!   optional, and only attempted when the caller's `config.template_id` is
//!   `None` (no template already chosen) **and** real `ai_settings` were
//!   provided — the same "AI is optional, never required" discipline every
//!   other AI-consuming feature in this codebase follows (`ai::edit_plan`/
//!   `ai::smart_edit`/`ai::auto_template` module doc comments). When both
//!   hold, [`run_dry_run_with_ai`] reuses `commands::auto_template::
//!   run_suggestion` verbatim — the real, already-shipped Phase U2 feature,
//!   not a second, parallel "what would the AI decide" implementation. If
//!   the caller already chose a template, or provided no AI settings,
//!   [`DryRunResult::ai_decision`] is honestly `None`, never fabricated —
//!   and, per that feature's own "propose, don't apply" discipline, a
//!   populated `ai_decision` never overwrites [`DryRunResult::resolved_template`]
//!   itself; it is shown alongside it as a proposal only.
//!
//! ## Honest scope note: no "CapCut Execution Plan" field
//!
//! Upgrade spec §18's own field list includes a "CapCut Execution Plan" —
//! this codebase's confirmed architectural decision (`UPGRADE_PLAN.md`'s
//! "Explicitly out of scope" section) is direct draft-file export, not CapCut
//! GUI/RPA automation, and `batch::pipeline::run_pipeline` itself never
//! builds a CapCut export at all (it renders straight through
//! `render::build_render_graph`/ffmpeg) — there is no real "CapCut execution"
//! step for this dry run to describe. [`DryRunExpectedOutput`]'s own
//! `container`/`video_codec`/resolution fields are this codebase's honest
//! equivalent: the real resolved *render* plan a batch job would execute.

use std::path::Path;

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;

use crate::ai::auto_template::AiTemplateRecommendation;
use crate::audio::pcm;
use crate::commands::ai::AiProviderSettings;
use crate::error::AppErrorPayload;
use crate::media::probe;
use crate::render::{
    self,
    presets::{Container, VideoCodec},
};
use crate::templates::{self, io as template_io, Template};
use crate::vad::{self, CutParams, VadParams, VadProvider};

use super::error::BatchError;
use super::manager;
use super::pipeline::{self, PipelineIo};
use super::types::BatchPipelineConfig;

/// Real, probed facts about the source media (upgrade spec §18's "Input").
#[derive(Debug, Clone, Serialize, Type)]
pub struct DryRunInput {
    pub path: String,
    pub duration_us: i64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub has_video: bool,
}

/// Which real config value a resolved [`CutParams`] came from — mirrors the
/// exact precedence `run_pipeline`'s own `effective_cut_params` uses.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SilenceSettingsSource {
    /// `BatchPipelineConfig::remove_silence` was set explicitly.
    Explicit,
    /// No explicit override; the resolved template's own `silence_settings`
    /// applied instead.
    Template,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct SilenceRemovalPlan {
    pub enabled: bool,
    pub params: Option<CutParams>,
    pub source: Option<SilenceSettingsSource>,
    /// Real, VAD-computed total microseconds that would be cut — `Some`
    /// only when `enabled` and the source has a real audio track to score
    /// (module doc comment). Never a fabricated estimate.
    pub predicted_removed_us: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct CaptionsPlan {
    pub enabled: bool,
    pub reason: String,
    /// Only meaningful when `enabled` — the real, already-resolved
    /// transcription model id a real batch job would use. Transcription
    /// itself never actually runs during a dry run (module doc comment).
    pub transcription_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct DryRunEditingPlan {
    pub silence_removal: SilenceRemovalPlan,
    pub captions: CaptionsPlan,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct DryRunExpectedOutput {
    /// The exact real path `run_pipeline` would render to
    /// (`pipeline::default_output_path`'s own naming logic) — no render
    /// ever happens, so this path never actually gets written by a dry run.
    pub output_path: String,
    /// Real prediction: probed duration minus the real VAD-computed
    /// silence-removal total, only when that total was actually computed
    /// (`SilenceRemovalPlan::predicted_removed_us`). `None` when silence
    /// removal is disabled, or wasn't computed (module doc comment) — never
    /// a fabricated number.
    pub predicted_duration_us: Option<i64>,
    pub width: u32,
    pub height: u32,
    pub container: Container,
    pub video_codec: VideoCodec,
}

/// Upgrade spec §18's exact field list (minus "CapCut Execution Plan" — see
/// module doc comment) — everything grounded in a real resolution/computation
/// this codebase already has, never a fabricated number.
#[derive(Debug, Clone, Serialize, Type)]
pub struct DryRunResult {
    pub input: DryRunInput,
    pub resolved_template: Option<Template>,
    pub ai_decision: Option<AiTemplateRecommendation>,
    pub editing_plan: DryRunEditingPlan,
    pub expected_output: DryRunExpectedOutput,
}

/// The real, Tauri-free, AI-free dry-run core (module doc comment covers
/// exactly which analysis steps are real vs. estimated). `ai_decision` is
/// computed by the caller ([`run_dry_run_with_ai`], which has the
/// `AiProviderSettings`/catalog this function deliberately does not need)
/// and simply passed through unchanged — keeping this function directly
/// testable, for every non-AI scenario, without a mock AI server.
pub(crate) fn run_dry_run(
    io: &PipelineIo,
    media_path: &Path,
    config: &BatchPipelineConfig,
    ai_decision: Option<AiTemplateRecommendation>,
) -> Result<DryRunResult, BatchError> {
    if !media_path.exists() {
        return Err(BatchError::MediaNotFound {
            path: media_path.display().to_string(),
        });
    }
    let probed =
        probe::probe(io.ffprobe, media_path).map_err(|e| pipeline::stage_failed("Analyzing", e))?;
    if !probed.has_video && !probed.has_audio {
        return Err(BatchError::UnsupportedMedia {
            path: media_path.display().to_string(),
        });
    }

    let template = match &config.template_id {
        Some(id) => Some(pipeline::resolve_template(io.templates_dir, id)?),
        None => None,
    };

    // -- Captions / transcription: same up-front validation `run_pipeline`
    //    itself does, so a dry run surfaces this real misconfiguration
    //    rather than silently describing a plan that would fail immediately.
    let needs_transcript = config.captions.is_some();
    if needs_transcript && config.transcription_model_id.is_none() {
        return Err(BatchError::TranscriptionModelRequired);
    }
    let captions_plan = CaptionsPlan {
        enabled: needs_transcript,
        reason: if needs_transcript {
            "captions settings were provided in the batch config, so this job would transcribe \
             then generate captions"
                .to_string()
        } else {
            "no caption settings were provided; captioning (and transcription) would be skipped"
                .to_string()
        },
        transcription_model_id: if needs_transcript {
            config.transcription_model_id.clone()
        } else {
            None
        },
    };

    // -- Silence removal: same explicit-overrides-template fallback
    //    `run_pipeline` itself uses.
    let template_cut_params = template.as_ref().map(|t| t.silence_settings);
    let effective_cut_params = config.remove_silence.or(template_cut_params);
    let silence_source = if config.remove_silence.is_some() {
        Some(SilenceSettingsSource::Explicit)
    } else {
        template_cut_params.map(|_| SilenceSettingsSource::Template)
    };

    let mut predicted_removed_us = None;
    if let Some(cut_params) = effective_cut_params {
        if probed.has_audio {
            let samples = pcm::extract_pcm(io.ffmpeg, media_path)
                .map_err(|e| pipeline::stage_failed("Editing", e))?;
            let chunks = vad::SileroVadProvider
                .score_chunks(&samples, pcm::PCM_SAMPLE_RATE, None)
                .map_err(|e| pipeline::stage_failed("Editing", e))?;
            let segments = vad::segments_from_scores(&chunks, VadParams::default(), 0);
            let cuts = vad::build_cuts_from_speech_segments(
                &segments,
                "dry-run-preview",
                probed.duration_us,
                cut_params,
            );
            let removed: i64 = cuts.iter().map(|c| (c.end_us - c.start_us).max(0)).sum();
            predicted_removed_us = Some(removed);
        }
    }
    let silence_plan = SilenceRemovalPlan {
        enabled: effective_cut_params.is_some(),
        params: effective_cut_params,
        source: silence_source,
        predicted_removed_us,
    };

    // -- Rendering: real resolved export preset, never a real encode.
    let export_preset_id = config
        .export_preset_id
        .clone()
        .or_else(|| template.as_ref().map(|t| t.export_preset_id.clone()))
        .ok_or(BatchError::ExportPresetRequired)?;
    let preset = render::find_preset(&export_preset_id)
        .map_err(|e| pipeline::stage_failed("Rendering", e))?;

    let output_suffix = config.output_suffix.as_deref().unwrap_or("edited");
    let output_path = pipeline::default_output_path(media_path, &preset.settings, output_suffix)?;

    let predicted_duration_us =
        predicted_removed_us.map(|removed| (probed.duration_us - removed).max(0));

    Ok(DryRunResult {
        input: DryRunInput {
            path: media_path.display().to_string(),
            duration_us: probed.duration_us,
            width: probed.width,
            height: probed.height,
            has_audio: probed.has_audio,
            has_video: probed.has_video,
        },
        resolved_template: template,
        ai_decision,
        editing_plan: DryRunEditingPlan {
            silence_removal: silence_plan,
            captions: captions_plan,
        },
        expected_output: DryRunExpectedOutput {
            output_path: output_path.display().to_string(),
            predicted_duration_us,
            width: preset.settings.width,
            height: preset.settings.height,
            container: preset.settings.container,
            video_codec: preset.settings.video_codec,
        },
    })
}

/// Tauri-free (no `AppHandle`), but AI-aware: decides whether AI Auto
/// Template should even be attempted (module doc comment — only when
/// `config.template_id` is `None` and `ai_settings` is `Some`), and if so,
/// reuses `commands::auto_template::run_suggestion` verbatim before
/// delegating to [`run_dry_run`] for everything else. Split out from
/// [`run_dry_run_for_media`] so tests can exercise the full AI-aware path
/// against a real synthesized clip + the existing `ai::test_http` mock
/// server, without needing a running Tauri `AppHandle`.
pub(crate) fn run_dry_run_with_ai(
    io: &PipelineIo,
    media_path: &Path,
    config: &BatchPipelineConfig,
    ai_settings: Option<AiProviderSettings>,
    catalog: &[Template],
) -> Result<DryRunResult, AppErrorPayload> {
    let ai_decision = match (&config.template_id, ai_settings) {
        (None, Some(settings)) => Some(crate::commands::auto_template::run_suggestion(
            io.ffmpeg,
            io.ffprobe,
            media_path,
            &[],
            settings,
            catalog,
        )?),
        _ => None,
    };
    run_dry_run(io, media_path, config, ai_decision).map_err(|e| AppErrorPayload::from(&e))
}

/// `commands::batch::dry_run_batch_job`'s real logic: resolves the real
/// ffmpeg/ffprobe/templates paths and the real template catalog (built-in +
/// custom) a real batch job would use, then delegates to
/// [`run_dry_run_with_ai`].
pub fn run_dry_run_for_media(
    app: &AppHandle,
    media_path: String,
    config: BatchPipelineConfig,
    ai_settings: Option<AiProviderSettings>,
) -> Result<DryRunResult, AppErrorPayload> {
    let paths = manager::resolve_pipeline_paths(app).map_err(|e| AppErrorPayload::from(&e))?;
    let io = paths.as_io();

    let dir =
        crate::commands::templates::templates_dir(app).map_err(|e| AppErrorPayload::from(&e))?;
    let custom = template_io::list_custom_templates(&dir).map_err(|e| AppErrorPayload::from(&e))?;
    let catalog: Vec<Template> = templates::all_templates()
        .into_iter()
        .chain(custom)
        .collect();

    run_dry_run_with_ai(&io, Path::new(&media_path), &config, ai_settings, &catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;

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

    fn minimal_config(export_preset_id: &str) -> BatchPipelineConfig {
        BatchPipelineConfig {
            remove_silence: None,
            captions: None,
            transcription_model_id: None,
            transcription_language: None,
            template_id: None,
            export_preset_id: Some(export_preset_id.to_string()),
            output_suffix: None,
        }
    }

    /// Basic video+audio source (plain sine tone) — reused across tests that
    /// don't care about real VAD-detected speech.
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

    /// A tremolo-modulated 220Hz tone — the same real-Silero-VAD-detectable-
    /// as-speech fixture `batch::manager`'s own `synth_named_source` uses
    /// (see that function's doc comment for why a plain sine tone won't do).
    fn synth_speech_like_source(ffmpeg: &Path, dir: &Path) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
        let source = dir.join("speech.mp4");
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
                "sine=frequency=220:duration=3,tremolo=f=4:d=0.9",
                "-shortest",
            ])
            .path(&source);
        run_checked(ffmpeg, &args).expect("synthesizing a speech-like test source");
        source
    }

    fn synth_silent_source(ffmpeg: &Path, dir: &Path) -> PathBuf {
        use crate::ffmpeg::command::{run_checked, FfmpegArgs};
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
        run_checked(ffmpeg, &args).expect("synthesizing a real silent test source");
        source
    }

    struct TestEnv {
        ffmpeg: PathBuf,
        ffprobe: PathBuf,
        dir: PathBuf,
        models_dir: PathBuf,
        templates_dir: PathBuf,
    }

    impl TestEnv {
        fn new(label: &str) -> Self {
            let ffmpeg =
                crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
            let ffprobe = crate::ffmpeg::binaries::ffprobe_path(None)
                .expect("ffprobe resolvable in test env");
            let dir = std::env::temp_dir().join(format!("ave-dryrun-{label}-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let models_dir = dir.join("models");
            let templates_dir = dir.join("templates");
            Self {
                ffmpeg,
                ffprobe,
                dir,
                models_dir,
                templates_dir,
            }
        }

        fn io(&self) -> PipelineIo<'_> {
            no_op_io(
                &self.ffmpeg,
                &self.ffprobe,
                &self.models_dir,
                &self.templates_dir,
            )
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).ok();
        }
    }

    // -- No template configured ------------------------------------------

    #[test]
    fn dry_run_with_no_template_reports_real_input_and_skips_silence_removal() {
        let env = TestEnv::new("no-template");
        let source = synth_source(&env.ffmpeg, &env.dir);
        let config = minimal_config("fast_preview");

        let result = run_dry_run(&env.io(), &source, &config, None).expect("dry run succeeds");

        assert!(result.input.has_video);
        assert!(result.input.has_audio);
        assert_eq!(result.input.width, 320);
        assert_eq!(result.input.height, 240);
        assert!(
            (result.input.duration_us - 3_000_000).abs() < 500_000,
            "{}",
            result.input.duration_us
        );
        assert!(result.resolved_template.is_none());
        assert!(result.ai_decision.is_none());

        assert!(!result.editing_plan.silence_removal.enabled);
        assert!(result.editing_plan.silence_removal.params.is_none());
        assert!(result
            .editing_plan
            .silence_removal
            .predicted_removed_us
            .is_none());
        assert!(!result.editing_plan.captions.enabled);
        assert!(result
            .editing_plan
            .captions
            .transcription_model_id
            .is_none());

        let expected_preset = render::find_preset("fast_preview").unwrap();
        assert_eq!(result.expected_output.width, expected_preset.settings.width);
        assert_eq!(
            result.expected_output.height,
            expected_preset.settings.height
        );
        assert_eq!(
            result.expected_output.container,
            expected_preset.settings.container
        );
        assert_eq!(
            result.expected_output.video_codec,
            expected_preset.settings.video_codec
        );
        assert!(result.expected_output.predicted_duration_us.is_none());

        let expected_path =
            pipeline::default_output_path(&source, &expected_preset.settings, "edited").unwrap();
        assert_eq!(
            result.expected_output.output_path,
            expected_path.display().to_string()
        );

        // No render ever happens: the predicted output file must not exist.
        assert!(
            !expected_path.exists(),
            "a dry run must never actually render an output file"
        );
    }

    // -- A real built-in template ------------------------------------------

    #[test]
    fn dry_run_with_a_real_template_resolves_it_and_computes_a_real_predicted_duration() {
        let env = TestEnv::new("with-template");
        let source = synth_speech_like_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.template_id = Some("tmpl_tiktok".to_string());
        config.export_preset_id = None; // falls back to the template's own preset

        let result = run_dry_run(&env.io(), &source, &config, None).expect("dry run succeeds");

        let template = result
            .resolved_template
            .as_ref()
            .expect("tmpl_tiktok should resolve");
        assert_eq!(template.id, "tmpl_tiktok");

        assert!(result.editing_plan.silence_removal.enabled);
        assert_eq!(
            result.editing_plan.silence_removal.source,
            Some(SilenceSettingsSource::Template)
        );
        assert_eq!(
            result.editing_plan.silence_removal.params,
            Some(template.silence_settings)
        );
        let removed = result
            .editing_plan
            .silence_removal
            .predicted_removed_us
            .expect("a real audio track should get a real VAD-computed removal estimate");
        assert!(removed >= 0);
        assert!(
            removed < result.input.duration_us,
            "a speech-like source should not have its entire duration predicted as removed"
        );

        let predicted_duration = result
            .expected_output
            .predicted_duration_us
            .expect("silence removal being enabled should yield a real predicted duration");
        assert_eq!(predicted_duration, result.input.duration_us - removed);

        // Falls back to the template's own export preset (tiktok_1080x1920).
        let preset = render::find_preset("tiktok_1080x1920").unwrap();
        assert_eq!(result.expected_output.width, preset.settings.width);
        assert_eq!(result.expected_output.height, preset.settings.height);

        let expected_path =
            pipeline::default_output_path(&source, &preset.settings, "edited").unwrap();
        assert!(!expected_path.exists());
    }

    // -- An explicit remove_silence override wins over the template's own --

    #[test]
    fn an_explicit_remove_silence_override_wins_over_the_templates_own_settings() {
        let env = TestEnv::new("explicit-override");
        let source = synth_speech_like_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.template_id = Some("tmpl_tiktok".to_string());
        let explicit = CutParams {
            padding_before_us: 10_000,
            padding_after_us: 10_000,
            merge_gap_us: 0,
        };
        config.remove_silence = Some(explicit);

        let result = run_dry_run(&env.io(), &source, &config, None).expect("dry run succeeds");
        assert_eq!(
            result.editing_plan.silence_removal.source,
            Some(SilenceSettingsSource::Explicit)
        );
        assert_eq!(result.editing_plan.silence_removal.params, Some(explicit));
    }

    // -- Real silence: predicted duration correctly approaches zero --------

    #[test]
    fn a_source_with_no_detected_speech_predicts_nearly_the_whole_duration_removed() {
        // Mirrors `batch::pipeline`'s own
        // `silence_removal_that_finds_no_speech_correctly_fails_the_job` test
        // fixture — but a dry run must NOT fail the way a real render would
        // (`RenderError::EmptyTimeline`): it just predicts a (near-)fully-
        // empty output and returns `Ok`, since no real render is ever
        // attempted.
        let env = TestEnv::new("no-speech");
        let source = synth_silent_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.remove_silence = Some(CutParams::default());

        let result = run_dry_run(&env.io(), &source, &config, None)
            .expect("a dry run must succeed even when the predicted output would be empty");

        let removed = result
            .editing_plan
            .silence_removal
            .predicted_removed_us
            .expect("a real audio track is present");
        assert!(
            removed >= result.input.duration_us - 100_000,
            "expected almost the whole duration to be predicted as removed, got {removed} of {}",
            result.input.duration_us
        );
        let predicted_duration = result.expected_output.predicted_duration_us.unwrap();
        assert!(predicted_duration <= 100_000, "{predicted_duration}");
    }

    // -- Captions configured: reports the plan, never actually transcribes --

    #[test]
    fn captions_configured_reports_would_transcribe_without_an_installed_model() {
        let env = TestEnv::new("captions");
        let source = synth_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.captions = Some(crate::captions::generate::CaptionGenerationSettings {
            max_words_per_line: 4,
            max_chars_per_line: 24,
            grouping: crate::captions::generate::CaptionGroupingMode::Word,
        });
        // A model id that is definitely not installed under `env.models_dir`
        // (freshly created, empty). A real `run_pipeline` would fail with
        // `TranscriptionModelNotInstalled` here — a dry run must not, since
        // it never actually checks installation status or transcribes.
        config.transcription_model_id = Some("tiny".to_string());

        let result = run_dry_run(&env.io(), &source, &config, None)
            .expect("a dry run must succeed without an installed transcription model");

        assert!(result.editing_plan.captions.enabled);
        assert_eq!(
            result
                .editing_plan
                .captions
                .transcription_model_id
                .as_deref(),
            Some("tiny")
        );
    }

    #[test]
    fn captions_without_a_transcription_model_id_errors_up_front_like_the_real_pipeline() {
        let env = TestEnv::new("captions-no-model");
        let source = synth_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.captions = Some(crate::captions::generate::CaptionGenerationSettings {
            max_words_per_line: 4,
            max_chars_per_line: 24,
            grouping: crate::captions::generate::CaptionGroupingMode::Word,
        });
        config.transcription_model_id = None;

        let err = run_dry_run(&env.io(), &source, &config, None).unwrap_err();
        assert!(matches!(err, BatchError::TranscriptionModelRequired));
    }

    // -- Error passthrough ---------------------------------------------------

    #[test]
    fn a_missing_media_file_errors_with_a_real_media_not_found() {
        let env = TestEnv::new("missing");
        let missing = env.dir.join("does-not-exist.mp4");
        let config = minimal_config("fast_preview");

        let err = run_dry_run(&env.io(), &missing, &config, None).unwrap_err();
        assert!(matches!(err, BatchError::MediaNotFound { .. }));
    }

    #[test]
    fn an_unknown_template_id_errors_with_a_real_unknown_template() {
        let env = TestEnv::new("unknown-template");
        let source = synth_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.template_id = Some("does_not_exist".to_string());

        let err = run_dry_run(&env.io(), &source, &config, None).unwrap_err();
        assert!(matches!(err, BatchError::UnknownTemplate { .. }));
    }

    // -- No render ever happens, even for a fully-configured job -------------

    #[test]
    fn no_render_subprocess_ever_runs_during_a_dry_run() {
        let env = TestEnv::new("no-render");
        let source = synth_speech_like_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.template_id = Some("tmpl_tiktok".to_string());
        config.export_preset_id = None;

        let result = run_dry_run(&env.io(), &source, &config, None).expect("dry run succeeds");
        let output_path = PathBuf::from(&result.expected_output.output_path);
        assert!(
            !output_path.exists(),
            "a dry run must never produce a real rendered output file"
        );
        // The parent `batch_output` directory may exist (the same real
        // `default_output_path` helper a real job uses creates it), but it
        // must be empty — proving nothing was ever actually written into it.
        if let Some(parent) = output_path.parent() {
            if parent.is_dir() {
                let entries: Vec<_> = std::fs::read_dir(parent).unwrap().collect();
                assert!(
                    entries.is_empty(),
                    "expected no files in {parent:?}, found {entries:?}"
                );
            }
        }
    }

    // -- AI Auto Template wiring (run_dry_run_with_ai) -----------------------

    fn ai_settings(base_url: String) -> AiProviderSettings {
        use crate::commands::ai::AiProviderKind;
        AiProviderSettings {
            provider: AiProviderKind::OpenAi,
            base_url,
            model: "test-model".to_string(),
            temperature: 0.2,
            timeout_ms: 5_000,
            credential_ref: None,
        }
    }

    fn chat_completion_body(content: &str) -> String {
        serde_json::json!({
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }]
        })
        .to_string()
    }

    #[test]
    fn ai_decision_is_populated_when_no_template_is_chosen_and_ai_settings_are_given() {
        let env = TestEnv::new("ai-decision");
        let source = synth_speech_like_source(&env.ffmpeg, &env.dir);
        let config = minimal_config("fast_preview"); // template_id: None

        let recommendation_json = r#"{"version": 1, "template_id": "tmpl_tiktok", "reason": "fast-paced vertical-friendly content", "confidence": 0.8, "suggested_aspect": null}"#;
        let (base_url, rx) = crate::ai::test_http::spawn_one_shot(
            "HTTP/1.1 200 OK",
            chat_completion_body(recommendation_json),
        );

        let catalog = templates::all_templates();
        let result = run_dry_run_with_ai(
            &env.io(),
            &source,
            &config,
            Some(ai_settings(base_url)),
            &catalog,
        )
        .expect("dry run with AI succeeds against a real synthetic clip + mock AI server");

        let decision = result.ai_decision.expect("expected an AI recommendation");
        assert_eq!(decision.template_id, "tmpl_tiktok");
        assert_eq!(decision.template_name, "TikTok");
        // `resolved_template` stays honestly `None` — the caller never chose
        // one, and a recommendation is a proposal, never auto-applied.
        assert!(result.resolved_template.is_none());

        let captured = rx.recv().expect("mock server captured a real request");
        assert_eq!(captured.method, "POST");
    }

    #[test]
    fn ai_decision_is_skipped_when_a_template_was_already_chosen() {
        let env = TestEnv::new("ai-skip-template-chosen");
        let source = synth_speech_like_source(&env.ffmpeg, &env.dir);
        let mut config = minimal_config("fast_preview");
        config.template_id = Some("tmpl_tiktok".to_string());

        // A base_url that would fail immediately if ever actually contacted
        // — this proves no AI call is attempted at all when a template was
        // already chosen, not merely that it would fail gracefully.
        let ai = ai_settings("http://127.0.0.1:1".to_string());

        let catalog = templates::all_templates();
        let result = run_dry_run_with_ai(&env.io(), &source, &config, Some(ai), &catalog)
            .expect("dry run succeeds; the unreachable AI endpoint is never actually contacted");
        assert!(result.ai_decision.is_none());
    }

    #[test]
    fn ai_decision_is_skipped_when_no_ai_settings_are_given() {
        let env = TestEnv::new("ai-skip-no-settings");
        let source = synth_speech_like_source(&env.ffmpeg, &env.dir);
        let config = minimal_config("fast_preview");

        let catalog = templates::all_templates();
        let result = run_dry_run_with_ai(&env.io(), &source, &config, None, &catalog)
            .expect("dry run succeeds with no AI settings at all");
        assert!(result.ai_decision.is_none());
    }
}
