//! Built-in `CaptionStyle` templates (master prompt §26: Minimal, TikTok,
//! Podcast, News, Gaming, Karaoke). Pure catalog function, same pattern as
//! `render::presets::all_presets` — each call returns fresh owned values
//! with fixed, stable `id`s; nothing here touches a project.

use crate::project::{
    CaptionAlignment, CaptionAnchor, CaptionBackground, CaptionOutline, CaptionPosition,
    CaptionShadow, CaptionStyle, Color, SafeMargins,
};

fn color(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b }
}

/// Returns the six built-in templates, in a fixed, documented order. Every
/// `id` is a stable literal (not a freshly-generated UUID) so the frontend
/// can hardcode e.g. `"template_tiktok"` as a default selection.
pub fn all_caption_templates() -> Vec<CaptionStyle> {
    vec![minimal(), tiktok(), podcast(), news(), gaming(), karaoke()]
}

/// The style used whenever a `Caption` has `style_id: None` (or one that
/// doesn't resolve against the current catalog) — plain white centered text,
/// no background/outline/shadow. This is a Rust-side port of the frontend's
/// own `FALLBACK_CAPTION_STYLE` (`src/captions/styleCatalog.ts`) — same
/// field values, byte-for-byte — so a caption with no resolvable style burns
/// in looking exactly like the live preview already renders it, not a
/// second, independently-invented fallback. Not persisted anywhere; a pure
/// fallback value, same as its frontend counterpart.
pub fn fallback_caption_style() -> CaptionStyle {
    CaptionStyle {
        id: "__fallback__".to_string(),
        name: "Default".to_string(),
        font_family: "system-ui".to_string(),
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
    }
}

/// Resolves a `Caption::style_id` against a style catalog (built-ins +
/// current project's own `caption_styles`, in that order — see
/// `render::graph::build_render_graph`'s call site for how the catalog is
/// assembled), falling back to [`fallback_caption_style`] for `None` or an
/// id that isn't found — the exact same two-step resolution the frontend's
/// `resolveCaptionStyle` (`src/captions/styleCatalog.ts`) already performs
/// for the live preview. Kept here, next to the catalog it resolves against,
/// rather than in `render::` — a project can have zero renders and still
/// want to know "what style does this caption resolve to" (e.g. a future
/// caption-list UI), so this isn't a render-only concern.
pub fn resolve_caption_style(catalog: &[CaptionStyle], style_id: Option<&str>) -> CaptionStyle {
    match style_id {
        None => fallback_caption_style(),
        Some(id) => catalog
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .unwrap_or_else(fallback_caption_style),
    }
}

/// Small, no background, subtle — a caption that stays out of the way.
fn minimal() -> CaptionStyle {
    CaptionStyle {
        id: "template_minimal".to_string(),
        name: "Minimal".to_string(),
        font_family: "Inter".to_string(),
        font_size: 28.0,
        bold: false,
        italic: false,
        alignment: CaptionAlignment::Center,
        position: CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: 0.0,
            offset_y: -0.08,
        },
        text_color: Color::WHITE,
        background: None,
        outline: None,
        shadow: Some(CaptionShadow {
            color: Color::BLACK,
            opacity: 0.5,
            offset_x: 0.0,
            offset_y: 0.003,
            blur: 6.0,
        }),
        opacity: 1.0,
        safe_margins: SafeMargins {
            top: 0.05,
            bottom: 0.06,
            left: 0.08,
            right: 0.08,
        },
    }
}

/// Bold, large, centered, high-contrast background — common short-form
/// (TikTok/Reels/Shorts) caption conventions.
fn tiktok() -> CaptionStyle {
    CaptionStyle {
        id: "template_tiktok".to_string(),
        name: "TikTok".to_string(),
        font_family: "Montserrat".to_string(),
        font_size: 46.0,
        bold: true,
        italic: false,
        alignment: CaptionAlignment::Center,
        position: CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: 0.0,
            offset_y: -0.18,
        },
        text_color: Color::WHITE,
        background: Some(CaptionBackground {
            color: Color::BLACK,
            opacity: 0.55,
        }),
        outline: Some(CaptionOutline {
            color: Color::BLACK,
            width: 0.1,
        }),
        shadow: Some(CaptionShadow {
            color: Color::BLACK,
            opacity: 0.8,
            offset_x: 0.0,
            offset_y: 0.004,
            blur: 10.0,
        }),
        opacity: 1.0,
        safe_margins: SafeMargins {
            top: 0.1,
            bottom: 0.2,
            left: 0.06,
            right: 0.06,
        },
    }
}

/// Understated lower-third look for long-form talking/interview content.
fn podcast() -> CaptionStyle {
    CaptionStyle {
        id: "template_podcast".to_string(),
        name: "Podcast".to_string(),
        font_family: "Inter".to_string(),
        font_size: 30.0,
        bold: false,
        italic: false,
        alignment: CaptionAlignment::Center,
        position: CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: 0.0,
            offset_y: -0.1,
        },
        text_color: Color::WHITE,
        background: Some(CaptionBackground {
            color: Color::BLACK,
            opacity: 0.4,
        }),
        outline: None,
        shadow: None,
        opacity: 1.0,
        safe_margins: SafeMargins {
            top: 0.05,
            bottom: 0.1,
            left: 0.1,
            right: 0.1,
        },
    }
}

/// Broadcast-news-style lower third: left-aligned, solid opaque bar.
fn news() -> CaptionStyle {
    CaptionStyle {
        id: "template_news".to_string(),
        name: "News".to_string(),
        font_family: "Roboto".to_string(),
        font_size: 32.0,
        bold: true,
        italic: false,
        alignment: CaptionAlignment::Left,
        position: CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: -0.35,
            offset_y: -0.12,
        },
        text_color: Color::WHITE,
        background: Some(CaptionBackground {
            color: color(0.05, 0.1, 0.35),
            opacity: 0.9,
        }),
        outline: None,
        shadow: None,
        opacity: 1.0,
        safe_margins: SafeMargins {
            top: 0.05,
            bottom: 0.12,
            left: 0.06,
            right: 0.06,
        },
    }
}

/// Vivid, thick-outlined, high-energy look for gaming/streaming content.
fn gaming() -> CaptionStyle {
    CaptionStyle {
        id: "template_gaming".to_string(),
        name: "Gaming".to_string(),
        font_family: "Montserrat".to_string(),
        font_size: 40.0,
        bold: true,
        italic: false,
        alignment: CaptionAlignment::Center,
        position: CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: 0.0,
            offset_y: -0.15,
        },
        text_color: color(1.0, 0.85, 0.1),
        background: None,
        outline: Some(CaptionOutline {
            color: Color::BLACK,
            width: 0.14,
        }),
        shadow: Some(CaptionShadow {
            color: Color::BLACK,
            opacity: 0.9,
            offset_x: 0.004,
            offset_y: 0.004,
            blur: 4.0,
        }),
        opacity: 1.0,
        safe_margins: SafeMargins {
            top: 0.05,
            bottom: 0.15,
            left: 0.06,
            right: 0.06,
        },
    }
}

/// Styled for active-word highlighting to actually be visible: a neutral
/// base color (the frontend overlays the highlight color on the currently
/// spoken word, master prompt §27), strong outline/shadow so it reads
/// clearly against any footage, generously sized.
fn karaoke() -> CaptionStyle {
    CaptionStyle {
        id: "template_karaoke".to_string(),
        name: "Karaoke".to_string(),
        font_family: "Montserrat".to_string(),
        font_size: 42.0,
        bold: true,
        italic: false,
        alignment: CaptionAlignment::Center,
        position: CaptionPosition {
            anchor: CaptionAnchor::Bottom,
            offset_x: 0.0,
            offset_y: -0.16,
        },
        text_color: color(0.85, 0.85, 0.85),
        background: Some(CaptionBackground {
            color: Color::BLACK,
            opacity: 0.35,
        }),
        outline: Some(CaptionOutline {
            color: Color::BLACK,
            width: 0.1,
        }),
        shadow: Some(CaptionShadow {
            color: Color::BLACK,
            opacity: 0.7,
            offset_x: 0.0,
            offset_y: 0.003,
            blur: 8.0,
        }),
        opacity: 1.0,
        safe_margins: SafeMargins {
            top: 0.05,
            bottom: 0.18,
            left: 0.08,
            right: 0.08,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_six_templates_are_present_exactly_once() {
        let templates = all_caption_templates();
        assert_eq!(templates.len(), 6);
        let ids: HashSet<&str> = templates.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 6, "template ids must be unique");
        for expected in [
            "template_minimal",
            "template_tiktok",
            "template_podcast",
            "template_news",
            "template_gaming",
            "template_karaoke",
        ] {
            assert!(ids.contains(expected), "missing template {expected}");
        }
        let names: HashSet<&str> = templates.iter().map(|t| t.name.as_str()).collect();
        for expected in ["Minimal", "TikTok", "Podcast", "News", "Gaming", "Karaoke"] {
            assert!(
                names.contains(expected),
                "missing template named {expected}"
            );
        }
    }

    #[test]
    fn every_template_has_sane_field_values() {
        for style in all_caption_templates() {
            assert!(!style.id.is_empty());
            assert!(!style.name.is_empty());
            assert!(!style.font_family.is_empty());
            assert!(style.font_size > 0.0);
            assert!((0.0..=1.0).contains(&style.opacity));
            assert!(style.safe_margins.top >= 0.0 && style.safe_margins.top < 1.0);
            assert!(style.safe_margins.bottom >= 0.0 && style.safe_margins.bottom < 1.0);
            assert!(style.safe_margins.left >= 0.0 && style.safe_margins.left < 1.0);
            assert!(style.safe_margins.right >= 0.0 && style.safe_margins.right < 1.0);
            if let Some(bg) = &style.background {
                assert!((0.0..=1.0).contains(&bg.opacity));
            }
            if let Some(outline) = &style.outline {
                assert!(outline.width > 0.0);
            }
            if let Some(shadow) = &style.shadow {
                assert!((0.0..=1.0).contains(&shadow.opacity));
                assert!(shadow.blur >= 0.0);
            }
        }
    }

    #[test]
    fn tiktok_is_bold_large_centered_with_high_contrast_background() {
        let tiktok = all_caption_templates()
            .into_iter()
            .find(|t| t.id == "template_tiktok")
            .unwrap();
        assert!(tiktok.bold);
        assert!(tiktok.font_size >= 40.0);
        assert_eq!(tiktok.alignment, CaptionAlignment::Center);
        assert!(tiktok.background.is_some());
    }

    #[test]
    fn minimal_has_no_background_and_is_smaller_than_tiktok() {
        let templates = all_caption_templates();
        let minimal = templates
            .iter()
            .find(|t| t.id == "template_minimal")
            .unwrap();
        let tiktok = templates
            .iter()
            .find(|t| t.id == "template_tiktok")
            .unwrap();
        assert!(minimal.background.is_none());
        assert!(minimal.font_size < tiktok.font_size);
    }

    #[test]
    fn karaoke_has_strong_contrast_aids_for_active_word_highlighting() {
        let karaoke = all_caption_templates()
            .into_iter()
            .find(|t| t.id == "template_karaoke")
            .unwrap();
        assert!(karaoke.outline.is_some());
        assert!(karaoke.shadow.is_some());
    }

    #[test]
    fn fallback_style_matches_the_frontends_own_fallback_caption_style() {
        let fallback = fallback_caption_style();
        assert_eq!(fallback.id, "__fallback__");
        assert_eq!(fallback.font_family, "system-ui");
        assert_eq!(fallback.font_size, 32.0);
        assert!(!fallback.bold);
        assert!(!fallback.italic);
        assert_eq!(fallback.alignment, CaptionAlignment::Center);
        assert_eq!(fallback.position.anchor, CaptionAnchor::Bottom);
        assert_eq!(fallback.position.offset_x, 0.0);
        assert_eq!(fallback.position.offset_y, 0.0);
        assert_eq!(fallback.text_color, Color::WHITE);
        assert!(fallback.background.is_none());
        assert!(fallback.outline.is_none());
        assert!(fallback.shadow.is_none());
        assert_eq!(fallback.opacity, 1.0);
    }

    #[test]
    fn resolve_caption_style_returns_fallback_for_none() {
        let catalog = all_caption_templates();
        let resolved = resolve_caption_style(&catalog, None);
        assert_eq!(resolved.id, "__fallback__");
    }

    #[test]
    fn resolve_caption_style_returns_fallback_for_an_unknown_id() {
        let catalog = all_caption_templates();
        let resolved = resolve_caption_style(&catalog, Some("does_not_exist"));
        assert_eq!(resolved.id, "__fallback__");
    }

    #[test]
    fn resolve_caption_style_finds_a_built_in_template_by_id() {
        let catalog = all_caption_templates();
        let resolved = resolve_caption_style(&catalog, Some("template_tiktok"));
        assert_eq!(resolved.id, "template_tiktok");
        assert_eq!(resolved.name, "TikTok");
    }

    #[test]
    fn resolve_caption_style_finds_a_project_custom_style_appended_after_built_ins() {
        let mut catalog = all_caption_templates();
        let custom = CaptionStyle {
            id: "custom_1".to_string(),
            ..fallback_caption_style()
        };
        catalog.push(custom);
        let resolved = resolve_caption_style(&catalog, Some("custom_1"));
        assert_eq!(resolved.id, "custom_1");
    }
}
