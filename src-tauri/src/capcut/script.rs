//! `ScriptFile`/`ScriptMaterial` — port of `script_file.py`'s top-level
//! draft-content JSON structure.
//!
//! ## The "materials only get added on `add_segment`" invariant
//!
//! `script_file.py`'s `ScriptFile.add_segment` is the *only* place that
//! pushes a segment's backing material (`VideoMaterial`/`AudioMaterial`/
//! `SegmentAnimations`/`Mask`/...) into `self.materials` — constructing a
//! `VideoSegment`/`TextSegment`/... on its own never touches
//! `ScriptFile.materials` at all (`video_segment.py`/`text_segment.py`'s
//! constructors just clone/hold the material object).
//!
//! This module preserves that invariant with a **derived-from-live-tracks**
//! design rather than a separately-maintained push bucket:
//! `ScriptFile::materials()` walks `self.tracks` fresh every time it's
//! called (`ScriptMaterial::from_tracks` below), so a segment's materials
//! appear if and only if that segment is actually present in a track — which
//! only ever happens via `ScriptFile::add_segment`. This is deliberately
//! *not* a literal field-for-field port of `script_file.py`'s mutable
//! `self.materials` bucket: `capcut::adapter`'s `add_mask`/`add_animation`
//! mutate an already-inserted segment in place (see that module's doc
//! comment for why this adapter allows that, unlike pyJianYingDraft's
//! attach-before-insert-only fluent API) — a snapshot bucket populated once
//! at `add_segment` time would silently go stale the moment a later
//! `add_mask`/`add_animation` call mutated that segment's mask/animations
//! field, since nothing would ever re-collect it. Deriving from live track
//! state avoids that whole class of bug: whatever the segment's *current*
//! state is when the draft is finally exported is exactly what gets
//! collected. `tests::constructing_a_segment_does_not_touch_the_materials_bucket`
//! is the regression test the task brief calls for; it holds under this
//! design for the same reason (an un-added segment lives in no track, so it
//! contributes nothing no matter how `materials()` is computed).
//!
//! The base skeleton `ScriptFile::new` builds (`base_skeleton` below)
//! mirrors `vendor/capcut-mate/src/pyJianYingDraft/assets/draft_content_template.json`
//! key-for-key — see `tests::skeleton_matches_every_top_level_key_in_the_reference_template`,
//! which parses that actual vendored file and diffs its key set against
//! ours (the "validate output's JSON *structure* against the real reference
//! template" this phase's brief asks for, since there is no real installed
//! CapCut in this environment to validate against instead).

use std::collections::HashSet;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::capcut::error::CapCutError;
use crate::capcut::material::{AudioMaterial, VideoMaterial};
use crate::capcut::track::{SegmentSlot, Track, TrackType};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptMaterial {
    pub videos: Vec<VideoMaterial>,
    pub audios: Vec<AudioMaterial>,
    /// `materials.texts[]` — `TextSegment::export_material()` output.
    pub texts: Vec<Value>,
    /// `materials.stickers[]` — `StickerSegment::export_material()` output.
    pub stickers: Vec<Value>,
    /// `materials.video_effects[]` — `EffectSegment::material_json()` output.
    pub video_effects: Vec<Value>,
    /// `materials.effects[]` (filters+mix-modes bucket in the Python
    /// original; this pass only ever populates it from `FilterSegment`).
    pub effects: Vec<Value>,
    /// `materials.speeds[]`.
    pub speeds: Vec<Value>,
    /// `materials.masks[]`.
    pub masks: Vec<Value>,
    /// `materials.material_animations[]`.
    pub material_animations: Vec<Value>,
}

impl ScriptMaterial {
    /// Walks every track's segments and collects what each contributes.
    /// See module doc comment for why this is derived fresh rather than
    /// maintained as a separately-populated bucket.
    pub fn from_tracks(tracks: &[Track]) -> Self {
        let mut m = ScriptMaterial::default();
        let mut seen_video_ids: HashSet<String> = HashSet::new();
        let mut seen_audio_ids: HashSet<String> = HashSet::new();

        for track in tracks {
            for slot in &track.segments {
                match slot {
                    SegmentSlot::Video(seg) => {
                        if seen_video_ids.insert(seg.material.material_id.clone()) {
                            m.videos.push(seg.material.clone());
                        }
                        if let Some(animations) = &seg.visual.animations {
                            m.material_animations.push(animations.export_json());
                        }
                        if let Some(mask) = &seg.mask {
                            m.masks.push(mask.export_json());
                        }
                        m.speeds.push(seg.visual.media.speed_material_json());
                    }
                    SegmentSlot::Audio(seg) => {
                        if seen_audio_ids.insert(seg.material.material_id.clone()) {
                            m.audios.push(seg.material.clone());
                        }
                        m.speeds.push(seg.media.speed_material_json());
                    }
                    SegmentSlot::Text(seg) => {
                        m.texts.push(seg.export_material());
                        if let Some(animations) = &seg.visual.animations {
                            m.material_animations.push(animations.export_json());
                        }
                    }
                    SegmentSlot::Sticker(seg) => m.stickers.push(seg.export_material()),
                    SegmentSlot::Effect(seg) => m.video_effects.push(seg.material_json()),
                    SegmentSlot::Filter(seg) => m.effects.push(seg.material_json()),
                }
            }
        }
        m
    }

    /// Matches `ScriptMaterial.export_json` in `script_file.py`; every array
    /// this pass never populates (no ported catalog/feature backs it) stays
    /// the documented empty `[]` the template itself starts with.
    pub fn export_json(&self) -> Value {
        json!({
            "ai_translates": [], "audio_balances": [], "audio_effects": [], "audio_fades": [],
            "audio_track_indexes": [], "audios": self.audios.iter().map(AudioMaterial::export_json).collect::<Vec<_>>(),
            "beats": [], "canvases": [], "chromas": [], "color_curves": [], "digital_humans": [],
            "drafts": [], "effects": self.effects,
            "flowers": [], "green_screens": [], "handwrites": [], "hsl": [], "images": [],
            "log_color_wheels": [], "loudnesses": [], "manual_deformations": [], "masks": self.masks,
            "material_animations": self.material_animations,
            "material_colors": [], "multi_language_refs": [], "placeholders": [], "plugin_effects": [],
            "primary_color_wheels": [], "realtime_denoises": [], "shapes": [], "smart_crops": [],
            "smart_relights": [], "sound_channel_mappings": [], "speeds": self.speeds,
            "stickers": self.stickers, "tail_leaders": [], "text_templates": [], "texts": self.texts,
            "time_marks": [], "transitions": [],
            "video_effects": self.video_effects,
            "video_trackings": [], "videos": self.videos.iter().map(VideoMaterial::export_json).collect::<Vec<_>>(),
            "vocal_beautifys": [], "vocal_separations": [],
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptFile {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_us: i64,
    pub maintrack_adsorb: bool,
    pub tracks: Vec<Track>,
    /// Generated once per draft (not per export) — `base_skeleton`'s "id"
    /// field must stay stable across repeated `export_json()` calls (e.g.
    /// `CapCutAdapter::export_draft`'s dual-file write of `draft_content.json`
    /// + `draft_info.json`, which must be byte-identical).
    draft_id: String,
}

impl ScriptFile {
    pub fn new(width: u32, height: u32, fps: f64) -> Self {
        Self {
            width,
            height,
            fps,
            duration_us: 0,
            maintrack_adsorb: true,
            tracks: Vec::new(),
            draft_id: Uuid::new_v4().to_string().to_uppercase(),
        }
    }

    /// Computed fresh from live track state — see module doc comment.
    pub fn materials(&self) -> ScriptMaterial {
        ScriptMaterial::from_tracks(&self.tracks)
    }

    /// The `render_index` the next appended track should use to stack above
    /// every existing one, matching `ScriptFile.next_track_render_index` in
    /// `script_file.py`.
    pub fn next_track_render_index(&self) -> i32 {
        self.tracks
            .iter()
            .map(|t| t.render_index)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0)
    }

    /// Appends a new track, returning its generated `track_id`.
    pub fn add_track(
        &mut self,
        track_type: TrackType,
        name: impl Into<String>,
        render_index: i32,
        mute: bool,
    ) -> String {
        let track_id = Uuid::new_v4().simple().to_string();
        self.tracks.push(Track::new(
            track_id.clone(),
            track_type,
            name,
            render_index,
            mute,
        ));
        track_id
    }

    fn find_track_mut(&mut self, track_id: &str) -> Result<&mut Track, CapCutError> {
        self.tracks
            .iter_mut()
            .find(|t| t.track_id == track_id)
            .ok_or_else(|| CapCutError::TrackNotFound {
                track_name: track_id.to_string(),
            })
    }

    /// Finds an already-inserted segment by its `segment_id`, across every
    /// track — used by `capcut::adapter`'s `add_animation`/`add_keyframe`/
    /// `add_mask`, which operate on a segment `add_video`/`add_caption`/
    /// `add_sticker` already handed to a track.
    pub fn find_segment_mut(&mut self, segment_id: &str) -> Option<&mut SegmentSlot> {
        self.tracks
            .iter_mut()
            .find_map(|t| t.find_segment_mut(segment_id))
    }

    /// Adds `segment` to the track named `track_id`.
    pub fn add_segment(&mut self, track_id: &str, segment: SegmentSlot) -> Result<(), CapCutError> {
        let end_us = segment.target_timerange().end();
        let track = self.find_track_mut(track_id)?;
        track.add_segment(segment)?;
        self.duration_us = self.duration_us.max(end_us);
        Ok(())
    }

    /// Matches `ScriptFile.dumps`'s field-assembly step in `script_file.py`
    /// (this crate writes with `serde_json::to_string_pretty` at the
    /// `capcut::export`/`adapter` boundary rather than duplicating that
    /// here). Deterministic given the same `ScriptFile` state — calling this
    /// twice in a row produces byte-identical output (see `draft_id` doc
    /// comment).
    pub fn export_json(&self) -> Value {
        let mut skeleton = base_skeleton();
        if let Value::Object(root) = &mut skeleton {
            root.insert("id".into(), json!(self.draft_id));
            root.insert("fps".into(), json!(self.fps));
            root.insert("duration".into(), json!(self.duration_us));
            root.insert(
                "canvas_config".into(),
                json!({ "width": self.width, "height": self.height, "ratio": "original" }),
            );
            if let Some(Value::Object(config)) = root.get_mut("config") {
                config.insert("maintrack_adsorb".into(), json!(self.maintrack_adsorb));
            }
            root.insert("materials".into(), self.materials().export_json());

            let mut ordered_tracks = self.tracks.clone();
            ordered_tracks.sort_by_key(|t| t.render_index);
            root.insert(
                "tracks".into(),
                json!(ordered_tracks
                    .iter()
                    .map(Track::export_json)
                    .collect::<Vec<_>>()),
            );
        }
        skeleton
    }
}

/// The literal top-level draft-content skeleton, matching
/// `vendor/capcut-mate/src/pyJianYingDraft/assets/draft_content_template.json`
/// key-for-key (see module doc comment / `tests::skeleton_matches_...`).
/// The placeholder `"id"` here is always overwritten by
/// `ScriptFile::export_json` with the stable `draft_id`; it exists only so
/// `top_level_keys()` (below) sees the key present.
fn base_skeleton() -> Value {
    json!({
        "canvas_config": { "height": 1080, "ratio": "original", "width": 1920 },
        "color_space": 0,
        "config": {
            "adjust_max_index": 1, "attachment_info": [], "combination_max_index": 1,
            "export_range": null, "extract_audio_last_index": 1, "lyrics_recognition_id": "",
            "lyrics_sync": true, "lyrics_taskinfo": [], "maintrack_adsorb": true,
            "material_save_mode": 0, "multi_language_current": "none", "multi_language_list": [],
            "multi_language_main": "none", "multi_language_mode": "none",
            "original_sound_last_index": 1, "record_audio_last_index": 1, "sticker_max_index": 1,
            "subtitle_keywords_config": null, "subtitle_recognition_id": "", "subtitle_sync": true,
            "subtitle_taskinfo": [], "system_font_list": [], "video_mute": false, "zoom_info_params": null,
        },
        "cover": null,
        "create_time": 0,
        "duration": 0,
        "extra_info": null,
        "fps": 30.0,
        "free_render_index_mode_on": false,
        "group_container": null,
        "id": "",
        "keyframe_graph_list": [],
        "keyframes": {
            "adjusts": [], "audios": [], "effects": [], "filters": [], "handwrites": [],
            "stickers": [], "texts": [], "videos": [],
        },
        "last_modified_platform": { "app_id": 3704, "app_source": "lv", "app_version": "5.9.0", "os": "windows" },
        "platform": { "app_id": 3704, "app_source": "lv", "app_version": "5.9.0", "os": "windows" },
        "materials": {},
        "mutable_config": null,
        "name": "",
        "new_version": "110.0.0",
        "relationships": [],
        "render_index_track_mode_on": false,
        "retouch_cover": null,
        "source": "default",
        "static_cover_image_path": "",
        "time_marks": null,
        "tracks": [],
        "update_time": 0,
        "version": 360000,
    })
}

/// Every top-level key `base_skeleton` must carry, so `add_segment`
/// callers/tests can diff against the real vendored template without
/// re-parsing it every time. Kept in sync manually with `base_skeleton`
/// above and cross-checked against the real file in the test below.
pub fn top_level_keys() -> HashSet<String> {
    if let Value::Object(map) = base_skeleton() {
        map.keys().cloned().collect()
    } else {
        HashSet::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capcut::caption_style::TextStyle;
    use crate::capcut::clip_settings::CapCutClipSettings;
    use crate::capcut::material::VideoMaterialKind;
    use crate::capcut::segment::{AudioSegment, TextSegment, VideoSegment};
    use crate::capcut::timerange::Timerange;

    #[test]
    fn constructing_a_segment_does_not_touch_the_materials_bucket() {
        let script = ScriptFile::new(1920, 1080, 30.0);
        let material = VideoMaterial::new(
            "a.mp4",
            "a.mp4",
            5_000_000,
            1920,
            1080,
            VideoMaterialKind::Video,
        );
        // Constructing the segment alone must not reach into `script` at
        // all -- there is nothing to "not touch" it wouldn't already be true
        // trivially, so this test's real assertion is on `script` itself:
        // its computed materials must still be empty after construction.
        let _segment = VideoSegment::new(
            material,
            Timerange::new(0, 5_000_000),
            Timerange::new(0, 5_000_000),
            1.0,
            1.0,
            false,
            CapCutClipSettings::default(),
        );
        assert!(
            script.materials().videos.is_empty(),
            "materials must stay empty until add_segment"
        );
    }

    #[test]
    fn add_segment_populates_the_materials_bucket_exactly_once() {
        let mut script = ScriptFile::new(1920, 1080, 30.0);
        let track_id = script.add_track(TrackType::Video, "V1", 0, false);
        let material = VideoMaterial::new(
            "a.mp4",
            "a.mp4",
            5_000_000,
            1920,
            1080,
            VideoMaterialKind::Video,
        );
        let segment = VideoSegment::new(
            material,
            Timerange::new(0, 5_000_000),
            Timerange::new(0, 5_000_000),
            1.0,
            1.0,
            false,
            CapCutClipSettings::default(),
        );

        assert!(
            script.materials().videos.is_empty(),
            "still empty before add_segment"
        );
        script
            .add_segment(&track_id, SegmentSlot::Video(segment))
            .expect("adding to the matching-type track should succeed");
        assert_eq!(
            script.materials().videos.len(),
            1,
            "exactly one material after one add_segment"
        );
        assert_eq!(script.duration_us, 5_000_000);
    }

    #[test]
    fn adding_the_same_audio_material_twice_does_not_duplicate_it() {
        // Mirrors `ScriptFile.add_material`'s "素材已存在" dedup check.
        let mut script = ScriptFile::new(1920, 1080, 30.0);
        let track_id = script.add_track(TrackType::Audio, "A1", 0, false);
        let material = AudioMaterial::new("a.mp3", "a.mp3", 5_000_000);

        let seg1 = AudioSegment::new(
            material.clone(),
            Timerange::new(0, 2_000_000),
            Timerange::new(0, 2_000_000),
            1.0,
            1.0,
            false,
        );
        let seg2 = AudioSegment::new(
            material,
            Timerange::new(0, 2_000_000),
            Timerange::new(3_000_000, 2_000_000),
            1.0,
            1.0,
            false,
        );

        script
            .add_segment(&track_id, SegmentSlot::Audio(seg1))
            .expect("first add ok");
        script
            .add_segment(&track_id, SegmentSlot::Audio(seg2))
            .expect("second add ok, non-overlapping timing");
        assert_eq!(
            script.materials().audios.len(),
            1,
            "same material_id should not be duplicated"
        );
    }

    #[test]
    fn export_json_reflects_added_tracks_and_materials() {
        let mut script = ScriptFile::new(1920, 1080, 24.0);
        let track_id = script.add_track(TrackType::Text, "Text", 15_000, false);
        let style = TextStyle {
            size: 10.0,
            bold: false,
            italic: false,
            color: (1.0, 1.0, 1.0),
            alpha: 1.0,
            align: 1,
        };
        let seg = TextSegment::new(
            "hi",
            Timerange::new(0, 1_000_000),
            style,
            CapCutClipSettings::default(),
        );
        script
            .add_segment(&track_id, SegmentSlot::Text(seg))
            .expect("add ok");

        let v = script.export_json();
        assert_eq!(v["fps"], json!(24.0));
        assert_eq!(v["canvas_config"]["width"], json!(1920));
        assert_eq!(v["tracks"].as_array().unwrap().len(), 1);
        assert_eq!(v["materials"]["texts"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn export_json_is_stable_across_repeated_calls() {
        let mut script = ScriptFile::new(1920, 1080, 30.0);
        let track_id = script.add_track(TrackType::Video, "V1", 0, false);
        let material = VideoMaterial::new(
            "a.mp4",
            "a.mp4",
            5_000_000,
            1920,
            1080,
            VideoMaterialKind::Video,
        );
        let segment = VideoSegment::new(
            material,
            Timerange::new(0, 5_000_000),
            Timerange::new(0, 5_000_000),
            1.0,
            1.0,
            false,
            CapCutClipSettings::default(),
        );
        script
            .add_segment(&track_id, SegmentSlot::Video(segment))
            .expect("add ok");

        let first = script.export_json();
        let second = script.export_json();
        assert_eq!(
            first, second,
            "export_json must be deterministic across repeated calls (e.g. dual-file save)"
        );
    }

    #[test]
    fn skeleton_matches_every_top_level_key_in_the_reference_template() {
        let reference_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/capcut-mate/src/pyJianYingDraft/assets/draft_content_template.json");
        let reference_text = std::fs::read_to_string(&reference_path).unwrap_or_else(|e| {
            panic!("reference template must be readable at {reference_path:?}: {e}")
        });
        let reference: Value =
            serde_json::from_str(&reference_text).expect("reference template must be valid JSON");
        let reference_keys: HashSet<String> = reference
            .as_object()
            .expect("reference is a JSON object")
            .keys()
            .cloned()
            .collect();

        let ours = top_level_keys();
        let missing: Vec<&String> = reference_keys
            .iter()
            .filter(|k| !ours.contains(k.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "our skeleton is missing top-level keys the real template has: {missing:?}"
        );
    }
}
