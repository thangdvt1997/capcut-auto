//! `CapCutClipSettings` — port of `segment.py`'s `ClipSettings` (the visual
//! transform CapCut attaches to a segment: opacity/flip/rotation/scale/
//! transform).
//!
//! Named `CapCutClipSettings` (not `ClipSettings`) specifically so it never
//! collides with `crate::project::ClipSettings`, this project's own
//! internal-timeline transform type (see this module's parent doc comment
//! and `IMPLEMENTATION_PLAN.md` Phase 9). The two are conveniently
//! near-identical in convention (`project::ClipSettings`'s doc comment
//! already says it mirrors pyJianYingDraft's `transform_x`/`transform_y`
//! half-canvas-unit convention), so `From<&project::ClipSettings>` below is a
//! direct field-for-field mapping, not a unit conversion.

use crate::project::ClipSettings as ProjectClipSettings;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapCutClipSettings {
    pub alpha: f64,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    /// Clockwise rotation in **degrees**, matching `ClipSettings.rotation`.
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    /// Half-canvas-width units.
    pub transform_x: f64,
    /// Half-canvas-height units.
    pub transform_y: f64,
}

impl Default for CapCutClipSettings {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            flip_horizontal: false,
            flip_vertical: false,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            transform_x: 0.0,
            transform_y: 0.0,
        }
    }
}

impl From<&ProjectClipSettings> for CapCutClipSettings {
    fn from(cs: &ProjectClipSettings) -> Self {
        Self {
            alpha: cs.opacity,
            flip_horizontal: cs.flip_h,
            flip_vertical: cs.flip_v,
            rotation: cs.rotation_deg,
            scale_x: cs.scale_x,
            scale_y: cs.scale_y,
            transform_x: cs.transform_x,
            transform_y: cs.transform_y,
        }
    }
}

impl CapCutClipSettings {
    /// Matches `ClipSettings.export_json` in `segment.py` exactly:
    /// `{"alpha", "flip": {...}, "rotation", "scale": {...}, "transform": {...}}`.
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "alpha": self.alpha,
            "flip": { "horizontal": self.flip_horizontal, "vertical": self.flip_vertical },
            "rotation": self.rotation,
            "scale": { "x": self.scale_x, "y": self.scale_y },
            "transform": { "x": self.transform_x, "y": self.transform_y },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_project_clip_settings_is_a_direct_field_mapping() {
        let p = ProjectClipSettings {
            opacity: 0.5,
            flip_h: true,
            flip_v: false,
            rotation_deg: 90.0,
            scale_x: 2.0,
            scale_y: 1.5,
            transform_x: 0.1,
            transform_y: -0.2,
        };
        let c = CapCutClipSettings::from(&p);
        assert_eq!(c.alpha, 0.5);
        assert!(c.flip_horizontal);
        assert!(!c.flip_vertical);
        assert_eq!(c.rotation, 90.0);
        assert_eq!(c.scale_x, 2.0);
        assert_eq!(c.scale_y, 1.5);
        assert_eq!(c.transform_x, 0.1);
        assert_eq!(c.transform_y, -0.2);
    }

    #[test]
    fn export_json_matches_pyjianyingdraft_shape() {
        let c = CapCutClipSettings::default();
        let v = c.export_json();
        assert_eq!(v["alpha"], serde_json::json!(1.0));
        assert_eq!(v["flip"]["horizontal"], serde_json::json!(false));
        assert_eq!(v["scale"]["x"], serde_json::json!(1.0));
        assert_eq!(v["transform"]["y"], serde_json::json!(0.0));
    }
}
