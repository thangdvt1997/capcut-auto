//! Highlight detection Tauri command surface (Phase 10 follow-up, master
//! prompt §21). Thin per master prompt §66 — all real logic lives in
//! `crate::highlights`/`crate::media::scene`/`crate::vad`/`crate::audio`.
//!
//! [`detect_media_scene_changes`] exposes the real, non-AI scene-change
//! signal on its own (useful even without running the full pipeline).
//! [`detect_highlights`] is the full pipeline: extracts PCM once, scores it
//! with the real Silero VAD provider for speech density, runs the real
//! ffmpeg scene-change detector, and — only when `ai_settings` is `Some` —
//! calls the configured `AIProvider` for semantic candidates
//! (`crate::highlights::semantic`), blending its scores with the real local
//! signals (`crate::highlights::combine`). With `ai_settings: None`, no AI
//! provider call is ever attempted; highlights are generated purely from
//! local signals (`crate::highlights::combine::local_only_highlights`).
//!
//! [`run_detection`] carries every real step and takes an already-resolved
//! `ffmpeg: &Path` rather than an `AppHandle` — the same "the Tauri command
//! is a one-line `resolve_ffmpeg` + delegate to a plain function" split
//! `commands::media`'s own real-subprocess tests rely on, so this pass's
//! tests can exercise the full real pipeline (real ffmpeg subprocess calls,
//! real VAD scoring, an optional mock AI server) without needing to stand up
//! a Tauri `AppHandle` at all.

use std::path::Path;

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;

use crate::audio::pcm::{self, PCM_SAMPLE_RATE};
use crate::commands::ai::{self, AiProviderSettings};
use crate::commands::media::resolve_ffmpeg;
use crate::error::AppErrorPayload;
use crate::highlights::{combine, semantic, Highlight};
use crate::media::error::MediaError;
use crate::media::scene;
use crate::project::TranscriptEntry;
use crate::vad::{self, VadProvider};

/// Default cap on how many highlights `detect_highlights` returns when the
/// caller doesn't specify one — generous enough to be useful, small enough
/// that a frontend list view doesn't need its own separate pagination yet.
const DEFAULT_MAX_HIGHLIGHTS: usize = 10;

/// Real, non-AI scene-change detection on its own (`media::scene`,
/// master prompt §21's "scene changes" signal) — exposed as its own command
/// so a caller can use it independently of the full highlight pipeline
/// below (e.g. a future scene-markers timeline feature, per
/// `IMPLEMENTATION_PLAN.md` Phase 11).
#[tauri::command]
#[specta::specta]
pub fn detect_media_scene_changes(
    app: AppHandle,
    media_path: String,
    threshold: Option<f32>,
) -> Result<Vec<i64>, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    scene::detect_scene_changes(
        &ffmpeg,
        Path::new(&media_path),
        threshold.unwrap_or(scene::DEFAULT_SCENE_THRESHOLD),
    )
    .map_err(|e| AppErrorPayload::from(&e))
}

/// Result alongside the highlights themselves, so a caller (and a test) can
/// tell whether the semantic (AI) signal actually ran without re-deriving it
/// from whether `ai_settings` was passed.
#[derive(Debug, Clone, Serialize, Type)]
pub struct HighlightDetectionResult {
    pub highlights: Vec<Highlight>,
    /// `true` only if AI settings were provided and the AI call/parse
    /// actually succeeded and its candidates were used. `false` means the
    /// result came entirely from real local signals — never a silent
    /// fallback the caller can't distinguish from "the AI path worked".
    pub used_ai_semantic_signal: bool,
}

/// The real pipeline, parameterized over an already-resolved `ffmpeg` path
/// rather than an `AppHandle` (module doc comment) — the only thing
/// [`detect_highlights`] itself adds is resolving that path and unwrapping
/// `max_highlights`' default. `pub(crate)` (widened from private) so
/// `commands::shorts`'s own pipeline can reuse this exact real detection
/// logic directly rather than re-deriving it — the same "widen a private
/// helper to `pub(crate)` for one more real caller" precedent
/// `commands::ai::resolve_api_key`/`build_provider` already established.
pub(crate) fn run_detection(
    ffmpeg: &Path,
    media_path: &Path,
    transcript: &[TranscriptEntry],
    total_duration_us: i64,
    ai_settings: Option<AiProviderSettings>,
    max_highlights: usize,
) -> Result<HighlightDetectionResult, AppErrorPayload> {
    // Real local signals: one PCM extraction feeds both real VAD scoring
    // (speech density) and real audio-energy computation — never extracted
    // twice.
    let samples =
        pcm::extract_pcm(ffmpeg, media_path).map_err(|e: MediaError| AppErrorPayload::from(&e))?;
    let vad_provider = vad::SileroVadProvider;
    let chunks = vad_provider
        .score_chunks(&samples, PCM_SAMPLE_RATE, None)
        .map_err(|e| AppErrorPayload::from(&e))?;
    let segments = vad::segments_from_scores(&chunks, vad::VadParams::default(), 0);

    // Real, non-AI scene-change detection.
    let scene_cuts =
        scene::detect_scene_changes(ffmpeg, media_path, scene::DEFAULT_SCENE_THRESHOLD)
            .map_err(|e| AppErrorPayload::from(&e))?;

    let (mut highlights, used_ai_semantic_signal) = match ai_settings {
        Some(settings) => {
            let api_key = ai::resolve_api_key(&settings).map_err(|e| AppErrorPayload::from(&e))?;
            let provider =
                ai::build_provider(&settings, api_key).map_err(|e| AppErrorPayload::from(&e))?;
            let prompt =
                semantic::build_highlight_prompt(transcript, total_duration_us, max_highlights);
            let request =
                prompt.into_request(settings.temperature, settings.timeout_ms, Some(2048));
            let response = provider
                .complete(&request)
                .map_err(|e| AppErrorPayload::from(&e))?;
            let candidates = semantic::parse_and_validate_candidates(&response.text)
                .map_err(|e| AppErrorPayload::from(&e))?;
            let blended = combine::blend_semantic_candidates(
                candidates,
                &segments,
                &samples,
                PCM_SAMPLE_RATE,
            );
            (blended, true)
        }
        None => {
            let windows =
                combine::candidate_windows_from_scene_changes(&scene_cuts, total_duration_us);
            let local = combine::local_only_highlights(
                &windows,
                &segments,
                &samples,
                PCM_SAMPLE_RATE,
                max_highlights,
            );
            (local, false)
        }
    };

    highlights.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    highlights.truncate(max_highlights);

    Ok(HighlightDetectionResult {
        highlights,
        used_ai_semantic_signal,
    })
}

/// Runs highlight detection end-to-end for one real media file (master
/// prompt §21's full signal list: transcript, speech density, audio energy,
/// scene changes, and — only when `ai_settings` is `Some` — semantic
/// importance). No real AI provider call is ever attempted unless the
/// caller passes real `ai_settings` (module doc comment).
#[tauri::command]
#[specta::specta]
pub fn detect_highlights(
    app: AppHandle,
    media_path: String,
    transcript: Vec<TranscriptEntry>,
    total_duration_us: i64,
    ai_settings: Option<AiProviderSettings>,
    max_highlights: Option<usize>,
) -> Result<HighlightDetectionResult, AppErrorPayload> {
    let ffmpeg = resolve_ffmpeg(&app).map_err(|e| AppErrorPayload::from(&e))?;
    run_detection(
        &ffmpeg,
        Path::new(&media_path),
        &transcript,
        total_duration_us,
        ai_settings,
        max_highlights.unwrap_or(DEFAULT_MAX_HIGHLIGHTS),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::test_http::spawn_one_shot;
    use crate::commands::ai::AiProviderKind;
    use crate::ffmpeg::command::{run_checked, FfmpegArgs};

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
    /// tone on the audio track — so the full pipeline (PCM extraction, VAD
    /// scoring, ffmpeg scene detection) runs against a real, tiny file
    /// rather than a mock, matching `render::job`/`media::scene`'s own
    /// synthetic-media test discipline.
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
    fn run_detection_with_no_ai_settings_uses_local_signals_only() {
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let dir =
            std::env::temp_dir().join(format!("ave-highlights-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let result = run_detection(&ffmpeg, &source, &[], 2_000_000, None, 5)
            .expect("local-only detection succeeds against a real synthetic clip");

        assert!(!result.used_ai_semantic_signal);
        assert!(
            !result.highlights.is_empty(),
            "expected at least one local-signal-derived highlight"
        );
        for h in &result.highlights {
            assert!(h.reason.contains("no AI provider configured"));
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_detection_with_ai_settings_blends_semantic_candidates_with_local_signals() {
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let dir =
            std::env::temp_dir().join(format!("ave-highlights-ai-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let candidates_json = r#"[{"start_us": 0, "end_us": 2000000, "score": 90.0, "title": "The whole clip", "reason": "it's the only thing here"}]"#;
        let (base_url, rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body(candidates_json));

        let result = run_detection(
            &ffmpeg,
            &source,
            &[],
            2_000_000,
            Some(ai_settings(base_url)),
            5,
        )
        .expect("AI-assisted detection succeeds against a real synthetic clip + mock server");

        assert!(result.used_ai_semantic_signal);
        assert_eq!(result.highlights.len(), 1);
        assert_eq!(result.highlights[0].title, "The whole clip");
        assert!(
            result.highlights[0].score > 0.0 && result.highlights[0].score <= 100.0,
            "{}",
            result.highlights[0].score
        );

        // The mock server actually received a real HTTP request (this
        // pass's "real HTTP-call-shape tested against the mock server"
        // requirement).
        let captured = rx.recv().expect("mock server captured a request");
        assert_eq!(captured.method, "POST");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_detection_surfaces_a_clear_error_for_a_malformed_ai_response() {
        let ffmpeg = crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable");
        let dir = std::env::temp_dir().join(format!(
            "ave-highlights-bad-ai-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_media(&ffmpeg, &dir);

        let (base_url, _rx) =
            spawn_one_shot("HTTP/1.1 200 OK", chat_completion_body("not json at all"));

        let err = run_detection(
            &ffmpeg,
            &source,
            &[],
            2_000_000,
            Some(ai_settings(base_url)),
            5,
        )
        .unwrap_err();
        assert_eq!(err.code, "HIGHLIGHT_MALFORMED_JSON");

        std::fs::remove_dir_all(&dir).ok();
    }
}
