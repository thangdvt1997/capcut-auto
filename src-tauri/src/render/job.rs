//! Runs a `RenderPlan` via `ffmpeg::command::run_with_progress`, mirroring
//! `media::proxy::generate_proxy`'s exact pattern (master prompt §44/§45):
//! a fraction-complete progress callback, `AtomicBool`-flag cancellation
//! that kills the ffmpeg child cleanly, and partial-output cleanup on both
//! cancellation and failure so a cancelled/failed render never leaves a
//! truncated file behind pretending to be a finished one.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use specta::Type;

use crate::ffmpeg::command::run_with_progress;

use super::error::RenderError;
use super::plan::RenderPlan;

#[derive(Debug, Clone, Copy, Serialize, Type)]
pub struct RenderJobProgress {
    /// 0.0..=1.0, `None` until ffmpeg reports its first `out_time_us`.
    pub fraction: Option<f64>,
    pub speed: Option<f64>,
    pub done: bool,
}

/// Execute `plan`, calling `on_progress` as ffmpeg reports it. `output_path`
/// must match `plan.args`' final output argument — passed separately here
/// only so this function can delete the right file on cancellation/failure
/// without re-parsing it back out of `plan.args`.
pub fn run_render_job(
    ffmpeg: &Path,
    plan: &RenderPlan,
    output_path: &Path,
    cancel: Option<&AtomicBool>,
    mut on_progress: impl FnMut(RenderJobProgress),
) -> Result<(), RenderError> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| RenderError::RenderFailed {
            details: format!("creating output dir {}: {e}", parent.display()),
        })?;
    }

    let expected = plan.expected_duration_us;
    let result = run_with_progress(
        ffmpeg,
        &plan.args,
        |p| {
            let fraction = if expected > 0 {
                p.out_time_us
                    .map(|us| (us as f64 / expected as f64).clamp(0.0, 1.0))
            } else {
                None
            };
            on_progress(RenderJobProgress {
                fraction,
                speed: p.speed,
                done: p.done,
            });
        },
        cancel,
    );

    match result {
        Ok(status) if status.success() => {
            on_progress(RenderJobProgress {
                fraction: Some(1.0),
                speed: None,
                done: true,
            });
            Ok(())
        }
        Ok(status) => {
            let _ = std::fs::remove_file(output_path);
            Err(RenderError::RenderFailed {
                details: format!("ffmpeg exited with {status}"),
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(output_path);
            if e.to_string().contains("cancelled") {
                Err(RenderError::Cancelled)
            } else {
                Err(RenderError::RenderFailed {
                    details: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::command::FfmpegArgs;
    use crate::project::{CanvasRatioPreset, CanvasV1, ClipSettings, Rational};
    use crate::render::graph::{RenderGraph, VideoClipNode, VideoLayer};
    use crate::render::plan::build_ffmpeg_plan;
    use crate::render::presets::find_preset;

    fn canvas() -> CanvasV1 {
        CanvasV1 {
            width: 320,
            height: 240,
            fps: Rational::new(10, 1),
            ratio_preset: CanvasRatioPreset::Custom,
        }
    }

    /// Builds a real, tiny render job against an actual synthetic ffmpeg
    /// source (not a mock) — this is the same "real subprocess, small input"
    /// discipline `media::proxy`'s own tests use.
    fn synth_source(ffmpeg: &Path, dir: &Path) -> std::path::PathBuf {
        let source = dir.join("in.mp4");
        let args = FfmpegArgs::new()
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-shortest",
            ])
            .path(&source);
        crate::ffmpeg::command::run_checked(ffmpeg, &args).expect("synthesizing test source");
        source
    }

    #[test]
    fn renders_a_real_tiny_clip_and_reports_completion() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-render-job-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let out = dir.join("out.mp4");

        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![VideoClipNode {
                    clip_id: "c1".into(),
                    source_path: source.to_string_lossy().to_string(),
                    is_image: false,
                    media_width: 320,
                    media_height: 240,
                    source_in_us: 0,
                    source_out_us: 2_000_000,
                    position_us: 0,
                    speed: 1.0,
                    settings: ClipSettings::default(),
                }],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let mut settings = find_preset("fast_preview").unwrap().settings;
        settings.width = 320;
        settings.height = 240;
        settings.fps = Rational::new(10, 1);
        let plan = build_ffmpeg_plan(&graph, &settings, &out, &[]).expect("plan builds");

        let mut saw_done = false;
        run_render_job(&ffmpeg, &plan, &out, None, |p| {
            if p.done {
                saw_done = true;
            }
        })
        .expect("render job succeeds");

        assert!(saw_done);
        assert!(out.exists());

        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let probed = crate::media::probe::probe(&ffprobe, &out).expect("probing rendered output");
        assert_eq!(probed.width, 320);
        assert_eq!(probed.height, 240);
        assert!(probed.has_video);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancelling_immediately_removes_partial_output_and_errors() {
        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir =
            std::env::temp_dir().join(format!("ave-render-cancel-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let out = dir.join("out.mp4");

        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![VideoClipNode {
                    clip_id: "c1".into(),
                    source_path: source.to_string_lossy().to_string(),
                    is_image: false,
                    media_width: 320,
                    media_height: 240,
                    source_in_us: 0,
                    source_out_us: 2_000_000,
                    position_us: 0,
                    speed: 1.0,
                    settings: ClipSettings::default(),
                }],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let mut settings = find_preset("fast_preview").unwrap().settings;
        settings.width = 320;
        settings.height = 240;
        settings.fps = Rational::new(10, 1);
        let plan = build_ffmpeg_plan(&graph, &settings, &out, &[]).expect("plan builds");

        let cancel = AtomicBool::new(true); // already cancelled before the job starts
        let result = run_render_job(&ffmpeg, &plan, &out, Some(&cancel), |_| {});

        assert!(matches!(result, Err(RenderError::Cancelled)));
        assert!(
            !out.exists(),
            "cancelled render must not leave a partial file behind"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real end-to-end audio-feature render test (master prompt §38): a real
    /// tiny clip whose audio track has volume reduction + fade-in/fade-out
    /// configured, actually rendered via real ffmpeg — mirroring this
    /// module's own real-render-test rigor (`renders_a_real_tiny_clip_and_
    /// reports_completion`) rather than only asserting on generated
    /// arguments (which `render::plan`'s own tests already cover).
    #[test]
    fn renders_a_real_clip_with_volume_and_fade_audio_features() {
        use crate::render::graph::AudioClipNode;
        use crate::render::plan::build_ffmpeg_plan as build_plan;

        let ffmpeg =
            crate::ffmpeg::binaries::ffmpeg_path(None).expect("ffmpeg resolvable in test env");
        let dir = std::env::temp_dir().join(format!(
            "ave-render-audio-features-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source = synth_source(&ffmpeg, &dir);
        let out = dir.join("out.mp4");

        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![VideoClipNode {
                    clip_id: "c1".into(),
                    source_path: source.to_string_lossy().to_string(),
                    is_image: false,
                    media_width: 320,
                    media_height: 240,
                    source_in_us: 0,
                    source_out_us: 2_000_000,
                    position_us: 0,
                    speed: 1.0,
                    settings: ClipSettings::default(),
                }],
            }],
            audio_layers: vec![crate::render::graph::AudioLayer {
                track_id: "a1".into(),
                clips: vec![AudioClipNode {
                    clip_id: "ac1".into(),
                    source_path: source.to_string_lossy().to_string(),
                    source_in_us: 0,
                    source_out_us: 2_000_000,
                    position_us: 0,
                    speed: 1.0,
                    volume: 0.5,
                    muted: false,
                    fade_in_us: 250_000,
                    fade_out_us: 250_000,
                    normalize: false,
                    noise_reduction: false,
                }],
                ducking: None,
            }],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let mut settings = find_preset("fast_preview").unwrap().settings;
        settings.width = 320;
        settings.height = 240;
        settings.fps = Rational::new(10, 1);
        let plan = build_plan(&graph, &settings, &out, &[]).expect("plan builds");

        let mut saw_done = false;
        run_render_job(&ffmpeg, &plan, &out, None, |p| {
            if p.done {
                saw_done = true;
            }
        })
        .expect("render job with audio features succeeds against real ffmpeg");

        assert!(saw_done);
        assert!(out.exists());

        let ffprobe =
            crate::ffmpeg::binaries::ffprobe_path(None).expect("ffprobe resolvable in test env");
        let probed = crate::media::probe::probe(&ffprobe, &out).expect("probing rendered output");
        assert!(probed.has_video);
        assert!(
            probed.has_audio,
            "the rendered output must still carry an audio stream after volume/fade filters"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
