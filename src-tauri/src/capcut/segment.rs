//! `BaseSegment`/`MediaSegment`/`VisualSegment` hierarchy and the six
//! concrete segment kinds — port of `segment.py`, `video_segment.py`,
//! `audio_segment.py`, `text_segment.py`, and `effect_segment.py`'s class
//! layout onto Rust structs (Rust has no class inheritance, so each level is
//! a composed field rather than a base class — `VisualSegmentFields` embeds
//! `MediaSegmentFields`, which embeds `BaseSegmentFields`, mirroring the
//! Python MRO field-for-field).
//!
//! **`EffectSegment`/`FilterSegment` are documented passthrough** (per this
//! phase's scope-reduction brief): with no ported effect/filter resource
//! catalog, there is no `effect_id`/`resource_id` to resolve for either —
//! they carry `project::Effect::kind`/`params` straight through as an
//! unresolved reference rather than fabricating a lookup that doesn't
//! exist. `StickerSegment` is real but exercises no data yet either (no
//! sticker concept in `project::types`) — it exists so `add_sticker`'s
//! signature is genuine, callable, working Rust, not a stub.

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::capcut::animation::SegmentAnimations;
use crate::capcut::caption_style::{TextBackground, TextBorder, TextShadow, TextStyle};
use crate::capcut::clip_settings::CapCutClipSettings;
use crate::capcut::keyframe::KeyframeList;
use crate::capcut::mask::Mask;
use crate::capcut::material::{AudioMaterial, VideoMaterial};
use crate::capcut::timerange::Timerange;

fn merge(mut base: Value, extra: Value) -> Value {
    if let (Some(b), Value::Object(e)) = (base.as_object_mut(), extra) {
        b.extend(e);
    }
    base
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaseSegmentFields {
    pub segment_id: String,
    pub material_id: String,
    pub target_timerange: Timerange,
    pub common_keyframes: Vec<KeyframeList>,
}

impl BaseSegmentFields {
    pub fn new(material_id: impl Into<String>, target_timerange: Timerange) -> Self {
        Self {
            segment_id: Uuid::new_v4().simple().to_string(),
            material_id: material_id.into(),
            target_timerange,
            common_keyframes: Vec::new(),
        }
    }

    pub fn overlaps(&self, other: &BaseSegmentFields) -> bool {
        self.target_timerange.overlaps(&other.target_timerange)
    }

    /// Matches `BaseSegment.export_json` in `segment.py`.
    pub fn export_json(&self) -> Value {
        json!({
            "enable_adjust": true,
            "enable_color_correct_adjust": false,
            "enable_color_curves": true,
            "enable_color_match_adjust": false,
            "enable_color_wheels": true,
            "enable_lut": true,
            "enable_smart_color_adjust": false,
            "last_nonzero_volume": 1.0,
            "reverse": false,
            "track_attribute": 0,
            "track_render_index": 0,
            "visible": true,
            "id": self.segment_id,
            "material_id": self.material_id,
            "target_timerange": self.target_timerange.export_json(),
            "common_keyframes": self.common_keyframes.iter().map(KeyframeList::export_json).collect::<Vec<_>>(),
            "keyframe_refs": [],
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSegmentFields {
    pub base: BaseSegmentFields,
    pub source_timerange: Option<Timerange>,
    pub speed: f64,
    pub speed_id: String,
    pub volume: f64,
    pub change_pitch: bool,
    pub extra_material_refs: Vec<String>,
}

impl MediaSegmentFields {
    pub fn new(
        material_id: impl Into<String>,
        source_timerange: Option<Timerange>,
        target_timerange: Timerange,
        speed: f64,
        volume: f64,
        change_pitch: bool,
    ) -> Self {
        let speed_id = Uuid::new_v4().simple().to_string();
        Self {
            base: BaseSegmentFields::new(material_id, target_timerange),
            source_timerange,
            speed,
            speed_id: speed_id.clone(),
            volume,
            change_pitch,
            extra_material_refs: vec![speed_id],
        }
    }

    /// Matches `MediaSegment.export_json` in `segment.py`.
    pub fn export_json(&self) -> Value {
        merge(
            self.base.export_json(),
            json!({
                "source_timerange": self.source_timerange.map(|t| t.export_json()),
                "speed": self.speed,
                "volume": self.volume,
                "extra_material_refs": self.extra_material_refs,
                "is_tone_modify": self.change_pitch,
            }),
        )
    }

    /// Matches `Speed.export_json` in `segment.py` — a standalone
    /// `materials.speeds[]` entry every media segment contributes.
    pub fn speed_material_json(&self) -> Value {
        json!({
            "curve_speed": null,
            "id": self.speed_id,
            "mode": 0,
            "speed": self.speed,
            "type": "speed",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualSegmentFields {
    pub media: MediaSegmentFields,
    pub clip_settings: CapCutClipSettings,
    pub uniform_scale: bool,
    pub animations: Option<SegmentAnimations>,
}

impl VisualSegmentFields {
    pub fn new(
        material_id: impl Into<String>,
        source_timerange: Option<Timerange>,
        target_timerange: Timerange,
        speed: f64,
        volume: f64,
        change_pitch: bool,
        clip_settings: CapCutClipSettings,
    ) -> Self {
        Self {
            media: MediaSegmentFields::new(
                material_id,
                source_timerange,
                target_timerange,
                speed,
                volume,
                change_pitch,
            ),
            clip_settings,
            uniform_scale: true,
            animations: None,
        }
    }

    /// Matches `VisualSegment.export_json` in `segment.py`.
    pub fn export_json(&self) -> Value {
        merge(
            self.media.export_json(),
            json!({
                "clip": self.clip_settings.export_json(),
                "uniform_scale": { "on": self.uniform_scale, "value": 1.0 },
            }),
        )
    }
}

// ---------------------------------------------------------------------
// VideoSegment
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct VideoSegment {
    pub visual: VisualSegmentFields,
    pub material: VideoMaterial,
    pub mask: Option<Mask>,
}

impl VideoSegment {
    pub fn new(
        material: VideoMaterial,
        source_timerange: Timerange,
        target_timerange: Timerange,
        speed: f64,
        volume: f64,
        change_pitch: bool,
        clip_settings: CapCutClipSettings,
    ) -> Self {
        Self {
            visual: VisualSegmentFields::new(
                material.material_id.clone(),
                Some(source_timerange),
                target_timerange,
                speed,
                volume,
                change_pitch,
                clip_settings,
            ),
            material,
            mask: None,
        }
    }

    pub fn target_timerange(&self) -> Timerange {
        self.visual.media.base.target_timerange
    }

    /// Matches `VideoSegment.export_json` in `video_segment.py`.
    pub fn export_json(&self) -> Value {
        merge(
            self.visual.export_json(),
            json!({ "hdr_settings": { "intensity": 1.0, "mode": 1, "nits": 1000 } }),
        )
    }
}

// ---------------------------------------------------------------------
// AudioSegment
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSegment {
    pub media: MediaSegmentFields,
    pub material: AudioMaterial,
}

impl AudioSegment {
    pub fn new(
        material: AudioMaterial,
        source_timerange: Timerange,
        target_timerange: Timerange,
        speed: f64,
        volume: f64,
        change_pitch: bool,
    ) -> Self {
        Self {
            media: MediaSegmentFields::new(
                material.material_id.clone(),
                Some(source_timerange),
                target_timerange,
                speed,
                volume,
                change_pitch,
            ),
            material,
        }
    }

    pub fn target_timerange(&self) -> Timerange {
        self.media.base.target_timerange
    }

    /// Matches `AudioSegment.export_json` in `audio_segment.py`.
    pub fn export_json(&self) -> Value {
        merge(
            self.media.export_json(),
            json!({ "clip": null, "hdr_settings": null }),
        )
    }
}

// ---------------------------------------------------------------------
// TextSegment
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TextSegment {
    pub visual: VisualSegmentFields,
    pub text: String,
    pub style: TextStyle,
    pub border: Option<TextBorder>,
    pub background: Option<TextBackground>,
    pub shadow: Option<TextShadow>,
}

impl TextSegment {
    pub fn new(
        text: impl Into<String>,
        timerange: Timerange,
        style: TextStyle,
        clip_settings: CapCutClipSettings,
    ) -> Self {
        let material_id = Uuid::new_v4().simple().to_string();
        let mut visual =
            VisualSegmentFields::new(material_id, None, timerange, 1.0, 1.0, false, clip_settings);
        // pyJianYingDraft always attaches a (possibly empty) SegmentAnimations
        // to text segments, referenced via extra_material_refs — mirrored
        // here rather than leaving `animations` at `None` for text.
        let animations = SegmentAnimations::new();
        visual.media.extra_material_refs = vec![animations.animation_id.clone()];
        visual.animations = Some(animations);
        Self {
            visual,
            text: text.into(),
            style,
            border: None,
            background: None,
            shadow: None,
        }
    }

    pub fn target_timerange(&self) -> Timerange {
        self.visual.media.base.target_timerange
    }

    pub fn material_id(&self) -> &str {
        &self.visual.media.base.material_id
    }

    /// Matches `TextSegment.export_json` (the *segment*, not the material)
    /// in `text_segment.py`.
    pub fn export_json(&self) -> Value {
        merge(
            self.visual.export_json(),
            json!({
                "caption_info": null,
                "cartoon": false,
                "group_id": "",
                "hdr_settings": null,
                "is_placeholder": false,
                "template_id": "",
                "template_scene": "default",
            }),
        )
    }

    /// Matches `TextSegment.export_material` in `text_segment.py` — the
    /// `materials.texts[]` entry. Ported with the structurally load-bearing
    /// fields (text/content/styles, size/color/alignment, border/background/
    /// shadow numerics, font-size/global-alpha) rather than the Python
    /// original's full ~80-key dict; several purely cosmetic bookkeeping
    /// fields the real CapCut UI uses for its own editing affordances
    /// (`preset_*`, `combo_info`, `subtitle_keywords`, ...) are omitted as a
    /// documented gap rather than guessed at, since this pass has no way to
    /// verify them against a real install either way.
    pub fn export_material(&self) -> Value {
        let content = json!({
            "styles": [{
                "fill": { "content": { "solid": { "color": [self.style.color.0, self.style.color.1, self.style.color.2] } } },
                "range": [0, self.text.chars().count()],
                "size": self.style.size,
            }],
            "text": self.text,
        });

        let mut ret = Map::new();
        ret.insert("id".into(), json!(self.material_id()));
        ret.insert("type".into(), json!("text"));
        ret.insert("content".into(), json!(content.to_string()));
        ret.insert("text_color".into(), json!(rgb_to_hex(self.style.color)));
        ret.insert("text_alpha".into(), json!(self.style.alpha));
        ret.insert("global_alpha".into(), json!(self.style.alpha));
        ret.insert("font_size".into(), json!(self.style.size));
        ret.insert("alignment".into(), json!(self.style.align));
        ret.insert("bold".into(), json!(self.style.bold));
        ret.insert("italic".into(), json!(self.style.italic));

        if let Some(border) = &self.border {
            ret.insert("border_alpha".into(), json!(border.alpha));
            ret.insert("border_color".into(), json!(rgb_to_hex(border.color)));
            ret.insert("border_width".into(), json!(border.width));
        }
        if let Some(bg) = &self.background {
            if let Value::Object(bg_fields) = bg.export_json() {
                ret.extend(bg_fields);
            }
        }
        if let Some(shadow) = &self.shadow {
            ret.insert("has_shadow".into(), json!(true));
            ret.insert("shadow_alpha".into(), json!(shadow.alpha));
            ret.insert("shadow_angle".into(), json!(shadow.angle));
            ret.insert("shadow_color".into(), json!(rgb_to_hex(shadow.color)));
            ret.insert("shadow_distance".into(), json!(shadow.distance));
            ret.insert(
                "shadow_smoothing".into(),
                json!(shadow.diffuse / 100.0 * 3.0),
            );
        } else {
            ret.insert("has_shadow".into(), json!(false));
        }

        Value::Object(ret)
    }
}

fn rgb_to_hex(c: (f32, f32, f32)) -> String {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", to_u8(c.0), to_u8(c.1), to_u8(c.2))
}

// ---------------------------------------------------------------------
// StickerSegment
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct StickerSegment {
    pub visual: VisualSegmentFields,
    pub resource_id: String,
}

impl StickerSegment {
    pub fn new(
        resource_id: impl Into<String>,
        timerange: Timerange,
        clip_settings: CapCutClipSettings,
    ) -> Self {
        let material_id = Uuid::new_v4().simple().to_string();
        Self {
            visual: VisualSegmentFields::new(
                material_id,
                None,
                timerange,
                1.0,
                1.0,
                false,
                clip_settings,
            ),
            resource_id: resource_id.into(),
        }
    }

    pub fn target_timerange(&self) -> Timerange {
        self.visual.media.base.target_timerange
    }

    pub fn export_json(&self) -> Value {
        self.visual.export_json()
    }

    /// Matches `StickerSegment.export_material` in `video_segment.py`.
    pub fn export_material(&self) -> Value {
        json!({
            "id": self.visual.media.base.material_id,
            "resource_id": self.resource_id,
            "sticker_id": self.resource_id,
            "source_platform": 1,
            "type": "sticker",
        })
    }
}

// ---------------------------------------------------------------------
// EffectSegment / FilterSegment — documented passthrough (see module doc).
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct EffectSegment {
    pub segment_id: String,
    pub material_id: String,
    pub target_timerange: Timerange,
    /// `project::Effect::kind`, passed through unresolved.
    pub kind: String,
    /// `project::Effect::params`, passed through unresolved.
    pub params: Value,
}

impl EffectSegment {
    pub fn new(kind: impl Into<String>, params: Value, timerange: Timerange) -> Self {
        Self {
            segment_id: Uuid::new_v4().simple().to_string(),
            material_id: Uuid::new_v4().simple().to_string(),
            target_timerange: timerange,
            kind: kind.into(),
            params,
        }
    }

    pub fn export_json(&self) -> Value {
        json!({
            "id": self.segment_id,
            "material_id": self.material_id,
            "target_timerange": self.target_timerange.export_json(),
            "common_keyframes": [],
            "extra_material_refs": [self.material_id],
        })
    }

    /// Unresolved passthrough material entry — no effect-resource catalog
    /// is ported this pass (see module doc comment).
    pub fn material_json(&self) -> Value {
        json!({
            "id": self.material_id,
            "type": "video_effect",
            "unresolved_kind": self.kind,
            "params": self.params,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterSegment {
    pub segment_id: String,
    pub material_id: String,
    pub target_timerange: Timerange,
    pub kind: String,
    pub params: Value,
}

impl FilterSegment {
    pub fn new(kind: impl Into<String>, params: Value, timerange: Timerange) -> Self {
        Self {
            segment_id: Uuid::new_v4().simple().to_string(),
            material_id: Uuid::new_v4().simple().to_string(),
            target_timerange: timerange,
            kind: kind.into(),
            params,
        }
    }

    pub fn export_json(&self) -> Value {
        json!({
            "id": self.segment_id,
            "material_id": self.material_id,
            "target_timerange": self.target_timerange.export_json(),
            "common_keyframes": [],
            "extra_material_refs": [self.material_id],
        })
    }

    pub fn material_json(&self) -> Value {
        json!({
            "id": self.material_id,
            "type": "filter",
            "unresolved_kind": self.kind,
            "params": self.params,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timerange(start: i64, dur: i64) -> Timerange {
        Timerange::new(start, dur)
    }

    #[test]
    fn video_segment_export_json_includes_target_timerange_and_hdr() {
        let material = VideoMaterial::new(
            "a.mp4",
            "a.mp4",
            10_000_000,
            1920,
            1080,
            crate::capcut::material::VideoMaterialKind::Video,
        );
        let seg = VideoSegment::new(
            material,
            timerange(0, 5_000_000),
            timerange(0, 5_000_000),
            1.0,
            1.0,
            false,
            CapCutClipSettings::default(),
        );
        let v = seg.export_json();
        assert_eq!(v["target_timerange"]["duration"], json!(5_000_000));
        assert!(v.get("hdr_settings").is_some());
    }

    #[test]
    fn audio_segment_export_json_has_null_clip() {
        let material = AudioMaterial::new("a.mp3", "a.mp3", 3_000_000);
        let seg = AudioSegment::new(
            material,
            timerange(0, 3_000_000),
            timerange(0, 3_000_000),
            1.0,
            1.0,
            false,
        );
        let v = seg.export_json();
        assert_eq!(v["clip"], Value::Null);
    }

    #[test]
    fn text_segment_material_json_includes_text_and_color() {
        let style = TextStyle {
            size: 24.0,
            bold: false,
            italic: false,
            color: (1.0, 1.0, 1.0),
            alpha: 1.0,
            align: 1,
        };
        let seg = TextSegment::new(
            "Hello",
            timerange(0, 1_000_000),
            style,
            CapCutClipSettings::default(),
        );
        let mat = seg.export_material();
        assert_eq!(mat["type"], json!("text"));
        assert_eq!(mat["text_color"], json!("#FFFFFF"));
        let content: Value = serde_json::from_str(mat["content"].as_str().unwrap()).unwrap();
        assert_eq!(content["text"], json!("Hello"));
    }

    #[test]
    fn effect_segment_passes_kind_and_params_through_unresolved() {
        let seg = EffectSegment::new("blur", json!({"radius": 5}), timerange(0, 1_000_000));
        let mat = seg.material_json();
        assert_eq!(mat["unresolved_kind"], json!("blur"));
        assert_eq!(mat["params"]["radius"], json!(5));
    }

    #[test]
    fn media_segment_fields_seed_extra_refs_with_speed_id() {
        let media = MediaSegmentFields::new(
            "mat1",
            Some(timerange(0, 1_000_000)),
            timerange(0, 1_000_000),
            1.0,
            1.0,
            false,
        );
        assert_eq!(media.extra_material_refs, vec![media.speed_id.clone()]);
    }
}
