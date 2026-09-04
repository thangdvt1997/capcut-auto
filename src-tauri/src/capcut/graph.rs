//! `CapCutExportGraph` — the CapCut-flavored sibling of `render::graph`'s
//! `RenderGraph` (master prompt §70): `Project -> CapCutExportGraph ->
//! CapCutAdapter -> Draft`. Resolves every `ProjectV1` reference
//! (media/clip/caption-style/animation/keyframe ids) once, up front, so
//! `capcut::adapter` never has to look back at `ProjectV1` itself — the same
//! separation `render::build_render_graph` already establishes for the
//! FFmpeg path.
//!
//! ## Track -> CapCut-track-kind mapping (documented, not silent)
//!
//! - `Video`/`Image`/`Overlay` tracks -> CapCut `Video` tracks (`Overlay` is
//!   just another visual layer here, matching `render::graph`'s own
//!   `is_visual_kind` and `fcpxml::document`'s treatment of `Overlay` as a
//!   video-ish track, not a sticker).
//! - `Audio` tracks -> CapCut `Audio` tracks.
//! - `Caption` tracks -> CapCut `Text` tracks, one CapCut text track per
//!   project caption track (not merged into one), so caption-track identity
//!   (locked/hidden/muted, separate authoring surfaces) survives the export.
//! - `Effect` tracks contribute no layer of their own — `project.effects`
//!   entries are resolved directly against their referencing `Clip`'s
//!   timeline position (`EffectNode` below), matching how
//!   `render::graph::build_render_graph` already represents `project.effects`
//!   independent of any `Effect`-kind track's own (usually empty) `clip_ids`.
//!   An effect whose `clip_id` doesn't resolve to any known clip is skipped
//!   (not a hard error) — an orphaned effect annotation blocking an entire
//!   export would be disproportionate; `fcpxml::document` makes the same
//!   "skip a dangling reference silently rather than fail export" call for
//!   its own dangling `media_id` case.
//! - Hidden tracks and effectively-muted audio tracks
//!   (`timeline::ops::effective_track_mute_state`) are excluded, matching
//!   `fcpxml`/`render::graph`.
//! - A missing `Clip::media_id` reference *is* a hard error
//!   (`CapCutError::DanglingReference`), matching `render::graph`'s
//!   `RenderError::MissingMedia` — a clip that claims to show media that
//!   isn't in `project.media` is a real data-integrity problem, not a
//!   softenable gap.

use std::collections::HashMap;

use crate::capcut::error::CapCutError;
use crate::project::{
    Animation, Caption, CaptionStyle, Clip, Effect, Keyframe, MediaItem, MediaKind, ProjectV1,
    Track, TrackKind,
};
use crate::timeline::ops::effective_track_mute_state;

#[derive(Debug, Clone)]
pub struct VideoClipNode {
    pub clip_id: String,
    pub source_path: String,
    pub is_image: bool,
    pub media_width: u32,
    pub media_height: u32,
    pub media_duration_us: i64,
    pub source_in_us: i64,
    pub source_out_us: i64,
    pub position_us: i64,
    pub speed: f64,
    pub clip_settings: crate::project::ClipSettings,
    pub animations: Vec<Animation>,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone)]
pub struct VideoLayer {
    pub track_id: String,
    pub track_name: String,
    pub render_index: i32,
    pub clips: Vec<VideoClipNode>,
}

#[derive(Debug, Clone)]
pub struct AudioClipNode {
    pub clip_id: String,
    pub source_path: String,
    pub media_duration_us: i64,
    pub source_in_us: i64,
    pub source_out_us: i64,
    pub position_us: i64,
    pub speed: f64,
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone)]
pub struct AudioLayer {
    pub track_id: String,
    pub track_name: String,
    pub clips: Vec<AudioClipNode>,
}

#[derive(Debug, Clone)]
pub struct CaptionNode {
    pub caption_id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub text: String,
    pub style: Option<CaptionStyle>,
}

#[derive(Debug, Clone)]
pub struct CaptionTrackNodes {
    pub track_id: String,
    pub track_name: String,
    pub render_index: i32,
    pub captions: Vec<CaptionNode>,
}

#[derive(Debug, Clone)]
pub struct EffectNode {
    pub effect_id: String,
    pub clip_id: String,
    pub kind: String,
    pub params: serde_json::Value,
    pub start_us: i64,
    pub end_us: i64,
}

#[derive(Debug, Clone)]
pub struct CapCutExportGraph {
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub fps: f64,
    /// Bottom-to-top z-order, matching `RenderGraph::video_layers`.
    pub video_layers: Vec<VideoLayer>,
    pub audio_layers: Vec<AudioLayer>,
    pub caption_tracks: Vec<CaptionTrackNodes>,
    pub effect_nodes: Vec<EffectNode>,
}

fn is_visual_kind(kind: TrackKind) -> bool {
    matches!(
        kind,
        TrackKind::Video | TrackKind::Image | TrackKind::Overlay
    )
}

fn clips_by_track<'a>(tracks: &'a [Track], clips: &'a [Clip]) -> HashMap<&'a str, Vec<&'a Clip>> {
    let by_id: HashMap<&str, &'a Clip> = clips.iter().map(|c| (c.id.as_str(), c)).collect();
    tracks
        .iter()
        .map(|t| {
            let mut ordered: Vec<&Clip> = t
                .clip_ids
                .iter()
                .filter_map(|id| by_id.get(id.as_str()).copied())
                .filter(|c| c.enabled)
                .collect();
            ordered.sort_by_key(|c| c.position_us);
            (t.id.as_str(), ordered)
        })
        .collect()
}

/// Build a `CapCutExportGraph` from `project`. See module doc comment for
/// the full track/effect mapping rules.
pub fn build_capcut_export_graph(project: &ProjectV1) -> Result<CapCutExportGraph, CapCutError> {
    let media_by_id: HashMap<&str, &MediaItem> =
        project.media.iter().map(|m| (m.id.as_str(), m)).collect();
    let mute_state = effective_track_mute_state(&project.tracks);
    let clips_by_track_map = clips_by_track(&project.tracks, &project.clips);

    let animations_by_clip: HashMap<&str, Vec<&Animation>> = {
        let mut map: HashMap<&str, Vec<&Animation>> = HashMap::new();
        for a in &project.animations {
            map.entry(a.clip_id.as_str()).or_default().push(a);
        }
        map
    };
    let keyframes_by_clip: HashMap<&str, Vec<&Keyframe>> = {
        let mut map: HashMap<&str, Vec<&Keyframe>> = HashMap::new();
        for k in &project.keyframes {
            map.entry(k.clip_id.as_str()).or_default().push(k);
        }
        map
    };
    let styles_by_id: HashMap<&str, &CaptionStyle> = project
        .caption_styles
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();
    let clip_by_id: HashMap<&str, &Clip> =
        project.clips.iter().map(|c| (c.id.as_str(), c)).collect();

    let mut video_layers = Vec::new();
    let mut audio_layers = Vec::new();
    let mut caption_tracks = Vec::new();

    for track in &project.tracks {
        if track.hidden {
            continue;
        }
        if is_visual_kind(track.kind) {
            let mut clips = Vec::new();
            for clip in clips_by_track_map
                .get(track.id.as_str())
                .into_iter()
                .flatten()
            {
                let Some(media_id) = &clip.media_id else {
                    continue;
                };
                let media = media_by_id.get(media_id.as_str()).ok_or_else(|| {
                    CapCutError::DanglingReference {
                        details: format!(
                            "clip '{}' references unknown media '{media_id}'",
                            clip.id
                        ),
                    }
                })?;
                clips.push(VideoClipNode {
                    clip_id: clip.id.clone(),
                    source_path: media.source_path.clone(),
                    is_image: media.kind == MediaKind::Image,
                    media_width: media.width,
                    media_height: media.height,
                    media_duration_us: media.duration_us,
                    source_in_us: clip.source_in_us,
                    source_out_us: clip.source_out_us,
                    position_us: clip.position_us,
                    speed: clip.speed,
                    clip_settings: clip.clip_settings.clone(),
                    animations: animations_by_clip
                        .get(clip.id.as_str())
                        .into_iter()
                        .flatten()
                        .map(|a| (*a).clone())
                        .collect(),
                    keyframes: keyframes_by_clip
                        .get(clip.id.as_str())
                        .into_iter()
                        .flatten()
                        .map(|k| (*k).clone())
                        .collect(),
                });
            }
            video_layers.push(VideoLayer {
                track_id: track.id.clone(),
                track_name: track.name.clone(),
                render_index: track.render_index,
                clips,
            });
        } else if track.kind == TrackKind::Audio {
            if *mute_state.get(&track.id).unwrap_or(&false) {
                continue;
            }
            let mut clips = Vec::new();
            for clip in clips_by_track_map
                .get(track.id.as_str())
                .into_iter()
                .flatten()
            {
                let Some(media_id) = &clip.media_id else {
                    continue;
                };
                let media = media_by_id.get(media_id.as_str()).ok_or_else(|| {
                    CapCutError::DanglingReference {
                        details: format!(
                            "clip '{}' references unknown media '{media_id}'",
                            clip.id
                        ),
                    }
                })?;
                clips.push(AudioClipNode {
                    clip_id: clip.id.clone(),
                    source_path: media.source_path.clone(),
                    media_duration_us: media.duration_us,
                    source_in_us: clip.source_in_us,
                    source_out_us: clip.source_out_us,
                    position_us: clip.position_us,
                    speed: clip.speed,
                    keyframes: keyframes_by_clip
                        .get(clip.id.as_str())
                        .into_iter()
                        .flatten()
                        .map(|k| (*k).clone())
                        .collect(),
                });
            }
            audio_layers.push(AudioLayer {
                track_id: track.id.clone(),
                track_name: track.name.clone(),
                clips,
            });
        } else if track.kind == TrackKind::Caption {
            let captions: Vec<CaptionNode> = project
                .captions
                .iter()
                .filter(|c| c.track_id == track.id)
                .map(|c: &Caption| CaptionNode {
                    caption_id: c.id.clone(),
                    start_us: c.start_us,
                    end_us: c.end_us,
                    text: c.text.clone(),
                    style: c
                        .style_id
                        .as_deref()
                        .and_then(|id| styles_by_id.get(id))
                        .map(|s| (*s).clone()),
                })
                .collect();
            caption_tracks.push(CaptionTrackNodes {
                track_id: track.id.clone(),
                track_name: track.name.clone(),
                render_index: track.render_index,
                captions,
            });
        }
        // TrackKind::Effect tracks contribute no layer of their own; see
        // module doc comment for how `project.effects` is resolved instead.
    }

    video_layers.sort_by_key(|l| l.render_index);

    let effect_nodes: Vec<EffectNode> = project
        .effects
        .iter()
        .filter_map(|e: &Effect| {
            let clip = clip_by_id.get(e.clip_id.as_str())?;
            Some(EffectNode {
                effect_id: e.id.clone(),
                clip_id: e.clip_id.clone(),
                kind: e.kind.clone(),
                params: e.params.clone(),
                start_us: clip.position_us,
                end_us: clip.position_us + (clip.source_out_us - clip.source_in_us).max(0),
            })
        })
        .collect();

    Ok(CapCutExportGraph {
        canvas_width: project.canvas.width,
        canvas_height: project.canvas.height,
        fps: project.canvas.fps.num as f64 / project.canvas.fps.den.max(1) as f64,
        video_layers,
        audio_layers,
        caption_tracks,
        effect_nodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        CanvasRatioPreset, CanvasV1, ClipSettings, MediaKind, Rational, TrackKind,
    };

    fn base_project() -> ProjectV1 {
        let mut p = ProjectV1::new("Graph Test");
        p.canvas = CanvasV1 {
            width: 1920,
            height: 1080,
            fps: Rational::new(30, 1),
            ratio_preset: CanvasRatioPreset::Ratio16x9,
        };
        p
    }

    fn media(id: &str, kind: MediaKind) -> MediaItem {
        MediaItem {
            id: id.into(),
            kind,
            source_path: format!("D:/media/{id}.mp4"),
            duration_us: 10_000_000,
            width: 1920,
            height: 1080,
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

    fn clip(id: &str, media_id: &str, pos: i64, dur: i64) -> Clip {
        Clip {
            id: id.into(),
            track_id: String::new(),
            media_id: Some(media_id.into()),
            source_in_us: 0,
            source_out_us: dur,
            position_us: pos,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        }
    }

    #[test]
    fn missing_media_reference_is_a_hard_error() {
        let mut p = base_project();
        p.tracks.push(Track {
            id: "v1".into(),
            kind: TrackKind::Video,
            name: "V1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        p.clips.push(clip("c1", "nonexistent", 0, 1_000_000));

        let err = build_capcut_export_graph(&p).expect_err("dangling media reference should error");
        assert!(matches!(err, CapCutError::DanglingReference { .. }));
    }

    #[test]
    fn video_and_audio_and_caption_tracks_are_resolved() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video));
        p.media.push(media("m2", MediaKind::Audio));
        p.tracks.push(Track {
            id: "v1".into(),
            kind: TrackKind::Video,
            name: "V1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        p.clips.push(clip("c1", "m1", 0, 5_000_000));
        p.tracks.push(Track {
            id: "a1".into(),
            kind: TrackKind::Audio,
            name: "A1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c2".into()],
        });
        p.clips.push(clip("c2", "m2", 0, 5_000_000));
        p.tracks.push(Track {
            id: "cap1".into(),
            kind: TrackKind::Caption,
            name: "Captions".into(),
            render_index: 1,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec![],
        });
        p.captions.push(Caption {
            id: "cap_a".into(),
            track_id: "cap1".into(),
            start_us: 0,
            end_us: 1_000_000,
            text: "hi".into(),
            words: vec![],
            style_id: None,
        });

        let graph = build_capcut_export_graph(&p).expect("graph builds");
        assert_eq!(graph.video_layers.len(), 1);
        assert_eq!(graph.audio_layers.len(), 1);
        assert_eq!(graph.caption_tracks.len(), 1);
        assert_eq!(graph.caption_tracks[0].captions.len(), 1);
    }

    #[test]
    fn overlay_track_is_treated_as_a_video_layer() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video));
        p.tracks.push(Track {
            id: "ov1".into(),
            kind: TrackKind::Overlay,
            name: "Overlay".into(),
            render_index: 5,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        p.clips.push(clip("c1", "m1", 0, 1_000_000));

        let graph = build_capcut_export_graph(&p).expect("graph builds");
        assert_eq!(graph.video_layers.len(), 1);
        assert_eq!(graph.video_layers[0].track_id, "ov1");
    }

    #[test]
    fn effect_with_no_matching_clip_is_skipped_not_errored() {
        let mut p = base_project();
        p.effects.push(Effect {
            id: "e1".into(),
            clip_id: "nonexistent".into(),
            kind: "blur".into(),
            params: serde_json::json!({}),
        });
        let graph = build_capcut_export_graph(&p).expect("graph builds despite orphan effect");
        assert!(graph.effect_nodes.is_empty());
    }

    #[test]
    fn effect_resolves_start_end_from_its_clip() {
        let mut p = base_project();
        p.media.push(media("m1", MediaKind::Video));
        p.tracks.push(Track {
            id: "v1".into(),
            kind: TrackKind::Video,
            name: "V1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into()],
        });
        p.clips.push(clip("c1", "m1", 2_000_000, 3_000_000));
        p.effects.push(Effect {
            id: "e1".into(),
            clip_id: "c1".into(),
            kind: "blur".into(),
            params: serde_json::json!({"radius": 4}),
        });

        let graph = build_capcut_export_graph(&p).expect("graph builds");
        assert_eq!(graph.effect_nodes.len(), 1);
        assert_eq!(graph.effect_nodes[0].start_us, 2_000_000);
        assert_eq!(graph.effect_nodes[0].end_us, 5_000_000);
    }
}
