//! `RenderGraph` — the clean intermediate representation between a
//! `ProjectV1` and an FFmpeg plan (master prompt §69):
//!
//! ```text
//! Project -> RenderGraph -> FFmpeg Plan -> FFmpeg
//! ```
//!
//! `build_render_graph` walks the timeline once and resolves everything an
//! FFmpeg plan builder needs (source paths, media dimensions, trim/position,
//! per-clip settings, track z-order, effective mute state) so
//! `render::plan` never has to look back at `ProjectV1`/`MediaItem` itself —
//! this is what keeps the UI (and any future render backend) from ever
//! constructing FFmpeg commands directly.
//!
//! **Honesty about `Caption`/`Effect`** (per this phase's task brief): there
//! is no caption burn-in system yet (Phase 8) and no effect catalog at all
//! (`Effect::params` is opaque JSON with no defined shapes). `CaptionNode`
//! and `EffectNode` below exist so this schema does not need to change shape
//! when those phases land, but `render::plan` treats every node in
//! `caption_nodes`/`effect_nodes` as a documented no-op today — see that
//! module's doc comment. Nothing here fabricates a burn-in or visual effect.

use std::collections::HashMap;

use crate::project::{
    CanvasV1, Clip, ClipSettings, MediaItem, MediaKind, ProjectV1, Track, TrackKind,
};
use crate::timeline::ops::effective_track_mute_state;

use super::error::RenderError;

/// One video/image/overlay-track clip, fully resolved: source file, media
/// dimensions (needed to turn `ClipSettings::scale_x/y` into pixel
/// dimensions at the FFmpeg-argument boundary), trim, timeline placement,
/// speed, and per-clip visual settings.
#[derive(Debug, Clone)]
pub struct VideoClipNode {
    pub clip_id: String,
    pub source_path: String,
    pub is_image: bool,
    pub media_width: u32,
    pub media_height: u32,
    pub source_in_us: i64,
    pub source_out_us: i64,
    pub position_us: i64,
    pub speed: f64,
    pub settings: ClipSettings,
}

impl VideoClipNode {
    /// This clip's duration as it appears on the output timeline — the
    /// source trim divided by playback speed (a `speed=2.0` clip plays in
    /// half the wall-clock time it occupies in the source).
    pub fn on_timeline_duration_us(&self) -> i64 {
        on_timeline_duration_us(self.source_in_us, self.source_out_us, self.speed)
    }

    pub fn end_position_us(&self) -> i64 {
        self.position_us + self.on_timeline_duration_us()
    }
}

/// One video-ish track (`Video`/`Image`/`Overlay` — all genuinely visual
/// layers per this phase's task brief), in the z-order it composites at.
#[derive(Debug, Clone)]
pub struct VideoLayer {
    pub track_id: String,
    pub render_index: i32,
    pub clips: Vec<VideoClipNode>,
}

/// One audio-track clip, fully resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioClipNode {
    pub clip_id: String,
    pub source_path: String,
    pub source_in_us: i64,
    pub source_out_us: i64,
    pub position_us: i64,
    pub speed: f64,
}

impl AudioClipNode {
    pub fn on_timeline_duration_us(&self) -> i64 {
        on_timeline_duration_us(self.source_in_us, self.source_out_us, self.speed)
    }

    pub fn end_position_us(&self) -> i64 {
        self.position_us + self.on_timeline_duration_us()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioLayer {
    pub track_id: String,
    pub clips: Vec<AudioClipNode>,
}

/// A `Caption` (`project.captions`) placed on a `Caption`-kind track.
/// Represented so the schema is stable across Phase 8, but `render::plan`
/// does not burn these in — see module doc comment.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionNode {
    pub caption_id: String,
    pub track_id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
}

/// An `Effect` (`project.effects`) attached to a clip. Represented so the
/// schema is stable once an effect catalog exists, but `render::plan` does
/// not apply any visual effect for these today — see module doc comment.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectNode {
    pub effect_id: String,
    pub clip_id: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct RenderGraph {
    pub canvas: CanvasV1,
    /// Total output duration, in microseconds — the max end position across
    /// every included video and audio clip. `0` if the timeline is empty.
    pub duration_us: i64,
    /// Bottom-to-top z-order (ascending `Track::render_index`).
    pub video_layers: Vec<VideoLayer>,
    pub audio_layers: Vec<AudioLayer>,
    pub caption_nodes: Vec<CaptionNode>,
    pub effect_nodes: Vec<EffectNode>,
}

fn on_timeline_duration_us(source_in_us: i64, source_out_us: i64, speed: f64) -> i64 {
    let source_span = (source_out_us - source_in_us).max(0) as f64;
    let speed = if speed > 0.0 { speed } else { 1.0 };
    (source_span / speed).round() as i64
}

fn is_visual_kind(kind: TrackKind) -> bool {
    matches!(
        kind,
        TrackKind::Video | TrackKind::Image | TrackKind::Overlay
    )
}

/// Build a `RenderGraph` from `project`. Respects:
/// - `Track::hidden` for visual tracks (a hidden video/image/overlay track
///   does not composite at all — its clips are simply not visited);
/// - effective audio mute/solo state (`timeline::ops::effective_track_mute_state`)
///   for audio tracks (an effectively-muted track's clips are excluded from
///   the mix);
/// - `Clip::enabled` (a disabled clip is skipped on any track kind).
///
/// `Track::locked` is deliberately **not** consulted — it is an editing
/// affordance (prevents accidental drag/trim in the UI), not a render
/// property; a locked-but-visible track still renders.
pub fn build_render_graph(project: &ProjectV1) -> Result<RenderGraph, RenderError> {
    let media_by_id: HashMap<&str, &MediaItem> =
        project.media.iter().map(|m| (m.id.as_str(), m)).collect();
    let mute_state = effective_track_mute_state(&project.tracks);
    let clips_by_track = clips_by_track(&project.tracks, &project.clips);

    let mut video_layers = Vec::new();
    let mut audio_layers = Vec::new();

    for track in &project.tracks {
        if is_visual_kind(track.kind) {
            if track.hidden {
                continue;
            }
            let mut clips = Vec::new();
            for clip in clips_by_track.get(track.id.as_str()).into_iter().flatten() {
                if !clip.enabled {
                    continue;
                }
                let Some(media_id) = &clip.media_id else {
                    // A visual clip with no media reference has nothing to
                    // draw (e.g. a placeholder); skip rather than error —
                    // an empty clip is not a render failure.
                    continue;
                };
                let media = media_by_id.get(media_id.as_str()).ok_or_else(|| {
                    RenderError::MissingMedia {
                        clip_id: clip.id.clone(),
                        media_id: media_id.clone(),
                    }
                })?;
                clips.push(VideoClipNode {
                    clip_id: clip.id.clone(),
                    source_path: media.source_path.clone(),
                    is_image: media.kind == MediaKind::Image,
                    media_width: media.width,
                    media_height: media.height,
                    source_in_us: clip.source_in_us,
                    source_out_us: clip.source_out_us,
                    position_us: clip.position_us,
                    speed: clip.speed,
                    settings: clip.clip_settings.clone(),
                });
            }
            clips.sort_by_key(|c| (c.position_us, c.clip_id.clone()));
            video_layers.push(VideoLayer {
                track_id: track.id.clone(),
                render_index: track.render_index,
                clips,
            });
        } else if track.kind == TrackKind::Audio {
            if *mute_state.get(&track.id).unwrap_or(&false) {
                continue;
            }
            let mut clips = Vec::new();
            for clip in clips_by_track.get(track.id.as_str()).into_iter().flatten() {
                if !clip.enabled {
                    continue;
                }
                let Some(media_id) = &clip.media_id else {
                    continue;
                };
                let media = media_by_id.get(media_id.as_str()).ok_or_else(|| {
                    RenderError::MissingMedia {
                        clip_id: clip.id.clone(),
                        media_id: media_id.clone(),
                    }
                })?;
                clips.push(AudioClipNode {
                    clip_id: clip.id.clone(),
                    source_path: media.source_path.clone(),
                    source_in_us: clip.source_in_us,
                    source_out_us: clip.source_out_us,
                    position_us: clip.position_us,
                    speed: clip.speed,
                });
            }
            clips.sort_by_key(|c| (c.position_us, c.clip_id.clone()));
            audio_layers.push(AudioLayer {
                track_id: track.id.clone(),
                clips,
            });
        }
        // TrackKind::Caption / TrackKind::Effect tracks contribute no video
        // or audio layer content; their clips (if any) are intentionally not
        // walked here — captions/effects are represented via
        // `project.captions`/`project.effects` below instead, which is the
        // real source of caption text / effect parameters.
    }

    video_layers.sort_by_key(|l| l.render_index);

    let caption_nodes = project
        .captions
        .iter()
        .map(|c| CaptionNode {
            caption_id: c.id.clone(),
            track_id: c.track_id.clone(),
            start_us: c.start_us,
            end_us: c.end_us,
            text: c.text.clone(),
        })
        .collect();

    let effect_nodes = project
        .effects
        .iter()
        .map(|e| EffectNode {
            effect_id: e.id.clone(),
            clip_id: e.clip_id.clone(),
            kind: e.kind.clone(),
        })
        .collect();

    let duration_us = video_layers
        .iter()
        .flat_map(|l| l.clips.iter().map(|c| c.end_position_us()))
        .chain(
            audio_layers
                .iter()
                .flat_map(|l| l.clips.iter().map(|c| c.end_position_us())),
        )
        .max()
        .unwrap_or(0);

    Ok(RenderGraph {
        canvas: project.canvas.clone(),
        duration_us,
        video_layers,
        audio_layers,
        caption_nodes,
        effect_nodes,
    })
}

fn clips_by_track<'a>(tracks: &'a [Track], clips: &'a [Clip]) -> HashMap<&'a str, Vec<&'a Clip>> {
    let mut by_id: HashMap<&str, &'a Clip> = HashMap::new();
    for clip in clips {
        by_id.insert(clip.id.as_str(), clip);
    }
    let mut out: HashMap<&str, Vec<&'a Clip>> = HashMap::new();
    for track in tracks {
        let ordered: Vec<&Clip> = track
            .clip_ids
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .collect();
        out.insert(track.id.as_str(), ordered);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{CanvasRatioPreset, MediaKind, Rational};

    fn media(id: &str, kind: MediaKind, w: u32, h: u32, dur_us: i64) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind,
            source_path: format!("D:/media/{id}.mp4"),
            duration_us: dur_us,
            width: w,
            height: h,
            fps: Rational::new(30, 1),
            codec: "h264".into(),
            bitrate: 0,
            audio_channels: 2,
            sample_rate: 48_000,
            rotation_deg: 0,
            created_at: None,
            proxy_path: None,
            thumbnail_path: None,
        }
    }

    fn track(id: &str, kind: TrackKind, render_index: i32, clip_ids: Vec<&str>) -> Track {
        Track {
            id: id.into(),
            kind,
            name: id.into(),
            render_index,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: clip_ids.into_iter().map(String::from).collect(),
        }
    }

    fn clip(
        id: &str,
        track_id: &str,
        media_id: &str,
        in_us: i64,
        out_us: i64,
        pos_us: i64,
    ) -> Clip {
        Clip {
            id: id.into(),
            track_id: track_id.into(),
            media_id: Some(media_id.into()),
            source_in_us: in_us,
            source_out_us: out_us,
            position_us: pos_us,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    fn base_project() -> ProjectV1 {
        let mut p = ProjectV1::new("Render Graph Test");
        p.canvas = CanvasV1 {
            width: 1920,
            height: 1080,
            fps: Rational::new(30, 1),
            ratio_preset: CanvasRatioPreset::Ratio16x9,
        };
        p
    }

    #[test]
    fn single_video_and_audio_track_produce_one_layer_each() {
        let mut p = base_project();
        p.media
            .push(media("m1", MediaKind::Video, 1920, 1080, 5_000_000));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.tracks.push(track("a1", TrackKind::Audio, 0, vec!["c2"]));
        p.clips.push(clip("c1", "v1", "m1", 0, 5_000_000, 0));
        p.clips.push(clip("c2", "a1", "m1", 0, 5_000_000, 0));

        let graph = build_render_graph(&p).expect("graph builds");
        assert_eq!(graph.video_layers.len(), 1);
        assert_eq!(graph.audio_layers.len(), 1);
        assert_eq!(graph.video_layers[0].clips.len(), 1);
        assert_eq!(graph.duration_us, 5_000_000);
    }

    #[test]
    fn video_layers_are_sorted_by_ascending_render_index_for_z_order() {
        let mut p = base_project();
        p.media
            .push(media("m1", MediaKind::Video, 1920, 1080, 1_000_000));
        // Insert the higher render_index track first to prove sorting, not
        // insertion order, determines z-order.
        p.tracks
            .push(track("top", TrackKind::Overlay, 5, vec!["c_top"]));
        p.tracks
            .push(track("bottom", TrackKind::Video, 0, vec!["c_bottom"]));
        p.clips.push(clip("c_top", "top", "m1", 0, 1_000_000, 0));
        p.clips
            .push(clip("c_bottom", "bottom", "m1", 0, 1_000_000, 0));

        let graph = build_render_graph(&p).expect("graph builds");
        let ids: Vec<&str> = graph
            .video_layers
            .iter()
            .map(|l| l.track_id.as_str())
            .collect();
        assert_eq!(ids, vec!["bottom", "top"]);
    }

    #[test]
    fn hidden_video_track_is_excluded_entirely() {
        let mut p = base_project();
        p.media
            .push(media("m1", MediaKind::Video, 1920, 1080, 1_000_000));
        let mut hidden = track("v1", TrackKind::Video, 0, vec!["c1"]);
        hidden.hidden = true;
        p.tracks.push(hidden);
        p.clips.push(clip("c1", "v1", "m1", 0, 1_000_000, 0));

        let graph = build_render_graph(&p).expect("graph builds");
        assert!(graph.video_layers.is_empty());
        assert_eq!(graph.duration_us, 0);
    }

    #[test]
    fn effectively_muted_audio_track_is_excluded_from_the_mix() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Audio, 0, 0, 1_000_000));
        let mut solo_track = track("a1", TrackKind::Audio, 0, vec!["c1"]);
        solo_track.solo = true;
        let other_track = track("a2", TrackKind::Audio, 0, vec!["c2"]);
        p.tracks.push(solo_track);
        p.tracks.push(other_track);
        p.clips.push(clip("c1", "a1", "m1", 0, 1_000_000, 0));
        p.clips.push(clip("c2", "a2", "m1", 0, 1_000_000, 0));

        let graph = build_render_graph(&p).expect("graph builds");
        // Only the solo'd track's layer should carry clips; the other
        // audio track is effectively muted (solo elsewhere) and excluded.
        assert_eq!(graph.audio_layers.len(), 1);
        assert_eq!(graph.audio_layers[0].track_id, "a1");
    }

    #[test]
    fn disabled_clips_are_skipped() {
        let mut p = base_project();
        p.media
            .push(media("m1", MediaKind::Video, 1920, 1080, 1_000_000));
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        let mut c = clip("c1", "v1", "m1", 0, 1_000_000, 0);
        c.enabled = false;
        p.clips.push(c);

        let graph = build_render_graph(&p).expect("graph builds");
        assert!(graph.video_layers[0].clips.is_empty());
    }

    #[test]
    fn missing_media_reference_errors() {
        let mut p = base_project();
        p.tracks.push(track("v1", TrackKind::Video, 0, vec!["c1"]));
        p.clips
            .push(clip("c1", "v1", "nonexistent", 0, 1_000_000, 0));

        let err = build_render_graph(&p).expect_err("should error on missing media");
        assert!(matches!(err, RenderError::MissingMedia { .. }));
    }

    #[test]
    fn caption_and_effect_nodes_are_captured_but_do_not_affect_duration() {
        let mut p = base_project();
        p.tracks.push(track("cap1", TrackKind::Caption, 0, vec![]));
        p.captions.push(crate::project::Caption {
            id: "cap_a".into(),
            track_id: "cap1".into(),
            start_us: 0,
            end_us: 100_000_000, // far beyond any video/audio clip
            text: "hello".into(),
            words: vec![],
            style_id: None,
        });
        p.effects.push(crate::project::Effect {
            id: "eff_a".into(),
            clip_id: "c1".into(),
            kind: "blur".into(),
            params: serde_json::json!({}),
        });

        let graph = build_render_graph(&p).expect("graph builds");
        assert_eq!(graph.caption_nodes.len(), 1);
        assert_eq!(graph.effect_nodes.len(), 1);
        // No video/audio content at all -> duration stays 0, proving
        // captions/effects are represented but not load-bearing for output
        // duration (that's plan.rs's no-op guarantee, exercised here at the
        // graph level too).
        assert_eq!(graph.duration_us, 0);
    }

    #[test]
    fn clip_speed_shortens_on_timeline_duration() {
        let node = VideoClipNode {
            clip_id: "c1".into(),
            source_path: "x.mp4".into(),
            is_image: false,
            media_width: 1920,
            media_height: 1080,
            source_in_us: 0,
            source_out_us: 4_000_000,
            position_us: 0,
            speed: 2.0,
            settings: ClipSettings::default(),
        };
        assert_eq!(node.on_timeline_duration_us(), 2_000_000);
        assert_eq!(node.end_position_us(), 2_000_000);
    }
}
