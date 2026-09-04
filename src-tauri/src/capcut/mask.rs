//! `Mask`/`MaskType` — port of `video_segment.py`'s `Mask` class and
//! `metadata/mask_meta.py`'s `MaskType` enum.
//!
//! **Scope note**: `mask_meta.py` is a *small*, fixed 6-entry enum (line,
//! mirror, circle, rectangle, heart, star) — nothing like the
//! multi-thousand-entry effect/filter/transition catalogs this phase's
//! brief says to skip. It is ported in full here (real resource ids, real
//! default aspect ratios) since `IMPLEMENTATION_PLAN.md` explicitly calls
//! for a "mask size-ratio computation" test, which only means something
//! against real metadata.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskType {
    Line,
    Mirror,
    Circle,
    Rectangle,
    Heart,
    Star,
}

pub struct MaskTypeMeta {
    pub name: &'static str,
    pub resource_type: &'static str,
    pub effect_id: &'static str,
    pub resource_id: &'static str,
    /// `default_aspect_ratio` in `mask_meta.py` — used by
    /// `compute_mask_width` below.
    pub default_aspect_ratio: f64,
}

impl MaskType {
    /// Real values ported verbatim from `metadata/mask_meta.py`.
    pub const fn meta(self) -> MaskTypeMeta {
        match self {
            MaskType::Line => MaskTypeMeta {
                name: "线性",
                resource_type: "line",
                effect_id: "6791652175668843016",
                resource_id: "1f467b8b9bb94cecc46d916219b7940a",
                default_aspect_ratio: 1.0,
            },
            MaskType::Mirror => MaskTypeMeta {
                name: "镜面",
                resource_type: "mirror",
                effect_id: "6791699060140020232",
                resource_id: "b2c0516d1f737f4542fb9b2862907817",
                default_aspect_ratio: 1.0,
            },
            MaskType::Circle => MaskTypeMeta {
                name: "圆形",
                resource_type: "circle",
                effect_id: "6791700663249146381",
                resource_id: "9a55eae0e99ee6d1ecbc6defaf0501ec",
                default_aspect_ratio: 1.0,
            },
            MaskType::Rectangle => MaskTypeMeta {
                name: "矩形",
                resource_type: "rectangle",
                effect_id: "6791700809454195207",
                resource_id: "ef361d96c456cd6077c76d737f98898d",
                default_aspect_ratio: 1.0,
            },
            MaskType::Heart => MaskTypeMeta {
                name: "爱心",
                resource_type: "geometric_shape",
                effect_id: "6794051276482023949",
                resource_id: "0bf09fa1e3a32464fed4f71e49a8ab01",
                default_aspect_ratio: 1.115,
            },
            MaskType::Star => MaskTypeMeta {
                name: "星形",
                resource_type: "geometric_shape",
                effect_id: "6794051169434997255",
                resource_id: "155612dee601d3f5422a3fbeabc7610c",
                default_aspect_ratio: 1.05,
            },
        }
    }
}

/// Port of `VideoSegment.add_mask`'s width formula:
/// `width = rect_width if rect_width is not None else size * material_height
/// * default_aspect_ratio / material_width`. `rect_width` is only ever
/// supplied for `MaskType::Rectangle` in the Python original; callers should
/// pass `None` for every other mask type.
pub fn compute_mask_width(
    rect_width: Option<f64>,
    size: f64,
    mask_type: MaskType,
    material_width: u32,
    material_height: u32,
) -> f64 {
    rect_width.unwrap_or_else(|| {
        size * material_height as f64 * mask_type.meta().default_aspect_ratio
            / material_width as f64
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    pub global_id: String,
    pub mask_type: MaskType,
    /// Half-material-width units (matches `Mask.center_x` in `video_segment.py`).
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub aspect_ratio: f64,
    pub rotation: f64,
    pub invert: bool,
    /// `0.0..=1.0`.
    pub feather: f64,
    /// `0.0..=1.0`.
    pub round_corner: f64,
}

impl Mask {
    /// Matches `Mask.export_json` in `video_segment.py`.
    pub fn export_json(&self) -> serde_json::Value {
        let meta = self.mask_type.meta();
        serde_json::json!({
            "config": {
                "aspectRatio": self.aspect_ratio,
                "centerX": self.center_x,
                "centerY": self.center_y,
                "feather": self.feather,
                "height": self.height,
                "invert": self.invert,
                "rotation": self.rotation,
                "roundCorner": self.round_corner,
                "width": self.width,
            },
            "id": self.global_id,
            "name": meta.name,
            "platform": "all",
            "position_info": "",
            "resource_type": meta.resource_type,
            "resource_id": meta.resource_id,
            "type": "mask",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_mask_width_defaults_to_size_when_no_rect_width_given() {
        // material 1920x1080, size=0.5 (half height), rectangle aspect ratio 1.0.
        let w = compute_mask_width(None, 0.5, MaskType::Rectangle, 1920, 1080);
        // 0.5 * 1080 * 1.0 / 1920 = 0.28125
        assert!((w - 0.28125).abs() < 1e-9);
    }

    #[test]
    fn heart_mask_uses_its_own_default_aspect_ratio() {
        let w = compute_mask_width(None, 0.5, MaskType::Heart, 1920, 1080);
        // 0.5 * 1080 * 1.115 / 1920
        let expected = 0.5 * 1080.0 * 1.115 / 1920.0;
        assert!((w - expected).abs() < 1e-9);
    }

    #[test]
    fn explicit_rect_width_overrides_the_computed_value() {
        let w = compute_mask_width(Some(0.75), 0.5, MaskType::Rectangle, 1920, 1080);
        assert_eq!(w, 0.75);
    }

    #[test]
    fn export_json_includes_resolved_metadata() {
        let mask = Mask {
            global_id: "m1".into(),
            mask_type: MaskType::Circle,
            center_x: 0.0,
            center_y: 0.0,
            width: 0.5,
            height: 0.5,
            aspect_ratio: 1.0,
            rotation: 0.0,
            invert: false,
            feather: 0.0,
            round_corner: 0.0,
        };
        let v = mask.export_json();
        assert_eq!(v["resource_type"], serde_json::json!("circle"));
        assert_eq!(v["type"], serde_json::json!("mask"));
    }
}
