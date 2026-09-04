//! Turns a `RenderGraph` into a real `FfmpegArgs` "FFmpeg Plan" — the last
//! stage before `FFmpeg` itself (master prompt §69):
//! `Project -> RenderGraph -> FFmpeg Plan -> FFmpeg`.
//!
//! Reimplemented from `vendor/autocut/src-tauri/src/export_mp4.rs`'s
//! concat-demuxer *cutting* technique (trim each kept interval, re-encode),
//! but genuinely extended to multi-track compositing, which autocut's
//! version does not do at all (`docs/architecture-audit.md` §4:
//! single-source-only). The technique used here is different from autocut's
//! concat demuxer because compositing (overlay/opacity/transform) needs a
//! real filter graph, not just a flat list of source ranges:
//!
//! - Every clip gets its own dedicated `-i` input, trimmed at the input
//!   level (`-ss`/`-to` for video/audio, `-loop 1 -t` for a still image) —
//!   this is what `source_in_us`/`source_out_us` map to.
//! - Each clip's `ClipSettings` (scale/flip/rotate/opacity) and `speed`
//!   become a `scale,hflip,vflip,rotate,format+colorchannelmixer,setpts`
//!   filter chain, ending with a timestamp shift so the clip's frames land
//!   at `position_us` on the shared output timeline.
//! - Video tracks are composited back-to-front (ascending
//!   `Track::render_index`, i.e. bottom to top) via chained `overlay`
//!   filters onto a `color=` canvas, each one time-windowed with
//!   `enable='between(t,start,end)'` so a clip only draws during its own
//!   on-timeline span — this is the actual multi-track overlay compositing
//!   autocut's export never implemented.
//! - Audio tracks are time-shifted with `adelay` and mixed with `amix`
//!   (volume-compensated, since `amix` silently attenuates by `1/n`
//!   otherwise), respecting the effectively-muted/hidden exclusions already
//!   applied when the `RenderGraph` was built.
//!
//! **`Caption`/`Effect` no-op, honestly**: `graph.caption_nodes` and
//! `graph.effect_nodes` are *not* touched by this module at all — no caption
//! burn-in filter, no effect filter, is ever emitted for them. See
//! `render::graph` module doc comment for why (no caption-rendering system
//! or effect catalog exists yet). This is a deliberate, documented gap, not
//! an oversight — the day Phase 8 lands a caption burn-in filter, or an
//! effect catalog exists, this is the one place a filter needs to be added
//! per node kind.
//!
//! **Coordinate convention for `ClipSettings::transform_x/y`**: half-canvas
//! units, positive-y-is-up (matching pyJianYingDraft's own convention —
//! `vendor/capcut-mate/src/pyJianYingDraft/segment.py`'s doc comment notes
//! captions imported from Jianying take `transform_y = -0.8`, i.e. a
//! negative value moves *down* toward the bottom of the frame). FFmpeg's
//! `overlay` filter is y-down in pixel space, so the pixel offset below
//! negates `transform_y` to translate between the two.

use std::path::Path;

use crate::ffmpeg::command::FfmpegArgs;
use crate::project::ClipSettings;

use super::error::RenderError;
use super::graph::{AudioClipNode, RenderGraph, VideoClipNode};
use super::hwaccel::{resolve_video_encoder, EncoderBackend};
use super::presets::{AudioCodec, RenderSettings, VideoCodec};

#[derive(Debug, Clone)]
pub struct RenderPlan {
    pub args: FfmpegArgs,
    /// The full render's expected output duration, used by the caller to
    /// turn ffmpeg's raw `out_time_us` progress into a 0.0..=1.0 fraction.
    pub expected_duration_us: i64,
}

fn fmt_secs(us: i64) -> String {
    format!("{:.6}", us as f64 / 1_000_000.0)
}

/// `round()` on `f64` returns `f64`; this project's canvas/media dimensions
/// are always small enough (well under `i64`'s range) for the `as i64` cast
/// to be exact, so a helper just documents the intent at each call site.
fn round_i64(v: f64) -> i64 {
    v.round() as i64
}

struct VideoClipFilter {
    /// The `-i`-preceding per-input args, e.g. `-ss 1.000000 -to 3.000000`
    /// or `-loop 1 -framerate 30/1 -t 2.000000`.
    input_args: Vec<String>,
    source_path: String,
    /// The filter_complex chain segment for this clip alone, e.g.
    /// `[2:v]scale=1920:1080,setpts=(PTS-STARTPTS)/1.000000+0.000000/TB[v2]`.
    filter_chain: String,
    /// This clip's output label, e.g. `v2`.
    label: String,
    overlay_x: i64,
    overlay_y: i64,
    start_s: String,
    end_s: String,
}

fn build_video_clip_filter(
    input_index: usize,
    clip: &VideoClipNode,
    canvas_width: u32,
    canvas_height: u32,
    fps: (u32, u32),
) -> VideoClipFilter {
    let ClipSettings {
        opacity,
        flip_h,
        flip_v,
        rotation_deg,
        scale_x,
        scale_y,
        transform_x,
        transform_y,
    } = clip.settings;

    let on_duration_us = clip.on_timeline_duration_us();
    let input_args = if clip.is_image {
        vec![
            "-loop".to_string(),
            "1".to_string(),
            "-framerate".to_string(),
            format!("{}/{}", fps.0, fps.1),
            "-t".to_string(),
            fmt_secs(on_duration_us),
        ]
    } else {
        vec![
            "-ss".to_string(),
            fmt_secs(clip.source_in_us),
            "-to".to_string(),
            fmt_secs(clip.source_out_us),
        ]
    };

    let (scaled_w, scaled_h) = if clip.media_width == 0 || clip.media_height == 0 {
        // Unknown media dimensions (e.g. a probe that failed to report
        // width/height) — fall back to filling the canvas rather than
        // requesting an invalid 0x0 scale.
        (canvas_width, canvas_height)
    } else {
        (
            round_i64(clip.media_width as f64 * scale_x).max(2) as u32,
            round_i64(clip.media_height as f64 * scale_y).max(2) as u32,
        )
    };

    let mut chain = format!("[{input_index}:v]scale={scaled_w}:{scaled_h}");
    if flip_h {
        chain.push_str(",hflip");
    }
    if flip_v {
        chain.push_str(",vflip");
    }
    if rotation_deg != 0.0 {
        let angle = rotation_deg.to_radians();
        chain.push_str(&format!(
            ",rotate={angle:.6}:ow=rotw({angle:.6}):oh=roth({angle:.6}):c=none"
        ));
    }
    if (opacity - 1.0).abs() > f64::EPSILON {
        chain.push_str(&format!(",format=rgba,colorchannelmixer=aa={opacity:.4}"));
    }
    let speed = if clip.speed > 0.0 { clip.speed } else { 1.0 };
    let position_s = clip.position_us as f64 / 1_000_000.0;
    chain.push_str(&format!(
        ",setpts=(PTS-STARTPTS)/{speed:.6}+{position_s:.6}/TB"
    ));
    let label = format!("v{input_index}");
    chain.push_str(&format!("[{label}]"));

    // Pixel offset: center the scaled clip, then apply the half-canvas
    // transform. `transform_y` is negated (see module doc comment) to
    // convert from the project schema's y-up convention to ffmpeg's y-down
    // pixel space.
    let overlay_x = round_i64(
        (canvas_width as f64 - scaled_w as f64) / 2.0 + transform_x * canvas_width as f64 / 2.0,
    );
    let overlay_y = round_i64(
        (canvas_height as f64 - scaled_h as f64) / 2.0 - transform_y * canvas_height as f64 / 2.0,
    );

    VideoClipFilter {
        input_args,
        source_path: clip.source_path.clone(),
        filter_chain: chain,
        label,
        overlay_x,
        overlay_y,
        start_s: fmt_secs(clip.position_us),
        end_s: fmt_secs(clip.end_position_us()),
    }
}

/// Decompose `speed` into a chain of `atempo` filters, each within ffmpeg's
/// supported `[0.5, 2.0]` range (`atempo` itself rejects values outside
/// that). Returns `None` for `speed == 1.0` (no filter needed at all).
fn atempo_chain(speed: f64) -> Option<Vec<String>> {
    if (speed - 1.0).abs() < f64::EPSILON {
        return None;
    }
    let mut remaining = speed.clamp(0.0625, 16.0); // ffmpeg's own documented atempo bounds when chained
    let mut factors = Vec::new();
    while remaining > 2.0 {
        factors.push(2.0);
        remaining /= 2.0;
    }
    while remaining < 0.5 {
        factors.push(0.5);
        remaining /= 0.5;
    }
    factors.push(remaining);
    Some(
        factors
            .into_iter()
            .map(|f| format!("atempo={f:.6}"))
            .collect(),
    )
}

struct AudioClipFilter {
    input_args: Vec<String>,
    source_path: String,
    filter_chain: String,
    label: String,
}

fn build_audio_clip_filter(input_index: usize, clip: &AudioClipNode) -> AudioClipFilter {
    let input_args = vec![
        "-ss".to_string(),
        fmt_secs(clip.source_in_us),
        "-to".to_string(),
        fmt_secs(clip.source_out_us),
    ];

    let mut parts = vec![format!("[{input_index}:a]")];
    if let Some(atempo) = atempo_chain(clip.speed) {
        parts.push(atempo.join(","));
    }
    let position_ms = clip.position_us / 1_000;
    parts.push(format!("adelay=delays={position_ms}:all=1"));
    let label = format!("a{input_index}");
    let filter_chain = format!("{}{}[{label}]", parts[0], {
        let rest = &parts[1..];
        format!(",{}", rest.join(","))
    });

    AudioClipFilter {
        input_args,
        source_path: clip.source_path.clone(),
        filter_chain,
        label,
    }
}

fn audio_encoder_name(codec: AudioCodec) -> &'static str {
    match codec {
        AudioCodec::Aac => "aac",
        AudioCodec::Opus => "libopus",
        AudioCodec::Vorbis => "libvorbis",
    }
}

fn software_video_args(encoder: &str, settings: &RenderSettings) -> Vec<String> {
    let mut args = vec!["-c:v".to_string(), encoder.to_string()];
    if settings.video_codec != VideoCodec::Vp9 {
        args.push("-preset".to_string());
        args.push(settings.x264_preset.clone());
    }
    if let Some(crf) = settings.crf {
        if settings.video_codec == VideoCodec::Vp9 {
            // libvpx-vp9's pure-CRF ("constant quality") mode requires an
            // explicit -b:v 0 — otherwise -crf alone is ignored and vpx
            // falls back to a bitrate-constrained mode.
            args.push("-b:v".to_string());
            args.push("0".to_string());
        }
        args.push("-crf".to_string());
        args.push(crf.to_string());
    } else if let Some(kbps) = settings.video_bitrate_kbps {
        args.push("-b:v".to_string());
        args.push(format!("{kbps}k"));
    }
    args
}

fn hardware_video_args(
    encoder: &str,
    settings: &RenderSettings,
) -> Result<Vec<String>, RenderError> {
    let kbps = settings.video_bitrate_kbps.ok_or_else(|| RenderError::InvalidSettings {
        details: "hardware encoder backends require video_bitrate_kbps set (CRF is a libx264/265-only concept; software backends support it, hardware backends here do not)".into(),
    })?;
    Ok(vec![
        "-c:v".to_string(),
        encoder.to_string(),
        "-b:v".to_string(),
        format!("{kbps}k"),
        "-maxrate".to_string(),
        format!("{}k", kbps * 3 / 2),
        "-bufsize".to_string(),
        format!("{}k", kbps * 2),
    ])
}

/// Build the full FFmpeg render plan. Pure (no filesystem/process access) —
/// safe to unit-test directly against synthetic `RenderGraph`s.
pub fn build_ffmpeg_plan(
    graph: &RenderGraph,
    settings: &RenderSettings,
    output_path: &Path,
) -> Result<RenderPlan, RenderError> {
    settings.validate()?;
    if graph.duration_us <= 0 {
        return Err(RenderError::EmptyTimeline);
    }

    let backend = settings
        .hardware_encoder
        .unwrap_or(EncoderBackend::Software);
    let encoder = resolve_video_encoder(backend, settings.video_codec)?;

    let mut filter_parts: Vec<String> = Vec::new();
    let mut input_args: Vec<(Vec<String>, String)> = Vec::new(); // (per-input flags, path)
    let mut input_index = 0usize;

    // --- Video: base canvas, then one overlay per clip in z-order ---
    let total_dur_s = fmt_secs(graph.duration_us);
    let fps = (graph.canvas.fps.num, graph.canvas.fps.den);
    filter_parts.push(format!(
        "color=c=black:size={}x{}:duration={total_dur_s}:rate={}/{}[base0]",
        graph.canvas.width, graph.canvas.height, fps.0, fps.1
    ));
    let mut current_base = "base0".to_string();
    let mut overlay_count = 0usize;

    for layer in &graph.video_layers {
        for clip in &layer.clips {
            let vf = build_video_clip_filter(
                input_index,
                clip,
                graph.canvas.width,
                graph.canvas.height,
                fps,
            );
            input_args.push((vf.input_args, vf.source_path));
            filter_parts.push(vf.filter_chain);

            overlay_count += 1;
            let next_base = format!("base{overlay_count}");
            filter_parts.push(format!(
                "[{current_base}][{}]overlay={}:{}:enable='between(t,{},{})'[{next_base}]",
                vf.label, vf.overlay_x, vf.overlay_y, vf.start_s, vf.end_s
            ));
            current_base = next_base;
            input_index += 1;
        }
    }
    let final_video_label = current_base;

    // --- Audio: per-clip delay, then amix (volume-compensated) ---
    let mut audio_labels: Vec<String> = Vec::new();
    for layer in &graph.audio_layers {
        for clip in &layer.clips {
            let af = build_audio_clip_filter(input_index, clip);
            input_args.push((af.input_args, af.source_path));
            filter_parts.push(af.filter_chain);
            audio_labels.push(af.label);
            input_index += 1;
        }
    }
    let final_audio_label = match audio_labels.len() {
        0 => None,
        1 => Some(audio_labels[0].clone()),
        n => {
            let inputs = audio_labels
                .iter()
                .map(|l| format!("[{l}]"))
                .collect::<String>();
            filter_parts.push(format!(
                "{inputs}amix=inputs={n}:duration=longest:dropout_transition=0,volume={n}[aout]"
            ));
            Some("aout".to_string())
        }
    };

    // --- Assemble FfmpegArgs ---
    let mut args = FfmpegArgs::new().args(["-y", "-hide_banner", "-v", "error"]);
    for (flags, path) in &input_args {
        args = args.args(flags);
        args = args.input(Path::new(path));
    }

    let filter_complex = filter_parts.join(";");
    args = args.arg("-filter_complex").arg(&filter_complex);
    args = args.arg("-map").arg(format!("[{final_video_label}]"));
    if let Some(audio_label) = &final_audio_label {
        args = args.arg("-map").arg(format!("[{audio_label}]"));
    }

    let video_args = match backend {
        EncoderBackend::Software => software_video_args(encoder, settings),
        _ => hardware_video_args(encoder, settings)?,
    };
    args = args.args(&video_args);
    args = args.args(["-r", &format!("{}/{}", fps.0, fps.1)]);

    if final_audio_label.is_some() {
        args = args.args([
            "-c:a",
            audio_encoder_name(settings.audio_codec),
            "-b:a",
            &format!("{}k", settings.audio_bitrate_kbps),
        ]);
    }

    if settings.container == super::presets::Container::Mp4 {
        // `+faststart` moves the moov atom to the front so the file is
        // playable before fully downloaded/copied — a standard, low-cost
        // default for delivered MP4s.
        args = args.args(["-movflags", "+faststart"]);
    }

    args = args.args(["-progress", "pipe:1", "-nostats"]);
    args = args.path(output_path);

    Ok(RenderPlan {
        args,
        expected_duration_us: graph.duration_us,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{CanvasRatioPreset, CanvasV1, Rational};
    use crate::render::graph::{AudioLayer, VideoLayer};
    use crate::render::presets::{find_preset, Container};

    fn canvas() -> CanvasV1 {
        CanvasV1 {
            width: 1920,
            height: 1080,
            fps: Rational::new(30, 1),
            ratio_preset: CanvasRatioPreset::Ratio16x9,
        }
    }

    fn video_clip(id: &str, path: &str, in_us: i64, out_us: i64, pos_us: i64) -> VideoClipNode {
        VideoClipNode {
            clip_id: id.into(),
            source_path: path.into(),
            is_image: false,
            media_width: 1920,
            media_height: 1080,
            source_in_us: in_us,
            source_out_us: out_us,
            position_us: pos_us,
            speed: 1.0,
            settings: ClipSettings::default(),
        }
    }

    fn args_string(plan: &RenderPlan) -> String {
        plan.args
            .as_slice()
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn settings_1080p() -> RenderSettings {
        find_preset("p1080").unwrap().settings
    }

    #[test]
    fn single_video_track_maps_trim_and_position_into_ss_to_and_setpts() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 5_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip("c1", "D:/in.mp4", 1_000_000, 6_000_000, 0)],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(s.contains("-ss 1.000000"), "{s}");
        assert!(s.contains("-to 6.000000"), "{s}");
        assert!(s.contains("D:/in.mp4"), "{s}");
        assert!(
            s.contains("setpts=(PTS-STARTPTS)/1.000000+0.000000/TB"),
            "{s}"
        );
        assert!(s.contains("-map [base1]"), "{s}");
        assert_eq!(plan.expected_duration_us, 5_000_000);
    }

    #[test]
    fn clip_positioned_later_on_the_timeline_shifts_setpts_and_overlay_enable_window() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 8_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip("c1", "D:/in.mp4", 0, 3_000_000, 5_000_000)],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(
            s.contains("setpts=(PTS-STARTPTS)/1.000000+5.000000/TB"),
            "{s}"
        );
        assert!(s.contains("enable='between(t,5.000000,8.000000)'"), "{s}");
    }

    #[test]
    fn two_video_tracks_overlay_in_ascending_render_index_order() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 4_000_000,
            video_layers: vec![
                VideoLayer {
                    track_id: "bottom".into(),
                    render_index: 0,
                    clips: vec![video_clip("c_bottom", "D:/bottom.mp4", 0, 4_000_000, 0)],
                },
                VideoLayer {
                    track_id: "top".into(),
                    render_index: 1,
                    clips: vec![video_clip("c_top", "D:/top.mp4", 0, 4_000_000, 0)],
                },
            ],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        // bottom.mp4 must be input 0 (processed first, drawn first / lowest
        // z-order); top.mp4 must be input 1, overlaid on top of it.
        let bottom_pos = s.find("D:/bottom.mp4").unwrap();
        let top_pos = s.find("D:/top.mp4").unwrap();
        assert!(bottom_pos < top_pos, "{s}");
        assert!(s.contains("[base0][v0]overlay"), "{s}");
        assert!(s.contains("[base1][v1]overlay"), "{s}");
        assert!(s.contains("-map [base2]"), "{s}");
    }

    #[test]
    fn clip_settings_map_to_the_expected_filters() {
        let mut clip = video_clip("c1", "D:/in.mp4", 0, 2_000_000, 0);
        clip.settings = ClipSettings {
            opacity: 0.5,
            flip_h: true,
            flip_v: true,
            rotation_deg: 90.0,
            scale_x: 0.5,
            scale_y: 0.5,
            transform_x: 1.0,
            transform_y: -1.0,
        };
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![clip],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(s.contains("scale=960:540"), "{s}"); // 1920*0.5, 1080*0.5
        assert!(s.contains("hflip"), "{s}");
        assert!(s.contains("vflip"), "{s}");
        assert!(s.contains("rotate=1.570796"), "{s}"); // 90 degrees in radians
        assert!(s.contains("format=rgba,colorchannelmixer=aa=0.5000"), "{s}");
        // transform_x=1.0 (half canvas width right of center),
        // transform_y=-1.0 (half canvas height *down* from center, since
        // negative-y is down per the module's documented convention).
        // center_x = (1920-960)/2 = 480; + 1.0*960 = 1440
        // center_y = (1080-540)/2 = 270; - (-1.0)*540 = 270+540 = 810
        assert!(s.contains("overlay=1440:810"), "{s}");
    }

    #[test]
    fn image_clip_uses_loop_and_t_instead_of_ss_to() {
        let mut clip = video_clip("c1", "D:/photo.jpg", 0, 3_000_000, 0);
        clip.is_image = true;
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 3_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![clip],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(s.contains("-loop 1"), "{s}");
        assert!(s.contains("-t 3.000000"), "{s}");
        assert!(!s.contains("-ss"), "{s}");
    }

    #[test]
    fn double_speed_clip_halves_on_timeline_setpts_scaling() {
        let mut clip = video_clip("c1", "D:/in.mp4", 0, 4_000_000, 0);
        clip.speed = 2.0;
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000, // 4s of source at 2x speed = 2s on timeline
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![clip],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(
            s.contains("setpts=(PTS-STARTPTS)/2.000000+0.000000/TB"),
            "{s}"
        );
    }

    #[test]
    fn no_audio_layers_means_no_audio_map_or_codec_args() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip("c1", "D:/in.mp4", 0, 2_000_000, 0)],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(!s.contains("-c:a"), "{s}");
        assert_eq!(s.matches("-map").count(), 1);
    }

    #[test]
    fn single_audio_track_is_mapped_directly_without_amix() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 3_000_000,
            video_layers: vec![],
            audio_layers: vec![AudioLayer {
                track_id: "a1".into(),
                clips: vec![AudioClipNode {
                    clip_id: "ac1".into(),
                    source_path: "D:/audio.mp3".into(),
                    source_in_us: 0,
                    source_out_us: 3_000_000,
                    position_us: 0,
                    speed: 1.0,
                }],
            }],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(!s.contains("amix"), "{s}");
        assert!(s.contains("-map [a0]"), "{s}");
        assert!(s.contains("-c:a aac"), "{s}");
    }

    #[test]
    fn two_audio_tracks_mix_with_volume_compensation() {
        let mk = |id: &str, path: &str| AudioClipNode {
            clip_id: id.into(),
            source_path: path.into(),
            source_in_us: 0,
            source_out_us: 3_000_000,
            position_us: 0,
            speed: 1.0,
        };
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 3_000_000,
            video_layers: vec![],
            audio_layers: vec![
                AudioLayer {
                    track_id: "a1".into(),
                    clips: vec![mk("ac1", "D:/a.mp3")],
                },
                AudioLayer {
                    track_id: "a2".into(),
                    clips: vec![mk("ac2", "D:/b.mp3")],
                },
            ],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(
            s.contains(
                "[a0][a1]amix=inputs=2:duration=longest:dropout_transition=0,volume=2[aout]"
            ),
            "{s}"
        );
        assert!(s.contains("-map [aout]"), "{s}");
    }

    #[test]
    fn audio_clip_position_maps_to_adelay_milliseconds() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 5_000_000,
            video_layers: vec![],
            audio_layers: vec![AudioLayer {
                track_id: "a1".into(),
                clips: vec![AudioClipNode {
                    clip_id: "ac1".into(),
                    source_path: "D:/audio.mp3".into(),
                    source_in_us: 0,
                    source_out_us: 2_000_000,
                    position_us: 1_500_000,
                    speed: 1.0,
                }],
            }],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(s.contains("adelay=delays=1500:all=1"), "{s}");
    }

    #[test]
    fn empty_timeline_errors_rather_than_producing_a_degenerate_plan() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 0,
            video_layers: vec![],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let err =
            build_ffmpeg_plan(&graph, &settings_1080p(), Path::new("D:/out.mp4")).unwrap_err();
        assert!(matches!(err, RenderError::EmptyTimeline));
    }

    #[test]
    fn windows_paths_with_spaces_and_vietnamese_characters_survive_as_single_arguments() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip(
                    "c1",
                    r"C:\Video tiếng Việt\phỏng vấn 01.mp4",
                    0,
                    2_000_000,
                    0,
                )],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let plan = build_ffmpeg_plan(
            &graph,
            &settings_1080p(),
            Path::new(r"D:\My Videos\out.mp4"),
        )
        .unwrap();
        let raw: Vec<String> = plan
            .args
            .as_slice()
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(raw
            .iter()
            .any(|a| a == r"C:\Video tiếng Việt\phỏng vấn 01.mp4"));
        assert!(raw.iter().any(|a| a == r"D:\My Videos\out.mp4"));
    }

    #[test]
    fn hardware_backend_without_bitrate_errors_instead_of_guessing_one() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip("c1", "D:/in.mp4", 0, 2_000_000, 0)],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let mut settings = settings_1080p();
        settings.hardware_encoder = Some(EncoderBackend::Nvenc);
        // p1080 preset uses crf, not bitrate.
        let err = build_ffmpeg_plan(&graph, &settings, Path::new("D:/out.mp4")).unwrap_err();
        assert!(matches!(err, RenderError::InvalidSettings { .. }));
    }

    #[test]
    fn hardware_backend_with_bitrate_produces_nvenc_encoder_args() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip("c1", "D:/in.mp4", 0, 2_000_000, 0)],
            }],
            audio_layers: vec![],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let mut settings = find_preset("youtube_1080p").unwrap().settings; // bitrate-based
        settings.hardware_encoder = Some(EncoderBackend::Nvenc);
        let plan = build_ffmpeg_plan(&graph, &settings, Path::new("D:/out.mp4")).unwrap();
        let s = args_string(&plan);
        assert!(s.contains("-c:v h264_nvenc"), "{s}");
        assert!(s.contains("-b:v 8000k"), "{s}");
    }

    #[test]
    fn webm_container_uses_vp9_and_opus_style_args_shape() {
        let graph = RenderGraph {
            canvas: canvas(),
            duration_us: 2_000_000,
            video_layers: vec![VideoLayer {
                track_id: "v1".into(),
                render_index: 0,
                clips: vec![video_clip("c1", "D:/in.mp4", 0, 2_000_000, 0)],
            }],
            audio_layers: vec![AudioLayer {
                track_id: "a1".into(),
                clips: vec![AudioClipNode {
                    clip_id: "ac1".into(),
                    source_path: "D:/a.mp3".into(),
                    source_in_us: 0,
                    source_out_us: 2_000_000,
                    position_us: 0,
                    speed: 1.0,
                }],
            }],
            caption_nodes: vec![],
            effect_nodes: vec![],
        };
        let mut settings = settings_1080p();
        settings.container = Container::WebM;
        settings.video_codec = VideoCodec::Vp9;
        settings.audio_codec = AudioCodec::Opus;
        settings.crf = Some(30);
        let plan = build_ffmpeg_plan(&graph, &settings, Path::new("D:/out.webm")).unwrap();
        let s = args_string(&plan);
        assert!(s.contains("-c:v libvpx-vp9"), "{s}");
        assert!(s.contains("-b:v 0"), "{s}");
        assert!(s.contains("-crf 30"), "{s}");
        assert!(s.contains("-c:a libopus"), "{s}");
        assert!(!s.contains("-movflags"), "{s}");
    }
}
