//! `SegmentAnimation`/`SegmentAnimations` — port of `animation.py`'s
//! `Animation`/`VideoAnimation`/`Text_animation`/`SegmentAnimations`.
//!
//! **Scope reduction** (documented, not silent, per this phase's task
//! brief): `animation.py`'s real `Animation` subclasses are constructed from
//! `metadata.AnimationMeta` catalog entries (`IntroType`/`OutroType`/
//! `GroupAnimationType`/`TextIntro`/...) — CapCut/Jianying-provided
//! `effect_id`/`resource_id` pairs identifying one of hundreds of named
//! transition animations ("渐显", "向左滑动", ...). That catalog is exactly
//! the kind of multi-hundred-entry static resource table this phase's brief
//! says not to port (this app has no animation-picker UI or catalog of its
//! own yet). `project::Animation` likewise only carries an opaque
//! `name: String` with no resource id — so `SegmentAnimation` below passes
//! `name` straight through with `effect_id`/`resource_id` left empty (an
//! honest "structural, unresolved reference" placeholder, matching how
//! `add_effect`/`add_sticker` are documented as passthrough in
//! `crate::capcut::adapter`), rather than resolving it against a catalog
//! that doesn't exist in this app.

use uuid::Uuid;

use crate::project::{Animation as ProjectAnimation, AnimationKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationType {
    In,
    Out,
    Loop,
    Group,
}

impl AnimationType {
    fn wire_value(self) -> &'static str {
        match self {
            AnimationType::In => "in",
            AnimationType::Out => "out",
            AnimationType::Loop => "loop",
            AnimationType::Group => "group",
        }
    }
}

impl From<AnimationKind> for AnimationType {
    fn from(kind: AnimationKind) -> Self {
        match kind {
            AnimationKind::In => AnimationType::In,
            AnimationKind::Out => AnimationType::Out,
            AnimationKind::Loop => AnimationType::Loop,
            AnimationKind::Group => AnimationType::Group,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentAnimation {
    /// Stands in for `Animation.effect_id` — see module doc comment on why
    /// this is an opaque generated id rather than a resolved catalog id.
    pub effect_id: String,
    pub name: String,
    /// Always empty: no animation-resource catalog is ported this pass.
    pub resource_id: String,
    pub animation_type: AnimationType,
    pub start_us: i64,
    pub duration_us: i64,
    pub is_video_animation: bool,
}

impl SegmentAnimation {
    /// Matches `Animation.export_json` in `animation.py`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "anim_adjust_params": null,
            "platform": "all",
            "panel": if self.is_video_animation { "video" } else { "" },
            "material_type": if self.is_video_animation { "video" } else { "sticker" },
            "name": self.name,
            "id": self.effect_id,
            "type": self.animation_type.wire_value(),
            "resource_id": self.resource_id,
            "start": self.start_us,
            "duration": self.duration_us,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentAnimations {
    pub animation_id: String,
    pub animations: Vec<SegmentAnimation>,
}

impl SegmentAnimations {
    pub fn new() -> Self {
        Self {
            animation_id: Uuid::new_v4().simple().to_string(),
            animations: Vec::new(),
        }
    }

    /// Matches `SegmentAnimations.export_json` in `animation.py`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.animation_id,
            "type": "sticker_animation",
            "multi_language_current": "none",
            "animations": self.animations.iter().map(SegmentAnimation::export_json).collect::<Vec<_>>(),
        })
    }
}

impl Default for SegmentAnimations {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a `project::Animation` into a `SegmentAnimation`.
/// `is_video_animation` is the caller's call (a `Caption`-track animation is
/// a text animation; a `Video`/`Image`/`Overlay`-track one is a video
/// animation) since `project::Animation` itself carries no track-kind hint.
pub fn from_project_animation(
    anim: &ProjectAnimation,
    is_video_animation: bool,
) -> SegmentAnimation {
    SegmentAnimation {
        effect_id: Uuid::new_v4().simple().to_string(),
        name: anim.name.clone(),
        resource_id: String::new(),
        animation_type: anim.kind.into(),
        start_us: 0,
        duration_us: anim.duration_us,
        is_video_animation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_animation_kind_to_wire_type() {
        let anim = ProjectAnimation {
            id: "a1".into(),
            clip_id: "c1".into(),
            kind: AnimationKind::Out,
            name: "Fade".into(),
            duration_us: 500_000,
        };
        let seg = from_project_animation(&anim, true);
        assert_eq!(seg.animation_type, AnimationType::Out);
        let v = seg.export_json();
        assert_eq!(v["type"], serde_json::json!("out"));
        assert_eq!(v["resource_id"], serde_json::json!(""));
        assert_eq!(v["panel"], serde_json::json!("video"));
    }

    #[test]
    fn text_animation_reports_sticker_material_type() {
        let anim = ProjectAnimation {
            id: "a1".into(),
            clip_id: "c1".into(),
            kind: AnimationKind::In,
            name: "Typewriter".into(),
            duration_us: 300_000,
        };
        let seg = from_project_animation(&anim, false);
        let v = seg.export_json();
        assert_eq!(v["material_type"], serde_json::json!("sticker"));
        assert_eq!(v["panel"], serde_json::json!(""));
    }
}
