//! `TextStyle`/`TextBorder`/`TextBackground`/`TextShadow` — port of the
//! style-related classes in `text_segment.py`, plus the
//! `project::CaptionStyle -> these` mapping that `capcut::adapter::add_caption`
//! calls.
//!
//! ## Field-choice notes (per `IMPLEMENTATION_PLAN.md` Phase 8 item 5 /
//! Phase 9 task brief)
//!
//! - **Color**: `project::Color` is already per-channel `[0.0, 1.0]`,
//!   matching `TextStyle.color`'s own convention (`add_captions.py`'s
//!   `hex_to_rgb`) — direct passthrough, no hex round-trip.
//! - **Outline width**: `project::CaptionOutline::width` is documented as
//!   *already* a fraction of font size (matching `TextBorder`'s own
//!   `0.08`-style convention), unlike `TextBorder.__init__`'s raw
//!   `width: float = 40.0` constructor parameter (a 0-100 UI slider value
//!   that Python itself divides by `100.0 * 0.2` to get the fraction it
//!   actually stores/exports). Since `project::CaptionOutline::width` is
//!   already past that conversion, this adapter passes it straight through
//!   to `TextBorder::width` — applying the `/100.0*0.2` formula a second
//!   time here would be a bug, not a faithful port.
//! - **Shadow distance/angle**: `project::CaptionShadow` stores a
//!   Cartesian `offset_x`/`offset_y` (half-canvas-fraction units, matching
//!   `CaptionPosition`'s own convention) plus a `blur` already on
//!   `TextShadow.diffuse`'s native `0..=100` scale. CapCut's `TextShadow`
//!   instead wants a polar `distance`/`angle` pair on that same `0..=100`-ish
//!   scale (`distance: 0..=100`, `angle: -180..=180`). This adapter derives
//!   `distance = hypot(offset_x, offset_y) * 100.0` (scaling the
//!   half-canvas-fraction magnitude onto that `0..=100` distance scale,
//!   consistent with `blur` already sharing it) and
//!   `angle = atan2(offset_y, offset_x).to_degrees()`. This exact
//!   angle-axis orientation is a best-effort mapping, not verified against a
//!   real CapCut install (that verification is explicitly out of scope for
//!   this pass — see `IMPLEMENTATION_PLAN.md`'s "Validate draft
//!   compatibility..." checklist item).
//! - **Background**: unlike the task brief's expectation, `text_segment.py`
//!   *does* have a native background field — `TextBackground`, merged
//!   directly into `TextSegment.export_material()`'s top-level dict (not a
//!   field of `TextStyle`, but still a first-class, directly-exported
//!   concept). So `project::CaptionBackground` maps onto it directly; no
//!   shape/rect-segment synthesis is needed. This corrects the task brief's
//!   premise, based on reading the actual vendored source.
//! - **Font**: no font-resource catalog is ported this pass (same scope
//!   reduction reasoning as `metadata::FontType`'s 809-line catalog) — text
//!   always renders as CapCut's system-default font. `bold`/`italic` are
//!   plain style booleans independent of any font *resource*, so those
//!   still map through for real.

use crate::capcut::clip_settings::CapCutClipSettings;
use crate::project::{
    CaptionBackground, CaptionOutline, CaptionPosition, CaptionShadow, CaptionStyle,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: (f32, f32, f32),
    pub alpha: f64,
    /// `0` = left, `1` = center, `2` = right (matches `TextStyle.align`).
    pub align: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBorder {
    pub alpha: f64,
    pub color: (f32, f32, f32),
    /// Already a font-size fraction — see module doc comment.
    pub width: f64,
}

impl TextBorder {
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "content": { "solid": { "alpha": self.alpha, "color": [self.color.0, self.color.1, self.color.2] } },
            "width": self.width,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBackground {
    pub style: u8,
    pub alpha: f64,
    pub color_hex: [u8; 3],
    pub round_radius: f64,
    pub height: f64,
    pub width: f64,
    pub horizontal_offset: f64,
    pub vertical_offset: f64,
}

impl TextBackground {
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "background_style": self.style,
            "background_color": format!("#{:02X}{:02X}{:02X}", self.color_hex[0], self.color_hex[1], self.color_hex[2]),
            "background_alpha": self.alpha,
            "background_round_radius": self.round_radius,
            "background_height": self.height,
            "background_width": self.width,
            "background_horizontal_offset": self.horizontal_offset,
            "background_vertical_offset": self.vertical_offset,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShadow {
    pub alpha: f64,
    pub color: (f32, f32, f32),
    /// `0..=100`.
    pub diffuse: f64,
    /// `0..=100`.
    pub distance: f64,
    /// `-180..=180`.
    pub angle: f64,
}

impl TextShadow {
    pub fn export_json(&self) -> serde_json::Value {
        serde_json::json!({
            "diffuse": self.diffuse / 100.0 / 6.0,
            "alpha": self.alpha,
            "distance": self.distance,
            "content": { "solid": { "color": [self.color.0, self.color.1, self.color.2] } },
            "angle": self.angle,
        })
    }
}

fn to_rgb_tuple(c: crate::project::Color) -> (f32, f32, f32) {
    (c.r, c.g, c.b)
}

pub fn border_from_outline(outline: &CaptionOutline) -> TextBorder {
    TextBorder {
        alpha: 1.0,
        color: to_rgb_tuple(outline.color),
        width: outline.width,
    }
}

pub fn background_from_caption_background(bg: &CaptionBackground) -> TextBackground {
    let (r, g, b) = to_rgb_tuple(bg.color);
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    TextBackground {
        style: 1,
        alpha: bg.opacity,
        color_hex: [to_u8(r), to_u8(g), to_u8(b)],
        round_radius: 0.0,
        // Default box sizing (`text_segment.py`'s own `TextBackground`
        // constructor defaults) — `project::CaptionBackground` carries no
        // width/height/offset fields of its own to map from.
        height: 0.14,
        width: 0.14,
        horizontal_offset: 0.0,
        vertical_offset: 0.0,
    }
}

/// Derives `distance`/`angle` from `offset_x`/`offset_y` via `hypot`/`atan2`
/// — see module doc comment for the scale reasoning.
pub fn shadow_from_caption_shadow(shadow: &CaptionShadow) -> TextShadow {
    let distance = shadow.offset_x.hypot(shadow.offset_y) * 100.0;
    let angle = shadow.offset_y.atan2(shadow.offset_x).to_degrees();
    TextShadow {
        alpha: shadow.opacity,
        color: to_rgb_tuple(shadow.color),
        diffuse: shadow.blur,
        distance,
        angle,
    }
}

/// Maps a `CaptionPosition` onto `CapCutClipSettings::transform_x/y` — the
/// same half-canvas-fraction unit both already use. `anchor` supplies a base
/// vertical offset (matching `import_srt`'s own `transform_y=-0.8` default
/// for bottom-anchored captions in `script_file.py`) that `offset_y` then
/// fine-tunes on top of; `anchor` has no horizontal equivalent in this
/// schema, so `offset_x` maps straight to `transform_x`.
pub fn clip_settings_from_caption_position(position: &CaptionPosition) -> CapCutClipSettings {
    use crate::project::CaptionAnchor;
    let base_y = match position.anchor {
        CaptionAnchor::Top => 0.8,
        CaptionAnchor::Center => 0.0,
        CaptionAnchor::Bottom => -0.8,
    };
    CapCutClipSettings {
        transform_x: position.offset_x,
        transform_y: base_y + position.offset_y,
        ..CapCutClipSettings::default()
    }
}

pub fn text_style_from_caption_style(style: &CaptionStyle) -> TextStyle {
    use crate::project::CaptionAlignment;
    TextStyle {
        size: style.font_size,
        bold: style.bold,
        italic: style.italic,
        color: to_rgb_tuple(style.text_color),
        alpha: style.opacity,
        align: match style.alignment {
            CaptionAlignment::Left => 0,
            CaptionAlignment::Center => 1,
            CaptionAlignment::Right => 2,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Color;

    #[test]
    fn outline_width_passes_through_without_a_second_conversion() {
        let outline = CaptionOutline {
            color: Color::BLACK,
            width: 0.08,
        };
        let border = border_from_outline(&outline);
        assert_eq!(border.width, 0.08);
    }

    #[test]
    fn shadow_distance_and_angle_derived_via_hypot_atan2() {
        let shadow = CaptionShadow {
            color: Color::BLACK,
            opacity: 0.9,
            offset_x: 0.0,
            offset_y: 0.05,
            blur: 15.0,
        };
        let ts = shadow_from_caption_shadow(&shadow);
        assert!((ts.distance - 5.0).abs() < 1e-9);
        assert!((ts.angle - 90.0).abs() < 1e-9);
        assert_eq!(ts.diffuse, 15.0);
    }

    #[test]
    fn shadow_zero_offset_gives_zero_distance() {
        let shadow = CaptionShadow {
            color: Color::BLACK,
            opacity: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
        };
        let ts = shadow_from_caption_shadow(&shadow);
        assert_eq!(ts.distance, 0.0);
    }

    #[test]
    fn background_maps_color_and_opacity() {
        let bg = CaptionBackground {
            color: Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            },
            opacity: 0.5,
        };
        let tb = background_from_caption_background(&bg);
        assert_eq!(tb.color_hex, [255, 0, 0]);
        assert_eq!(tb.alpha, 0.5);
        let v = tb.export_json();
        assert_eq!(v["background_color"], serde_json::json!("#FF0000"));
    }

    #[test]
    fn bottom_anchor_maps_to_negative_transform_y_like_srt_import() {
        use crate::project::CaptionAnchor;
        let pos = CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: 0.1,
            offset_y: 0.05,
        };
        let cs = clip_settings_from_caption_position(&pos);
        assert_eq!(cs.transform_x, 0.1);
        assert!((cs.transform_y - (-0.75)).abs() < 1e-9);
    }

    #[test]
    fn text_style_maps_alignment_to_numeric_code() {
        use crate::project::{CaptionAlignment, CaptionAnchor, CaptionPosition, SafeMargins};
        let style = CaptionStyle {
            id: "s1".into(),
            name: "Test".into(),
            font_family: "Arial".into(),
            font_size: 42.0,
            bold: true,
            italic: false,
            alignment: CaptionAlignment::Right,
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
        };
        let ts = text_style_from_caption_style(&style);
        assert_eq!(ts.align, 2);
        assert!(ts.bold);
        assert_eq!(ts.size, 42.0);
    }
}
