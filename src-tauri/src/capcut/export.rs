//! Public entry points: walk a `CapCutExportGraph` calling `CapCutAdapter`'s
//! functions in the right order (`create_draft` first, materials/segments
//! before any dependent `add_animation`/`add_keyframe`/`add_mask` call,
//! matching `IMPLEMENTATION_PLAN.md`'s "materials before segments that
//! reference them" ordering requirement), then the `export_project_to_capcut_draft`
//! Tauri command fronting the whole `Project -> CapCutExportGraph ->
//! CapCutAdapter -> Draft` pipeline (master prompt §70). The command lives
//! here rather than under `commands/`, mirroring `fcpxml::export`'s own
//! precedent (see that module's doc comment for why).

use std::path::Path;

use crate::capcut::adapter::CapCutAdapter;
use crate::capcut::clip_settings::CapCutClipSettings;
use crate::capcut::error::CapCutError;
use crate::capcut::graph::{build_capcut_export_graph, AudioClipNode, VideoClipNode};
use crate::capcut::keyframe::from_project_keyframe;
use crate::capcut::material::{AudioMaterial, VideoMaterial, VideoMaterialKind};
use crate::capcut::timerange::Timerange;
use crate::capcut::track::TrackType;
use crate::error::AppErrorPayload;
use crate::project::ProjectV1;

fn file_stem_or(path: &str, fallback: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// A clip's on-timeline duration after `speed` is applied — same formula as
/// `render::graph`'s `on_timeline_duration_us`.
fn on_timeline_duration_us(source_in_us: i64, source_out_us: i64, speed: f64) -> i64 {
    let span = (source_out_us - source_in_us).max(0) as f64;
    let speed = if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        1.0
    };
    (span / speed).round() as i64
}

fn add_video_clip(
    adapter: &mut CapCutAdapter,
    track_id: &str,
    clip: &VideoClipNode,
) -> Result<String, CapCutError> {
    let clip_settings = CapCutClipSettings::from(&clip.clip_settings);
    let target_timerange = Timerange::new(
        clip.position_us,
        on_timeline_duration_us(clip.source_in_us, clip.source_out_us, clip.speed),
    );

    let segment_id = if clip.is_image {
        let material = VideoMaterial::new(
            clip.source_path.clone(),
            file_stem_or(&clip.source_path, &clip.clip_id),
            clip.media_duration_us,
            clip.media_width,
            clip.media_height,
            VideoMaterialKind::Photo,
        );
        adapter.add_image(track_id, material, target_timerange, clip_settings)?
    } else {
        let material = VideoMaterial::new(
            clip.source_path.clone(),
            file_stem_or(&clip.source_path, &clip.clip_id),
            clip.media_duration_us,
            clip.media_width,
            clip.media_height,
            VideoMaterialKind::Video,
        );
        let source_timerange = Timerange::new(
            clip.source_in_us,
            (clip.source_out_us - clip.source_in_us).max(0),
        );
        adapter.add_video(
            track_id,
            material,
            source_timerange,
            target_timerange,
            clip.speed,
            1.0,
            clip_settings,
        )?
    };

    for anim in &clip.animations {
        adapter.add_animation(
            &segment_id,
            anim.kind,
            &anim.name,
            0,
            anim.duration_us,
            true,
        )?;
    }
    for kf in &clip.keyframes {
        if let Some((property, resolved)) = from_project_keyframe(kf, clip.position_us) {
            adapter.add_keyframe(
                &segment_id,
                property,
                resolved.time_offset_us,
                resolved.value,
            )?;
        }
        // A keyframe whose `property` isn't one of the six this pass
        // supports is skipped, not fabricated — see `capcut::keyframe`'s
        // module doc comment.
    }
    Ok(segment_id)
}

fn add_audio_clip(
    adapter: &mut CapCutAdapter,
    track_id: &str,
    clip: &AudioClipNode,
) -> Result<String, CapCutError> {
    let material = AudioMaterial::new(
        clip.source_path.clone(),
        file_stem_or(&clip.source_path, &clip.clip_id),
        clip.media_duration_us,
    );
    let source_timerange = Timerange::new(
        clip.source_in_us,
        (clip.source_out_us - clip.source_in_us).max(0),
    );
    let target_timerange = Timerange::new(
        clip.position_us,
        on_timeline_duration_us(clip.source_in_us, clip.source_out_us, clip.speed),
    );
    let segment_id = adapter.add_audio(
        track_id,
        material,
        source_timerange,
        target_timerange,
        clip.speed,
        1.0,
    )?;

    for kf in &clip.keyframes {
        if let Some((property, resolved)) = from_project_keyframe(kf, clip.position_us) {
            adapter.add_keyframe(
                &segment_id,
                property,
                resolved.time_offset_us,
                resolved.value,
            )?;
        }
    }
    Ok(segment_id)
}

/// Builds a fully-populated `CapCutAdapter` from `project`: `create_draft`,
/// then every video/audio/caption/effect entry the `CapCutExportGraph`
/// resolved, in dependency order. Pure — does not touch the filesystem.
pub fn build_capcut_draft(project: &ProjectV1) -> Result<CapCutAdapter, CapCutError> {
    let graph = build_capcut_export_graph(project)?;
    let mut adapter =
        CapCutAdapter::create_draft(graph.canvas_width, graph.canvas_height, graph.fps);

    for layer in &graph.video_layers {
        let track_id = adapter.add_track(
            TrackType::Video,
            layer.track_name.clone(),
            layer.render_index,
        );
        for clip in &layer.clips {
            add_video_clip(&mut adapter, &track_id, clip)?;
        }
    }

    for layer in &graph.audio_layers {
        let track_id = adapter.add_track(TrackType::Audio, layer.track_name.clone(), 0);
        for clip in &layer.clips {
            add_audio_clip(&mut adapter, &track_id, clip)?;
        }
    }

    for caption_track in &graph.caption_tracks {
        let track_id = adapter.add_track(
            TrackType::Text,
            caption_track.track_name.clone(),
            caption_track.render_index,
        );
        for node in &caption_track.captions {
            let caption = crate::project::Caption {
                id: node.caption_id.clone(),
                track_id: caption_track.track_id.clone(),
                start_us: node.start_us,
                end_us: node.end_us,
                text: node.text.clone(),
                words: Vec::new(),
                style_id: node.style.as_ref().map(|s| s.id.clone()),
            };
            adapter.add_caption(&track_id, &caption, node.style.as_ref())?;
        }
    }

    if !graph.effect_nodes.is_empty() {
        let track_id = adapter.add_track(
            TrackType::Effect,
            "Effects",
            TrackType::Effect.default_render_index(),
        );
        for effect in &graph.effect_nodes {
            let timerange =
                Timerange::new(effect.start_us, (effect.end_us - effect.start_us).max(0));
            adapter.add_effect(&track_id, &effect.kind, effect.params.clone(), timerange)?;
        }
    }

    Ok(adapter)
}

/// Builds the draft and writes it to `draft_output_dir` (`draft_content.json`
/// + `draft_info.json`, see `CapCutAdapter::export_draft`).
pub fn export_project_to_capcut_draft_at(
    project: &ProjectV1,
    draft_output_dir: &Path,
) -> Result<(), CapCutError> {
    let adapter = build_capcut_draft(project)?;
    adapter.export_draft(draft_output_dir)
}

/// Tauri command: export `project` as a CapCut/Jianying draft folder at
/// `draft_output_path`. Specta-typed, following `fcpxml::export::export_fcpxml`'s
/// naming/error-envelope convention.
#[tauri::command]
#[specta::specta]
pub fn export_project_to_capcut_draft(
    project: ProjectV1,
    draft_output_path: String,
) -> Result<(), AppErrorPayload> {
    export_project_to_capcut_draft_at(&project, Path::new(&draft_output_path))
        .map_err(|e| AppErrorPayload::from(&e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        CanvasRatioPreset, CanvasV1, Caption, CaptionAlignment, CaptionAnchor, CaptionPosition,
        CaptionStyle, Clip, ClipSettings, Color, MediaItem, MediaKind, Rational, SafeMargins,
        Track, TrackKind,
    };

    fn sample_project() -> ProjectV1 {
        let mut p = ProjectV1::new("Round Trip");
        p.canvas = CanvasV1 {
            width: 1080,
            height: 1920,
            fps: Rational::new(30, 1),
            ratio_preset: CanvasRatioPreset::Ratio9x16,
        };
        p.media.push(MediaItem {
            id: "m1".into(),
            kind: MediaKind::Video,
            source_path: "C:/media/clip1.mp4".into(),
            duration_us: 20_000_000,
            width: 1080,
            height: 1920,
            fps: Rational::new(30, 1),
            codec: "h264".into(),
            bitrate: 5_000_000,
            audio_channels: 2,
            sample_rate: 48_000,
            rotation_deg: 0,
            created_at: None,
            proxy_path: None,
            thumbnail_path: None,
        });
        p.tracks.push(Track {
            id: "v1".into(),
            kind: TrackKind::Video,
            name: "V1".into(),
            render_index: 0,
            locked: false,
            hidden: false,
            muted: false,
            solo: false,
            clip_ids: vec!["c1".into(), "c2".into()],
        });
        p.clips.push(Clip {
            id: "c1".into(),
            track_id: "v1".into(),
            media_id: Some("m1".into()),
            source_in_us: 0,
            source_out_us: 3_000_000,
            position_us: 0,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
        p.clips.push(Clip {
            id: "c2".into(),
            track_id: "v1".into(),
            media_id: Some("m1".into()),
            source_in_us: 3_000_000,
            source_out_us: 6_000_000,
            position_us: 3_000_000,
            speed: 1.0,
            enabled: true,
            group_id: None,
            clip_settings: ClipSettings::default(),
        });
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
        p.caption_styles.push(CaptionStyle {
            id: "style1".into(),
            name: "Default".into(),
            font_family: "Arial".into(),
            font_size: 32.0,
            bold: false,
            italic: false,
            alignment: CaptionAlignment::Center,
            position: CaptionPosition {
                anchor: CaptionAnchor::Bottom,
                offset_x: 0.0,
                offset_y: 0.0,
            },
            text_color: Color::WHITE,
            background: None,
            outline: None,
            shadow: None,
            opacity: 1.0,
            safe_margins: SafeMargins::default(),
        });
        p.captions.push(Caption {
            id: "cap_a".into(),
            track_id: "cap1".into(),
            start_us: 0,
            end_us: 2_000_000,
            text: "Hello, world!".into(),
            words: vec![],
            style_id: Some("style1".into()),
        });
        p
    }

    #[test]
    fn full_pipeline_produces_a_well_formed_draft_with_expected_segments() {
        let project = sample_project();
        let adapter =
            build_capcut_draft(&project).expect("pipeline should succeed on a small real project");
        let exported = adapter.script.export_json();

        assert_eq!(exported["canvas_config"]["width"], serde_json::json!(1080));
        assert_eq!(exported["canvas_config"]["height"], serde_json::json!(1920));

        let tracks = exported["tracks"].as_array().expect("tracks array");
        assert_eq!(tracks.len(), 2, "one video track + one text track");

        let video_track = tracks
            .iter()
            .find(|t| t["type"] == "video")
            .expect("a video track exists");
        assert_eq!(
            video_track["segments"].as_array().unwrap().len(),
            2,
            "two clips on the video track"
        );

        let text_track = tracks
            .iter()
            .find(|t| t["type"] == "text")
            .expect("a text track exists");
        assert_eq!(
            text_track["segments"].as_array().unwrap().len(),
            1,
            "one caption"
        );

        let materials = &exported["materials"];
        assert_eq!(
            materials["videos"].as_array().unwrap().len(),
            2,
            "one VideoMaterial per clip (not deduped: distinct in/out ranges)"
        );
        assert_eq!(materials["texts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn export_project_to_capcut_draft_at_writes_real_files() {
        let project = sample_project();
        let dir = std::env::temp_dir().join(format!(
            "capcut_export_pipeline_test_{}",
            std::process::id()
        ));
        export_project_to_capcut_draft_at(&project, &dir).expect("export should succeed");
        assert!(dir.join("draft_content.json").exists());
        assert!(dir.join("draft_info.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_media_reference_surfaces_as_a_dangling_reference_error() {
        let mut project = sample_project();
        project.media.clear();
        let err = build_capcut_draft(&project)
            .expect_err("dangling media reference should fail the pipeline");
        assert!(matches!(err, CapCutError::DanglingReference { .. }));
    }
}
