//! `Keyframe`/`KeyframeList`/`KeyframeProperty` — port of `keyframe.py`.
//!
//! **The absolute-µs -> relative conversion** (`IMPLEMENTATION_PLAN.md`
//! Phase 9 tests list): `keyframe.py`'s `Keyframe.time_offset` is defined as
//! an offset *relative to the segment's own start* ("相对于素材起始点的时间
//! 偏移量"). This crate's `project::Keyframe::time_offset_us`, by contrast,
//! is documented nowhere as relative-to-anything in particular, but every
//! other `_us` field in `project::types` (`Clip::position_us`,
//! `Caption::start_us`, ...) is absolute project-timeline microseconds — so
//! for consistency with that established convention, this adapter treats
//! `Keyframe::time_offset_us` as **absolute** project time and converts it
//! to CapCut's clip-relative offset here, at this module's boundary, via
//! plain subtraction of the owning clip's `position_us`
//! (`absolute_to_relative_offset_us` below). A clip's `position_us` is
//! already its *on-timeline* (post-speed) start, matching what
//! `target_timerange.start` means for a CapCut segment, so no additional
//! speed-rescaling is needed on top of the subtraction.
//!
//! On the *value* side, no unit conversion is needed at all: `position_x`/
//! `position_y` keyframe values are meant to be expressed in the same
//! half-canvas-fraction unit `ClipSettings::transform_x`/`transform_y`
//! already use (`keyframe.py`'s own doc comment for `KeyframeProperty.position_x`
//! confirms this: "此处的数值应该为...单位是半个画布宽" — half a canvas
//! width), and `rotation`/`scale`/`alpha`/`volume` all already match their
//! `project::Keyframe`/`ClipSettings` counterparts directly. This is the
//! "relative-fraction" half of the task brief's "absolute-µs <->
//! relative-fraction conversion" phrase — it requires no code, only this
//! documented confirmation that the two schemas already agree on it.

use uuid::Uuid;

use crate::project::Keyframe as ProjectKeyframe;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyframeProperty {
    PositionX,
    PositionY,
    Rotation,
    ScaleX,
    ScaleY,
    UniformScale,
    Alpha,
    Saturation,
    Contrast,
    Brightness,
    Volume,
}

impl KeyframeProperty {
    /// Matches `KeyframeProperty`'s enum *values* in `keyframe.py` (the
    /// wire string CapCut itself expects for `property_type`).
    pub fn wire_value(self) -> &'static str {
        match self {
            KeyframeProperty::PositionX => "KFTypePositionX",
            KeyframeProperty::PositionY => "KFTypePositionY",
            KeyframeProperty::Rotation => "KFTypeRotation",
            KeyframeProperty::ScaleX => "KFTypeScaleX",
            KeyframeProperty::ScaleY => "KFTypeScaleY",
            KeyframeProperty::UniformScale => "UNIFORM_SCALE",
            KeyframeProperty::Alpha => "KFTypeAlpha",
            KeyframeProperty::Saturation => "KFTypeSaturation",
            KeyframeProperty::Contrast => "KFTypeContrast",
            KeyframeProperty::Brightness => "KFTypeBrightness",
            KeyframeProperty::Volume => "KFTypeVolume",
        }
    }

    /// Maps `project::Keyframe::property`'s open `String` catalog
    /// (`"position_x" | "position_y" | "rotation" | "scale" | "alpha" |
    /// "volume"`, per that field's doc comment) onto the six properties this
    /// pass actually supports. `"scale"` maps to `UniformScale` (this
    /// project's schema doesn't distinguish per-axis scale keyframes, unlike
    /// pyJianYingDraft's mutually-exclusive `scale_x`/`scale_y`/
    /// `uniform_scale` trio) — an honest simplification, not a silent gap:
    /// per-axis scale keyframing simply isn't a concept `project::Keyframe`
    /// exposes today. Returns `None` for any property name outside that
    /// documented set (a caller should skip, not fabricate, an unrecognized
    /// property).
    pub fn from_project_property(name: &str) -> Option<Self> {
        match name {
            "position_x" => Some(KeyframeProperty::PositionX),
            "position_y" => Some(KeyframeProperty::PositionY),
            "rotation" => Some(KeyframeProperty::Rotation),
            "scale" => Some(KeyframeProperty::UniformScale),
            "alpha" => Some(KeyframeProperty::Alpha),
            "volume" => Some(KeyframeProperty::Volume),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    pub kf_id: String,
    /// Relative to the owning segment's start, microseconds.
    pub time_offset_us: i64,
    pub value: f64,
}

impl Keyframe {
    pub fn new(time_offset_us: i64, value: f64) -> Self {
        Self {
            kf_id: Uuid::new_v4().simple().to_string(),
            time_offset_us,
            value,
        }
    }

    /// Matches `Keyframe.export_json` in `keyframe.py`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "curveType": "Line",
            "graphID": "",
            "left_control": { "x": 0.0, "y": 0.0 },
            "right_control": { "x": 0.0, "y": 0.0 },
            "id": self.kf_id,
            "time_offset": self.time_offset_us,
            "values": [self.value],
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeList {
    pub list_id: String,
    pub property: KeyframeProperty,
    pub keyframes: Vec<Keyframe>,
}

impl KeyframeList {
    pub fn new(property: KeyframeProperty) -> Self {
        Self {
            list_id: Uuid::new_v4().simple().to_string(),
            property,
            keyframes: Vec::new(),
        }
    }

    /// Kept sorted by `time_offset_us`, matching `KeyframeList.add_keyframe`
    /// in `keyframe.py`.
    pub fn add_keyframe(&mut self, time_offset_us: i64, value: f64) {
        self.keyframes.push(Keyframe::new(time_offset_us, value));
        self.keyframes.sort_by_key(|k| k.time_offset_us);
    }

    /// Matches `KeyframeList.export_json` in `keyframe.py`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.list_id,
            "keyframe_list": self.keyframes.iter().map(Keyframe::export_json).collect::<Vec<_>>(),
            "material_id": "",
            "property_type": self.property.wire_value(),
        })
    }
}

/// `keyframe.time_offset_us` (absolute project time, per this module's doc
/// comment) minus `clip_position_us` (the owning clip's on-timeline start) ->
/// CapCut's clip-relative `time_offset`.
pub fn absolute_to_relative_offset_us(keyframe_time_offset_us: i64, clip_position_us: i64) -> i64 {
    keyframe_time_offset_us - clip_position_us
}

/// Converts a `project::Keyframe` into a CapCut `Keyframe` relative to
/// `clip_position_us`. Returns `None` if `keyframe.property` isn't one of
/// the six properties `KeyframeProperty::from_project_property` recognizes
/// (see that function's doc comment) — the caller should skip such entries
/// rather than fabricate a property.
pub fn from_project_keyframe(
    keyframe: &ProjectKeyframe,
    clip_position_us: i64,
) -> Option<(KeyframeProperty, Keyframe)> {
    let property = KeyframeProperty::from_project_property(&keyframe.property)?;
    let relative = absolute_to_relative_offset_us(keyframe.time_offset_us, clip_position_us);
    Some((property, Keyframe::new(relative, keyframe.value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_minus_clip_position_gives_relative_offset() {
        // Clip starts at 5s on the timeline; keyframe sits at 5.5s absolute.
        assert_eq!(
            absolute_to_relative_offset_us(5_500_000, 5_000_000),
            500_000
        );
    }

    #[test]
    fn from_project_keyframe_maps_known_property_and_converts_time() {
        let pk = ProjectKeyframe {
            id: "k1".into(),
            clip_id: "c1".into(),
            property: "position_x".into(),
            time_offset_us: 5_250_000,
            value: 0.3,
            curve: "linear".into(),
        };
        let (prop, kf) = from_project_keyframe(&pk, 5_000_000).expect("recognized property");
        assert_eq!(prop, KeyframeProperty::PositionX);
        assert_eq!(kf.time_offset_us, 250_000);
        assert_eq!(kf.value, 0.3);
    }

    #[test]
    fn unrecognized_property_returns_none_rather_than_fabricating() {
        let pk = ProjectKeyframe {
            id: "k1".into(),
            clip_id: "c1".into(),
            property: "hue".into(),
            time_offset_us: 0,
            value: 0.0,
            curve: "linear".into(),
        };
        assert!(from_project_keyframe(&pk, 0).is_none());
    }

    #[test]
    fn keyframe_list_stays_sorted_by_time_offset() {
        let mut list = KeyframeList::new(KeyframeProperty::Alpha);
        list.add_keyframe(500_000, 1.0);
        list.add_keyframe(100_000, 0.0);
        list.add_keyframe(300_000, 0.5);
        let offsets: Vec<i64> = list.keyframes.iter().map(|k| k.time_offset_us).collect();
        assert_eq!(offsets, vec![100_000, 300_000, 500_000]);
    }

    #[test]
    fn export_json_uses_property_wire_value() {
        let list = KeyframeList::new(KeyframeProperty::Volume);
        let v = list.export_json();
        assert_eq!(v["property_type"], serde_json::json!("KFTypeVolume"));
    }
}
