//! `CapCutAdapter` — the exact internal function surface master prompt §29
//! calls for: `create_draft`, `add_video`, `add_audio`, `add_image`,
//! `add_caption`, `add_sticker`, `add_effect`, `add_mask`, `add_animation`,
//! `add_keyframe`, `save_draft`, `export_draft`. Plain Rust functions/methods
//! the app core calls directly — never an HTTP layer, per that section.
//!
//! `add_video`/`add_audio`/`add_image`/`add_caption`/`add_sticker`/
//! `add_effect` each return the new segment's generated `segment_id`;
//! `add_animation`/`add_keyframe`/`add_mask` take that id back to mutate the
//! already-inserted segment in place (CapCut/pyJianYingDraft's own idiom is
//! a fluent `segment.add_animation(...).add_mask(...)` *before*
//! `ScriptFile.add_segment` — this adapter instead allows attaching those
//! after insertion, which is equally correct since nothing about a
//! segment's mask/keyframes/animations affects the overlap check
//! `ScriptFile.add_segment`/`Track.add_segment` perform, and it keeps this
//! flat function-call surface matching the master prompt's list instead of
//! a builder pattern).

use std::path::Path;

use serde_json::Value;

use crate::capcut::animation::{AnimationType, SegmentAnimation, SegmentAnimations};
use crate::capcut::caption_style::{
    background_from_caption_background, border_from_outline, clip_settings_from_caption_position,
    shadow_from_caption_shadow, text_style_from_caption_style, TextStyle,
};
use crate::capcut::clip_settings::CapCutClipSettings;
use crate::capcut::error::CapCutError;
use crate::capcut::keyframe::KeyframeProperty;
use crate::capcut::mask::{compute_mask_width, Mask, MaskType};
use crate::capcut::material::{AudioMaterial, VideoMaterial, VideoMaterialKind};
use crate::capcut::script::ScriptFile;
use crate::capcut::segment::{
    AudioSegment, EffectSegment, StickerSegment, TextSegment, VideoSegment,
};
use crate::capcut::timerange::Timerange;
use crate::capcut::track::{SegmentSlot, TrackType};
use crate::project::{AnimationKind, Caption, CaptionStyle};

#[derive(Debug)]
pub struct CapCutAdapter {
    pub script: ScriptFile,
}

impl CapCutAdapter {
    /// `create_draft`: starts a new in-memory draft with the given canvas.
    pub fn create_draft(width: u32, height: u32, fps: f64) -> Self {
        Self {
            script: ScriptFile::new(width, height, fps),
        }
    }

    /// `add_video`: inserts a video/photo `VideoSegment` built from
    /// `material` onto `track_id`. Returns the new segment's id.
    #[allow(clippy::too_many_arguments)]
    pub fn add_video(
        &mut self,
        track_id: &str,
        material: VideoMaterial,
        source_timerange: Timerange,
        target_timerange: Timerange,
        speed: f64,
        volume: f64,
        clip_settings: CapCutClipSettings,
    ) -> Result<String, CapCutError> {
        let segment = VideoSegment::new(
            material,
            source_timerange,
            target_timerange,
            speed,
            volume,
            false,
            clip_settings,
        );
        let segment_id = segment.visual.media.base.segment_id.clone();
        self.script
            .add_segment(track_id, SegmentSlot::Video(segment))?;
        Ok(segment_id)
    }

    /// `add_image`: like `add_video`, but for a `VideoMaterialKind::Photo`
    /// material — CapCut represents images as `VideoSegment`s too
    /// (`video_segment.py` has no separate image-segment class), sourced
    /// from the start of the (effectively unbounded-duration) photo
    /// material for the target range's full length, matching
    /// `VideoSegment.__init__`'s own `speed=1.0`/no-`source_timerange`
    /// default path.
    pub fn add_image(
        &mut self,
        track_id: &str,
        material: VideoMaterial,
        target_timerange: Timerange,
        clip_settings: CapCutClipSettings,
    ) -> Result<String, CapCutError> {
        debug_assert_eq!(
            material.kind,
            VideoMaterialKind::Photo,
            "add_image expects a Photo-kind material"
        );
        let source_timerange = Timerange::new(0, target_timerange.duration);
        self.add_video(
            track_id,
            material,
            source_timerange,
            target_timerange,
            1.0,
            1.0,
            clip_settings,
        )
    }

    /// `add_audio`: inserts an `AudioSegment` built from `material` onto
    /// `track_id`. Returns the new segment's id.
    pub fn add_audio(
        &mut self,
        track_id: &str,
        material: AudioMaterial,
        source_timerange: Timerange,
        target_timerange: Timerange,
        speed: f64,
        volume: f64,
    ) -> Result<String, CapCutError> {
        let segment = AudioSegment::new(
            material,
            source_timerange,
            target_timerange,
            speed,
            volume,
            false,
        );
        let segment_id = segment.media.base.segment_id.clone();
        self.script
            .add_segment(track_id, SegmentSlot::Audio(segment))?;
        Ok(segment_id)
    }

    /// `add_caption`: the rich mapping function — converts a
    /// `project::Caption` plus its resolved `project::CaptionStyle` (`None`
    /// falls back to CapCut's own `TextStyle` defaults, matching
    /// `TextStyle.__init__`'s defaults) into a real CapCut `TextSegment` and
    /// inserts it onto `track_id`. See `capcut::caption_style`'s module doc
    /// comment for every field-mapping decision (color/outline-width/
    /// shadow-distance-angle/background/position). Returns the new
    /// segment's id.
    pub fn add_caption(
        &mut self,
        track_id: &str,
        caption: &Caption,
        style: Option<&CaptionStyle>,
    ) -> Result<String, CapCutError> {
        let text_style = style
            .map(text_style_from_caption_style)
            .unwrap_or(TextStyle {
                size: 8.0,
                bold: false,
                italic: false,
                color: (1.0, 1.0, 1.0),
                alpha: 1.0,
                align: 0,
            });
        let clip_settings = style
            .map(|s| clip_settings_from_caption_position(&s.position))
            .unwrap_or_default();

        let duration_us = (caption.end_us - caption.start_us).max(0);
        let mut segment = TextSegment::new(
            caption.text.clone(),
            Timerange::new(caption.start_us, duration_us),
            text_style,
            clip_settings,
        );

        if let Some(style) = style {
            segment.border = style.outline.as_ref().map(border_from_outline);
            segment.background = style
                .background
                .as_ref()
                .map(background_from_caption_background);
            segment.shadow = style.shadow.as_ref().map(shadow_from_caption_shadow);
        }

        let segment_id = segment.visual.media.base.segment_id.clone();
        self.script
            .add_segment(track_id, SegmentSlot::Text(segment))?;
        Ok(segment_id)
    }

    /// `add_sticker`: inserts a `StickerSegment` referencing `resource_id`
    /// (an opaque CapCut resource id — no sticker catalog is ported this
    /// pass, see `crate::capcut::segment`'s module doc comment). Returns the
    /// new segment's id.
    pub fn add_sticker(
        &mut self,
        track_id: &str,
        resource_id: &str,
        timerange: Timerange,
        clip_settings: CapCutClipSettings,
    ) -> Result<String, CapCutError> {
        let segment = StickerSegment::new(resource_id, timerange, clip_settings);
        let segment_id = segment.visual.media.base.segment_id.clone();
        self.script
            .add_segment(track_id, SegmentSlot::Sticker(segment))?;
        Ok(segment_id)
    }

    /// `add_effect`: inserts an `EffectSegment` carrying `kind`/`params`
    /// straight through unresolved (`project::Effect`'s own documented
    /// "opaque JSON, no catalog exists yet" shape — see
    /// `crate::capcut::segment`'s module doc comment). Returns the new
    /// segment's id.
    pub fn add_effect(
        &mut self,
        track_id: &str,
        kind: &str,
        params: Value,
        timerange: Timerange,
    ) -> Result<String, CapCutError> {
        let segment = EffectSegment::new(kind, params, timerange);
        let segment_id = segment.segment_id.clone();
        self.script
            .add_segment(track_id, SegmentSlot::Effect(segment))?;
        Ok(segment_id)
    }

    /// `add_mask`: attaches a `Mask` to an already-inserted `VideoSegment`
    /// (masks only ever apply to `VideoSegment` in CapCut's own model — see
    /// `video_segment.py`'s `VideoSegment.add_mask`). `size`/`feather`/
    /// `round_corner` use the same `0..=100` UI scale `video_segment.py`'s
    /// own `add_mask` takes (converted to `0.0..=1.0` here, matching that
    /// function's own `feather/100`/`round_corner/100`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_mask(
        &mut self,
        segment_id: &str,
        mask_type: MaskType,
        center_x: f64,
        center_y: f64,
        size: f64,
        rotation: f64,
        feather: f64,
        invert: bool,
        rect_width: Option<f64>,
        round_corner: Option<f64>,
    ) -> Result<(), CapCutError> {
        let slot = self.script.find_segment_mut(segment_id).ok_or_else(|| {
            CapCutError::SegmentNotFound {
                segment_id: segment_id.to_string(),
            }
        })?;
        let SegmentSlot::Video(video) = slot else {
            return Err(CapCutError::SegmentKindMismatch {
                segment_id: segment_id.to_string(),
                expected_kind: "video".to_string(),
            });
        };
        let width = compute_mask_width(
            rect_width,
            size,
            mask_type,
            video.material.width,
            video.material.height,
        );
        video.mask = Some(Mask {
            global_id: uuid::Uuid::new_v4().simple().to_string(),
            mask_type,
            center_x,
            center_y,
            width,
            height: size,
            aspect_ratio: mask_type.meta().default_aspect_ratio,
            rotation,
            invert,
            feather: feather / 100.0,
            round_corner: round_corner.unwrap_or(0.0) / 100.0,
        });
        Ok(())
    }

    /// `add_animation`: attaches an in/out/loop/group animation to an
    /// already-inserted `VideoSegment`/`TextSegment`/`StickerSegment` (every
    /// visual segment kind carries an optional `SegmentAnimations`).
    /// `is_video_animation` should be `true` for a `Video`/`Image`/`Overlay`-
    /// track segment and `false` for a `Caption`/sticker-track one, matching
    /// `animation.py`'s `Animation.is_video_animation` split.
    #[allow(clippy::too_many_arguments)]
    pub fn add_animation(
        &mut self,
        segment_id: &str,
        kind: AnimationKind,
        name: &str,
        start_us: i64,
        duration_us: i64,
        is_video_animation: bool,
    ) -> Result<(), CapCutError> {
        let slot = self.script.find_segment_mut(segment_id).ok_or_else(|| {
            CapCutError::SegmentNotFound {
                segment_id: segment_id.to_string(),
            }
        })?;
        let animations = match slot {
            SegmentSlot::Video(v) => &mut v.visual.animations,
            SegmentSlot::Text(t) => &mut t.visual.animations,
            SegmentSlot::Sticker(s) => &mut s.visual.animations,
            _ => {
                return Err(CapCutError::SegmentKindMismatch {
                    segment_id: segment_id.to_string(),
                    expected_kind: "video, text, or sticker".to_string(),
                })
            }
        };
        let animations = animations.get_or_insert_with(SegmentAnimations::new);
        animations.animations.push(SegmentAnimation {
            effect_id: uuid::Uuid::new_v4().simple().to_string(),
            name: name.to_string(),
            resource_id: String::new(),
            animation_type: AnimationType::from(kind),
            start_us,
            duration_us,
            is_video_animation,
        });
        Ok(())
    }

    /// `add_keyframe`: attaches a keyframe for `property` to an
    /// already-inserted `VideoSegment`/`AudioSegment`/`TextSegment`.
    /// `time_offset_us` should already be relative to the segment's own
    /// start (see `capcut::keyframe`'s module doc comment for converting
    /// from `project::Keyframe`'s absolute timing).
    pub fn add_keyframe(
        &mut self,
        segment_id: &str,
        property: KeyframeProperty,
        time_offset_us: i64,
        value: f64,
    ) -> Result<(), CapCutError> {
        let slot = self.script.find_segment_mut(segment_id).ok_or_else(|| {
            CapCutError::SegmentNotFound {
                segment_id: segment_id.to_string(),
            }
        })?;
        let common_keyframes = match slot {
            SegmentSlot::Video(v) => &mut v.visual.media.base.common_keyframes,
            SegmentSlot::Audio(a) => &mut a.media.base.common_keyframes,
            SegmentSlot::Text(t) => &mut t.visual.media.base.common_keyframes,
            _ => {
                return Err(CapCutError::SegmentKindMismatch {
                    segment_id: segment_id.to_string(),
                    expected_kind: "video, audio, or text".to_string(),
                })
            }
        };
        if let Some(list) = common_keyframes.iter_mut().find(|l| l.property == property) {
            list.add_keyframe(time_offset_us, value);
        } else {
            let mut list = crate::capcut::keyframe::KeyframeList::new(property);
            list.add_keyframe(time_offset_us, value);
            common_keyframes.push(list);
        }
        Ok(())
    }

    /// `save_draft`: writes the current draft-content JSON to `file_path`.
    pub fn save_draft(&self, file_path: &Path) -> Result<(), CapCutError> {
        let json = serde_json::to_string_pretty(&self.script.export_json())
            .expect("Value serialization cannot fail");
        std::fs::write(file_path, json).map_err(|e| CapCutError::WriteFailed {
            path: file_path.to_string_lossy().to_string(),
            details: e.to_string(),
        })
    }

    /// `export_draft`: creates `draft_dir` if needed and writes
    /// `draft_content.json` + `draft_info.json` (dual-file compatibility,
    /// matching `ScriptFile.save`'s default `dual_file_compatibility=True`
    /// in `script_file.py`), **and** `draft_meta_info.json` plus this
    /// draft's own entry in the shared `root_meta_info.json` registry
    /// (`capcut::meta` module doc comment) — real, first-time validation
    /// against an actual installed CapCut Pro (v9.3.0.3970) proved that
    /// without both of those, CapCut's own Projects-list UI never discovers
    /// the draft at all, even though `draft_content.json` alone is fully
    /// schema-valid and loads correctly once opened directly. `draft_root`
    /// (`draft_dir`'s parent — the shared `com.lveditor.draft` folder) and
    /// `draft_name` (`draft_dir`'s own leaf folder name) are derived from
    /// `draft_dir` itself, matching real CapCut's own "a draft's name is its
    /// folder's name" convention.
    pub fn export_draft(&self, draft_dir: &Path) -> Result<(), CapCutError> {
        std::fs::create_dir_all(draft_dir).map_err(|e| CapCutError::WriteFailed {
            path: draft_dir.to_string_lossy().to_string(),
            details: e.to_string(),
        })?;
        self.save_draft(&draft_dir.join("draft_content.json"))?;
        self.save_draft(&draft_dir.join("draft_info.json"))?;

        let draft_root = draft_dir.parent().unwrap_or(draft_dir);
        let draft_name = draft_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        let draft_id = uuid::Uuid::new_v4().to_string().to_uppercase();
        crate::capcut::meta::write_draft_meta_info(
            draft_dir,
            draft_root,
            &draft_id,
            &draft_name,
            self.script.duration_us,
        )?;
        crate::capcut::meta::register_draft_in_root_registry(
            draft_root,
            &draft_id,
            &draft_name,
            draft_dir,
        )?;
        Ok(())
    }

    /// Convenience wrapper `add_video`/`add_audio`/`add_caption`/`add_effect`
    /// callers use instead of hand-rolling `ScriptFile::add_track`, so
    /// `capcut::export`'s graph-walk doesn't need to reach into `self.script`
    /// directly.
    pub fn add_track(
        &mut self,
        track_type: TrackType,
        name: impl Into<String>,
        render_index: i32,
    ) -> String {
        self.script.add_track(track_type, name, render_index, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capcut::material::VideoMaterialKind;
    use crate::project::{
        CaptionAlignment, CaptionAnchor, CaptionOutline, CaptionPosition, CaptionShadow, Color,
        SafeMargins,
    };

    fn sample_video_material() -> VideoMaterial {
        VideoMaterial::new(
            "C:/media/a.mp4",
            "a.mp4",
            10_000_000,
            1920,
            1080,
            VideoMaterialKind::Video,
        )
    }

    #[test]
    fn create_draft_add_video_and_save_draft_round_trip() {
        let mut adapter = CapCutAdapter::create_draft(1920, 1080, 30.0);
        let track_id = adapter.add_track(TrackType::Video, "V1", 0);
        let segment_id = adapter
            .add_video(
                &track_id,
                sample_video_material(),
                Timerange::new(0, 5_000_000),
                Timerange::new(0, 5_000_000),
                1.0,
                1.0,
                CapCutClipSettings::default(),
            )
            .expect("add_video should succeed");
        assert!(!segment_id.is_empty());

        let dir = std::env::temp_dir().join(format!("capcut_adapter_test_{}", std::process::id()));
        adapter
            .export_draft(&dir)
            .expect("export_draft should succeed");
        let content = std::fs::read_to_string(dir.join("draft_content.json"))
            .expect("draft_content.json should exist");
        assert!(content.contains("\"tracks\""));
        let info = std::fs::read_to_string(dir.join("draft_info.json"))
            .expect("draft_info.json should exist (dual-file compat)");
        assert_eq!(content, info);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_caption_maps_style_and_returns_a_usable_segment_id() {
        let mut adapter = CapCutAdapter::create_draft(1080, 1920, 30.0);
        let track_id = adapter.add_track(TrackType::Text, "Captions", 15_000);

        let style = CaptionStyle {
            id: "s1".into(),
            name: "Bold Bottom".into(),
            font_family: "Arial".into(),
            font_size: 36.0,
            bold: true,
            italic: false,
            alignment: CaptionAlignment::Center,
            position: CaptionPosition {
                anchor: CaptionAnchor::Bottom,
                offset_x: 0.0,
                offset_y: 0.0,
            },
            text_color: Color::WHITE,
            background: None,
            outline: Some(CaptionOutline {
                color: Color::BLACK,
                width: 0.08,
            }),
            shadow: Some(CaptionShadow {
                color: Color::BLACK,
                opacity: 0.9,
                offset_x: 0.0,
                offset_y: 0.05,
                blur: 15.0,
            }),
            opacity: 1.0,
            safe_margins: SafeMargins::default(),
        };

        let caption = Caption {
            id: "cap1".into(),
            track_id: "cap_track".into(),
            start_us: 1_000_000,
            end_us: 3_000_000,
            text: "Hello world".into(),
            words: vec![],
            style_id: Some("s1".into()),
        };

        let segment_id = adapter
            .add_caption(&track_id, &caption, Some(&style))
            .expect("add_caption should succeed");

        // Round-trip through the exported JSON to confirm the mapped style
        // actually made it into `materials.texts[]`.
        let exported = adapter.script.export_json();
        let texts = exported["materials"]["texts"]
            .as_array()
            .expect("texts array");
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0]["border_width"], serde_json::json!(0.08));
        assert!(texts[0]["has_shadow"].as_bool().unwrap());

        // The segment itself is findable and mutable via its returned id,
        // e.g. by a subsequent add_animation/add_keyframe call.
        assert!(adapter.script.find_segment_mut(&segment_id).is_some());
    }

    #[test]
    fn add_mask_attaches_a_real_mask_to_a_video_segment() {
        let mut adapter = CapCutAdapter::create_draft(1920, 1080, 30.0);
        let track_id = adapter.add_track(TrackType::Video, "V1", 0);
        let segment_id = adapter
            .add_video(
                &track_id,
                sample_video_material(),
                Timerange::new(0, 5_000_000),
                Timerange::new(0, 5_000_000),
                1.0,
                1.0,
                CapCutClipSettings::default(),
            )
            .expect("add_video");

        adapter
            .add_mask(
                &segment_id,
                MaskType::Circle,
                0.0,
                0.0,
                0.5,
                0.0,
                0.0,
                false,
                None,
                None,
            )
            .expect("add_mask should succeed against a video segment");

        let exported = adapter.script.export_json();
        assert_eq!(exported["materials"]["masks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn add_mask_against_a_non_video_segment_is_a_kind_mismatch() {
        let mut adapter = CapCutAdapter::create_draft(1920, 1080, 30.0);
        let track_id = adapter.add_track(TrackType::Text, "Text", 15_000);
        let style = TextStyle {
            size: 10.0,
            bold: false,
            italic: false,
            color: (1.0, 1.0, 1.0),
            alpha: 1.0,
            align: 1,
        };
        let segment = crate::capcut::segment::TextSegment::new(
            "x",
            Timerange::new(0, 1_000_000),
            style,
            CapCutClipSettings::default(),
        );
        let segment_id = segment.visual.media.base.segment_id.clone();
        adapter
            .script
            .add_segment(&track_id, SegmentSlot::Text(segment))
            .unwrap();

        let err = adapter
            .add_mask(
                &segment_id,
                MaskType::Circle,
                0.0,
                0.0,
                0.5,
                0.0,
                0.0,
                false,
                None,
                None,
            )
            .unwrap_err();
        assert!(matches!(err, CapCutError::SegmentKindMismatch { .. }));
    }

    #[test]
    fn add_animation_and_add_keyframe_mutate_the_inserted_segment() {
        let mut adapter = CapCutAdapter::create_draft(1920, 1080, 30.0);
        let track_id = adapter.add_track(TrackType::Video, "V1", 0);
        let segment_id = adapter
            .add_video(
                &track_id,
                sample_video_material(),
                Timerange::new(0, 5_000_000),
                Timerange::new(0, 5_000_000),
                1.0,
                1.0,
                CapCutClipSettings::default(),
            )
            .expect("add_video");

        adapter
            .add_animation(&segment_id, AnimationKind::In, "Fade In", 0, 500_000, true)
            .expect("add_animation should succeed");
        adapter
            .add_keyframe(&segment_id, KeyframeProperty::Alpha, 100_000, 0.5)
            .expect("add_keyframe should succeed");

        let exported = adapter.script.export_json();
        assert_eq!(
            exported["materials"]["material_animations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let track = exported["tracks"].as_array().unwrap().first().unwrap();
        let segment = &track["segments"][0];
        assert_eq!(segment["common_keyframes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unknown_segment_id_is_reported_not_panicked_on() {
        let mut adapter = CapCutAdapter::create_draft(1920, 1080, 30.0);
        let err = adapter
            .add_animation("does-not-exist", AnimationKind::In, "x", 0, 0, true)
            .unwrap_err();
        assert!(matches!(err, CapCutError::SegmentNotFound { .. }));
    }
}
