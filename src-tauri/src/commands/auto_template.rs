//! AI Auto Template Tauri command surface (upgrade spec §7, `UPGRADE_PLAN.md`
//! Phase U2). Thin per master prompt §66 — all real logic lives in
//! `crate::ai::auto_template`/`crate::media::probe`/`crate::media::scene`/
//! `crate::templates`.
//!
//! [`suggest_template_for_media`] orchestrates every real signal upgrade spec
//! §7 asks for — Content Type (left to the model's own judgment, grounded in
//! the transcript/highlights it's given — see `ai::auto_template` module doc
//! comment and this pass's own writeup in `UPGRADE_PLAN.md` for why no
//! separate content-type classifier exists), Video Duration/Aspect Ratio
//! (`media::probe::probe`), Speech (a caller-supplied transcript, same
//! "caller passes the transcript in directly" convention
//! `commands::highlights`/`commands::shorts` already use), Scenes
//! (`media::scene::detect_scene_changes`), and Important Segments
//! (`commands::highlights::run_detection`, reused directly rather than
//! re-deriving highlight scoring a second way) — then asks the configured
//! `AIProvider` to recommend one real catalog template. Never applies the
//! recommendation to anything: that is upgrade spec §7's own separate
//! "Accept / Change Template / Customize / Run" step, a later frontend pass
//! (Phase 11's existing template-apply flow already covers "apply this
//! template to a project").
//!
//! [`run_suggestion`] carries every real step parameterized over
//! already-resolved `ffmpeg`/`ffprobe` paths and a caller-supplied `catalog`
//! rather than an `AppHandle` — the same "the Tauri command is a one-line
//! resolve + delegate to a plain function" split `commands::highlights::
//! run_detection` already establishes, so this pass's tests can exercise the
//! full real pipeline (real ffmpeg/ffprobe subprocess calls, real VAD/scene
//! signals, a mock AI server) without needing a Tauri `AppHandle` at all.
//!
//! ## One real AI call per suggestion, not two
//!
//! `commands::highlights::run_detection` optionally takes its own
//! `ai_settings` to blend an AI-proposed semantic signal into its highlight
//! scores. [`run_suggestion`] deliberately calls it with `ai_settings: None`
//! (local-signal-only highlights: speech density, audio energy, scene
//! changes) rather than threading the caller's AI settings through to it —
//! doing so would mean *two* AI calls (and two AI cost/latency hits) for one
//! Auto Template suggestion, which upgrade spec §7 never asks for. The single
//! AI call this module does make already receives the real local-signal
//! highlights (title/score/time range) as part of its own prompt (see
//! `ai::auto_template::build_auto_template_prompt`), so the model still sees
//! "where the good parts are" — just from one AI round trip, not two.

use std::path::Path;

use tauri::AppHandle;

use crate::ai::auto_template::{self, AiTemplateRecommendation};
use crate::commands::ai::{self, AiProviderSettings};
use crate::commands::highlights::run_detection;
use crate::commands::media::{resolve_ffmpeg, resolve_ffprobe};
use crate::commands::templates::templates_dir;
use crate::error::AppErrorPayload;
use crate::media::probe;
use crate::media::scene;
use crate::project::TranscriptEntry;
use crate::templates::{self, io as template_io, Template};

/// How many real local-signal highlights get computed and folded into the
/// recommendation prompt — matches
/// `ai::auto_template::MAX_HIGHLIGHTS_IN_PROMPT`'s own budget (that module
/// truncates further if handed more, but there's no reason to compute more
/// than it will ever use).
const MAX_HIGHLIGHTS_FOR_SUGGESTION: usize = 5;

/// The real pipeline, parameterized over already-resolved `ffmpeg`/`ffprobe`
/// paths and a caller-loaded `catalog` (module doc comment) — the only
/// things [`suggest_template_for_media`] itself adds are resolving those
/// paths and loading the real on-disk catalog.
pub(crate) fn run_suggestion(
    ffmpeg: &Path,
    ffprobe: &Path,
    media_path: &Path,
    transcript: &[TranscriptEntry],
    ai_settings: AiProviderSettings,
    catalog: &[Template],
) -> Result<AiTemplateRecommendation, AppErrorPayload> {
    // Real Video Duration/Aspect Ratio signal (upgrade spec §7).
    let probed = probe::probe(ffprobe, media_path).map_err(|e| AppErrorPayload::from(&e))?;

    // Real Scenes signal (upgrade spec §7).
    let scene_cuts =
        scene::detect_scene_changes(ffmpeg, media_path, scene::DEFAULT_SCENE_THRESHOLD)
            .map_err(|e| AppErrorPayload::from(&e))?;

    // Real Important Segments signal (upgrade spec §7) — local-signal-only,
    // module doc comment ("One real AI call per suggestion, not two").
    let highlight_result = run_detection(
        ffmpeg,
        media_path,
        transcript,
        probed.duration_us,
        None,
        MAX_HIGHLIGHTS_FOR_SUGGESTION,
    )?;

    let api_key = ai::resolve_api_key(&ai_settings).map_err(|e| AppErrorPayload::from(&e))?;
    let provider =
        ai::build_provider(&ai_settings, api_key).map_err(|e| AppErrorPayload::from(&e))?;

    let request = auto_template::build_auto_template_request(
        &probed,
        transcript,
        &scene_cuts,
        &highlight_result.highlights,
        catalog,
        ai_settings.temperature,
        ai_settings.timeout_ms,
    );
    let response = provider
        .complete(&request)
        .map_err(|e| AppErrorPayload::from(&e))?;

    auto_template::parse_and_validate(&response.text, catalog)
        .map_err(|e| AppErrorPayload::from(&e))
}

/// AI Auto Template (upgrade spec §7): user selects one video, this command
/// analyzes its real signals and returns one recommended template from the
/// real catalog (built-in + custom), with a reason and confidence — never
/// applied to anything by this command itself (module doc comment).
#[tauri::command]
#[specta::specta]
pub fn suggest_template_for_media(
    app: AppHandle,
    media_path: String,
    transcript: Vec<TranscriptEntry>,
    ai_settings: AiProviderSettings,
) -> Result<AiTemplateRecommendation, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let ffprobe = resolve_ffprobe(&app).map_err(|e| AppErrorPayload::from(&e))?;

    let dir = templates_dir(&app).map_err(|e| AppErrorPayload::from(&e))?;
    let custom = template_io::list_custom_templates(&dir).map_err(|e| AppErrorPayload::from(&e))?;
    let catalog: Vec<Template> = templates::all_templates()
        .into_iter()
        .chain(custom)
        .collect();

    run_suggestion(
        &ffmpeg,
        &ffprobe,
        Path::new(&media_path),
        &transcript,
        ai_settings,
        &catalog,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::spawn_one_shot;
    use crate::commands::ai::AiProviderKind;
    use crate::ffmpeg::command::{run_checked, FfmpegArgs};
    use crate::templates::all_templates;

    fn ai_settings(base_url: String) -> AiProviderSettings {
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

    /// A short (~2s) real synthetic clip — a red/blue hard cut plus a real
    /// amplitude-modulated tone on the audio track — so the full pipeline
    /// (ffprobe, ffmpeg scene detection, PCM extraction + VAD scoring) runs
    /// against a real, tiny file rather than a mock. The tone shape mirrors
    /// `batch::manager`'s own `synth_named_source` fixture (`UPGRADE_PLAN.md`
    /// Phase U1 writeup — a pure sine tone has no real speech spectral
    /// characteristics and Silero VAD correctly finds zero segments in one),
    /// verified against this project's real Silero model to produce a
    /// confident whole-clip speech segment.
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
                "sine=frequency=220:duration=2,tremolo=f=4:d=0.9",
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
    fn run_suggestion_resolves_to_a_real_template_via_a_mock_ai_server() {
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let ffprobe = crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable");
        let dir =
            std::env::temp_dir().join(format!("ave-auto-template-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let recommendation_json = r#"{"version": 1, "template_id": "tmpl_tiktok", "reason": "fast-paced vertical-friendly content", "confidence": 0.82, "suggested_aspect": null}"#;
        let (base_url, rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(recommendation_json));

        let catalog = all_templates();
        let result = run_suggestion(
            &ffmpeg,
            &ffprobe,
            &source,
            &[],
            ai_settings(base_url),
            &catalog,
        )
        .expect("suggestion succeeds against a real synthetic clip + mock AI server");

        assert_eq!(result.template_id, "tmpl_tiktok");
        assert_eq!(result.template_name, "TikTok");
        assert_eq!(result.confidence, 0.82);

        // The mock server actually received a real HTTP request carrying the
        // constructed prompt — real signals, not a stubbed call.
        let captured = rx.recv().expect("mock server captured a request");
        assert_eq!(captured.method, "POST");
        assert!(captured.body.contains("tmpl_tiktok"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_suggestion_surfaces_a_clear_error_for_a_malformed_ai_response() {
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let ffprobe = crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable");
        let dir = std::env::temp_dir().join(format!(
            "ave-auto-template-bad-ai-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body("not json at all"));

        let catalog = all_templates();
        let err = run_suggestion(
            &ffmpeg,
            &ffprobe,
            &source,
            &[],
            ai_settings(base_url),
            &catalog,
        )
        .unwrap_err();
        assert_eq!(err.code, "AUTO_TEMPLATE_MALFORMED_JSON");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_suggestion_surfaces_a_clear_error_for_an_ai_recommended_unknown_template_id() {
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let ffprobe = crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable");
        let dir = std::env::temp_dir().join(format!(
            "ave-auto-template-unknown-id-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let recommendation_json = r#"{"version": 1, "template_id": "tmpl_does_not_exist", "reason": "x", "confidence": 0.5, "suggested_aspect": null}"#;
        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(recommendation_json));

        let catalog = all_templates();
        let err = run_suggestion(
            &ffmpeg,
            &ffprobe,
            &source,
            &[],
            ai_settings(base_url),
            &catalog,
        )
        .unwrap_err();
        assert_eq!(err.code, "AUTO_TEMPLATE_UNKNOWN_TEMPLATE_ID");

        std::fs::remove_dir_all(&dir).ok();
    }
}
