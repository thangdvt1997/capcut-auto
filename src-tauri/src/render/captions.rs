//! Real FFmpeg `drawtext` filter generation for burned-in captions
//! (`STUDIO_PLAN.md` Phase S4, closing the `render::graph`/`render::plan`
//! documented no-op). `render::plan` is the only caller — kept in its own
//! module for the same reason `render::audio_filters` is: the plan builder's
//! already-large per-clip filter-chain logic shouldn't grow a second,
//! unrelated concern inline.
//!
//! ## `drawtext` vs. `subtitles=`/`ass=` — the real design call
//!
//! Two real ffmpeg techniques can burn text into a render: (1) one
//! `drawtext=` filter per caption, time-windowed via `enable=`, or (2)
//! generate a real `.ass` (Advanced SubStation Alpha) file with per-cue
//! style overrides and burn it in via the `subtitles=`/`ass=` filter. This
//! module uses (1). Reasoning, made after reading the actual style schema
//! and its two other consumers (`project::types::CaptionStyle`,
//! `capcut::caption_style`, `src/components/preview/CaptionOverlay.svelte`),
//! not assumed up front:
//! - `CaptionStyle` already tracks exactly the per-caption fields `drawtext`
//!   natively supports as filter options (`fontcolor`, `fontsize`, `box`/
//!   `boxcolor`, `bordercolor`/`borderw`, `shadowcolor`/`shadowx`/`shadowy`,
//!   `alpha`), so mapping it onto `drawtext` options is a direct,
//!   field-for-field translation, not a lossy reduction.
//! - An `.ass`-file approach would need genuinely new machinery this
//!   codebase has no precedent for (no "write a subtitle file" writer exists
//!   anywhere here), plus its own escaping rules, plus a real ASS style
//!   dialect (`\an`/`\pos`/`\c&Hxxxxxx&`/`\bord`/`\shad` override tags) to
//!   hand-generate correctly — meaningfully more machinery for the exact
//!   same real-world result `drawtext` already gives directly.
//! - `drawtext`'s one real style gap relative to `.ass` is per-word/per-line
//!   karaoke highlight timing — out of scope for burn-in per this phase's
//!   own honest-scope note (`docs/feature-matrix.md`'s "Active-word/karaoke
//!   highlighting" row: preview-only, none of the three export paths bake
//!   per-word timing into their output — this phase does not change that).
//!
//! ## Honest scope: font resolution
//!
//! No font-management/catalog system exists in this codebase
//! (`capcut::caption_style`'s own module doc comment notes the same gap for
//! the CapCut export path: "no font-resource catalog is ported this pass").
//! This module does not invent one. Instead it passes `CaptionStyle::
//! font_family` straight through as an ffmpeg **fontconfig pattern**
//! (`font=<family>[:style=Bold][:style=Italic][:style=Bold Italic]`) — a
//! real, verified mechanism (both this project's dev/CI ffmpeg on Linux and
//! the bundled Windows sidecar binary in `src-tauri/binaries/` are built
//! with `--enable-fontconfig --enable-libfreetype`, confirmed by inspecting
//! the actual shipped binary, not assumed). When fontconfig can resolve the
//! requested family/style, the real font/weight/slant renders; when it
//! can't (an unavailable family on the end user's machine), fontconfig
//! silently substitutes its own default sans-serif — every OTHER real style
//! property (color/size/position/background/outline/shadow/opacity) still
//! applies regardless. This is a real, honest fallback (matching Phase S2's
//! watermark work honestly scoping itself to static images only), not a
//! silent no-op and not a false claim of exact font-family/weight fidelity.
//!
//! ## Honest scope: shadow blur
//!
//! `CaptionStyle::shadow.blur` (a soft diffuse radius, matching the CapCut/
//! CSS-preview shadow model) has no ffmpeg `drawtext` equivalent — `drawtext`
//! only supports a flat, unblurred offset-copy shadow (`shadowcolor`/
//! `shadowx`/`shadowy`). This module applies the real offset/color and
//! documents (here, once) that `blur` itself does not carry through to the
//! burned-in render — a real, honest gap, not a silent drop of the whole
//! shadow.
//!
//! ## Position mapping: matching the live preview, not the type's own doc comment
//!
//! `CaptionPosition`'s doc comment claims its `offset_x`/`offset_y` share
//! `ClipSettings::transform_x/y`'s "positive-y-is-up" convention. But the
//! actual shipped live-preview renderer (`CaptionOverlay.svelte`) does *not*
//! negate `offset_y` when converting it to a CSS `translateY` (a y-down
//! pixel space, same direction ffmpeg's own `drawtext`/`overlay` pixel space
//! uses) — it passes it straight through. Per this phase's own task brief
//! ("reuse whatever anchor->position convention `CaptionOverlay.svelte`
//! already established... so burned-in captions land in the same place the
//! live preview already showed them, not a second, inconsistent
//! convention"), this module deliberately matches the live preview's own
//! *actual, shipped* pixel behavior (no sign flip) rather than the type doc
//! comment's stated intent — a user who has already nudged a caption's
//! `offset_y` slider while watching the live preview gets the same visual
//! result burned in, which is what actually matters here. Horizontally,
//! `CaptionOverlay.svelte`'s safe-area box is always centered
//! (`align-items: center`) regardless of anchor, with `offset_x` as a further
//! pixel nudge on top (positive = right, agreeing with ffmpeg's own x-right
//! pixel convention, no sign ambiguity there) — mirrored exactly below.
//!
//! Vertically, `anchor` selects which edge of the safe-margin box the
//! caption's own box is flush against (`Top`/`Bottom`/`Center`, matching
//! `CaptionOverlay.svelte`'s `justify-content: flex-start/flex-end/center`),
//! with `offset_y` a further pixel nudge on top of that anchored position.
//! Because ffmpeg cannot know a caption's real rendered pixel size
//! (`text_w`/`text_h`) until its own FreeType layout pass runs, position is
//! expressed as an ffmpeg `x`/`y` *expression* referencing `w`/`h`/`text_w`/
//! `text_h` (the same "let ffmpeg's own expression evaluator resolve
//! something only known at filter-run-time" idiom `render::plan` already
//! uses for `rotate=`'s `ow=rotw(angle)`), not a Rust-computed pixel number.
//!
//! `shadowx`/`shadowy` are the one exception: `ffmpeg -h filter=drawtext`
//! documents both as plain `<int>` options (unlike `x`/`y`, which are
//! `<string>` and accept expressions) — so, unlike the caption's own
//! position, a shadow's half-canvas-unit offset (`CaptionShadow::offset_x/y`,
//! the same convention as `CaptionPosition`) must be resolved to a concrete
//! pixel integer at plan-build time, using the real canvas dimensions this
//! function is given (`RenderGraph::canvas`, already known — no FreeType
//! layout pass is needed to know a shadow's *offset*, only a caption's own
//! rendered bounding box).

use crate::project::{CaptionAlignment, CaptionAnchor, CaptionStyle, Color};

fn fmt_secs(us: i64) -> String {
    format!("{:.6}", us as f64 / 1_000_000.0)
}

/// Escapes a raw string for direct, shell-free embedding as a `drawtext`
/// filter *option value* inside one already-assembled `-filter_complex`
/// argument (an `execve` argv element — no shell ever parses this, so no
/// third escaping level per ffmpeg's own docs is needed, only the two
/// documented "filtergraph escaping" levels below).
///
/// Verified against a real ffmpeg build (this project's own dev/CI ffmpeg
/// and the bundled Windows sidecar share the same drawtext implementation)
/// by exercising ffmpeg's own `-v debug` "Setting 'text' to value ..." log
/// line and a real rendered-frame pixel check, not assumed from the docs
/// alone — see this phase's `STUDIO_PLAN.md` writeup for the exact
/// escaping-algorithm derivation this function implements, straight from
/// ffmpeg's own "Notes on filtergraph escaping" section
/// (`doc/filters.texi`): a raw `\` needs BOTH the option-value level
/// (`\` -> `\\`) AND the filter-description level (each of those two
/// backslashes doubled again) applied, landing on 4 backslashes; a raw `'`
/// or `:` needs the same double application; `,`/`;`/`[`/`]` are only
/// special at the filter-description level, so a single backslash suffices.
/// A real embedded newline byte (`\n`, not the two-character escape
/// sequence — ffmpeg's own drawtext does NOT interpret a literal `\n`
/// two-char sequence as a line break, verified) is intentionally left
/// untouched: it is not a special character to either escaping level, and
/// FreeType's own line-breaking already renders it as a genuine new line
/// (verified: a caption's raw multi-line text renders as real, separate
/// lines with no escaping needed at all).
pub fn escape_drawtext_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '\\' => out.push_str("\\\\\\\\"),
            '\'' => out.push_str("\\\\\\'"),
            ':' => out.push_str("\\\\:"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '\r' => {} // normalize CRLF -> LF; the following '\n' still renders as a real line break
            _ => out.push(c),
        }
    }
    out
}

/// `project::Color` ([0.0, 1.0] per channel) to an ffmpeg `0xRRGGBB` color
/// literal, matching the hex-conversion convention `capcut::caption_style`'s
/// `background_from_caption_background` already uses for the same `Color`
/// type (round-half-up per channel, clamped).
fn color_hex(c: Color) -> String {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("0x{:02X}{:02X}{:02X}", to_u8(c.r), to_u8(c.g), to_u8(c.b))
}

/// `0xRRGGBB@alpha` — ffmpeg colors support this alpha suffix natively
/// (verified against a real ffmpeg build).
fn color_hex_alpha(c: Color, alpha: f64) -> String {
    format!("{}@{:.4}", color_hex(c), alpha.clamp(0.0, 1.0))
}

/// Fontconfig pattern for `drawtext`'s `font=` option: `<family>` alone, or
/// `<family>:style=<Bold ,Italic>` when the style calls for either — see
/// module doc comment's "Honest scope: font resolution" for what this does
/// and does not guarantee. The family name itself is escaped the same way
/// caption text is (a user-authored `font_family` could contain a `:` or
/// `'` just as easily as caption text can).
fn font_pattern(style: &CaptionStyle) -> String {
    let weights = match (style.bold, style.italic) {
        (true, true) => Some("Bold Italic"),
        (true, false) => Some("Bold"),
        (false, true) => Some("Italic"),
        (false, false) => None,
    };
    // Build the raw (unescaped) fontconfig pattern first, then run the
    // *whole* thing through the same escaper caption text gets — the `:'
    // before `style=` needs exactly the same double-backslash treatment as
    // any other literal colon embedded directly in this filter description
    // (verified against a real ffmpeg build: a single backslash here is a
    // real parse error, "Option not found: style" — the colon splits the
    // option list before `escape_drawtext_text`'s doubled backslash would
    // fully protect it).
    let raw = match weights {
        Some(w) => format!("{}:style={w}", style.font_family),
        None => style.font_family.clone(),
    };
    escape_drawtext_text(&raw)
}

/// The horizontal centering coefficient shared by every anchor (`k` in
/// `x = k*w - text_w/2`) — `CaptionOverlay.svelte`'s safe-area box is always
/// horizontally centered (`align-items: center`) regardless of `anchor`,
/// with `offset_x` a further half-canvas-unit nudge on top (see module doc
/// comment's derivation).
fn x_coefficient(style: &CaptionStyle) -> f64 {
    let m = &style.safe_margins;
    m.left + (1.0 - m.left - m.right) / 2.0 + style.position.offset_x / 2.0
}

/// The vertical expression for this style's `anchor` — `(coefficient, text_h
/// multiplier)` such that `y = coefficient*h - multiplier*text_h` (see
/// module doc comment's derivation; `Top` has multiplier 0, i.e. no `text_h`
/// term at all).
fn y_expr(style: &CaptionStyle) -> String {
    let m = &style.safe_margins;
    let offset = style.position.offset_y / 2.0;
    match style.position.anchor {
        CaptionAnchor::Top => {
            let k = m.top + offset;
            format!("{k:.6}*h")
        }
        CaptionAnchor::Bottom => {
            let k = 1.0 - m.bottom + offset;
            format!("{k:.6}*h-text_h")
        }
        CaptionAnchor::Center => {
            let k = m.top + (1.0 - m.top - m.bottom) / 2.0 + offset;
            format!("{k:.6}*h-text_h/2")
        }
    }
}

fn text_align_flag(alignment: CaptionAlignment) -> &'static str {
    match alignment {
        CaptionAlignment::Left => "left",
        CaptionAlignment::Center => "center",
        CaptionAlignment::Right => "right",
    }
}

/// Builds one caption's complete `drawtext=...` filter descriptor (no
/// surrounding `[in]`/`[out]` labels — `render::plan` wraps this the same
/// way it wraps every other per-node filter chain), time-windowed via
/// `enable='between(t,start,end)'` — this codebase's own established
/// time-windowing convention (`render::plan`'s per-clip `overlay=` filters).
/// `canvas_width`/`canvas_height` are only needed to resolve a shadow's
/// pixel offset (`shadowx`/`shadowy` are plain ints, unlike `x`/`y` — see
/// module doc comment); the caption's own `x`/`y` position stays a
/// resolution-independent expression.
///
/// `expansion=none` is always set: this caption's timing is already handled
/// by the `enable=` window above, not by any of `drawtext`'s own `%{...}`
/// expansion syntax (timecodes, frame numbers, etc.) — disabling expansion
/// entirely means a caption whose real text happens to contain a literal
/// `%` (e.g. "50% off") renders that `%` literally instead of ffmpeg logging
/// a "Stray %" warning and truncating the text at that point (verified
/// against a real ffmpeg build: a lone `%` under the default `expansion=
/// normal` silently drops everything from the `%` onward).
pub fn caption_drawtext_filter(
    text: &str,
    style: &CaptionStyle,
    canvas_width: u32,
    canvas_height: u32,
    start_us: i64,
    end_us: i64,
) -> String {
    let mut opts: Vec<String> = Vec::new();
    opts.push(format!("text={}", escape_drawtext_text(text)));
    opts.push(format!("font={}", font_pattern(style)));
    opts.push(format!("fontsize={:.2}", style.font_size));
    opts.push(format!("fontcolor={}", color_hex(style.text_color)));
    opts.push(format!("x={:.6}*w-text_w/2", x_coefficient(style)));
    opts.push(format!("y={}", y_expr(style)));
    opts.push(format!("text_align={}", text_align_flag(style.alignment)));
    opts.push(format!("alpha={:.4}", style.opacity.clamp(0.0, 1.0)));
    opts.push("expansion=none".to_string());

    if let Some(bg) = &style.background {
        opts.push("box=1".to_string());
        opts.push(format!(
            "boxcolor={}",
            color_hex_alpha(bg.color, bg.opacity)
        ));
        opts.push(format!(
            "boxborderw={}",
            (style.font_size * 0.2).round() as i64
        ));
    }
    if let Some(outline) = &style.outline {
        opts.push(format!("bordercolor={}", color_hex(outline.color)));
        opts.push(format!(
            "borderw={}",
            (outline.width * style.font_size).round().max(0.0) as i64
        ));
    }
    if let Some(shadow) = &style.shadow {
        // `blur`/diffuse has no drawtext equivalent — see module doc
        // comment's "Honest scope: shadow blur".
        opts.push(format!(
            "shadowcolor={}",
            color_hex_alpha(shadow.color, shadow.opacity)
        ));
        opts.push(format!(
            "shadowx={}",
            (shadow.offset_x * canvas_width as f64 / 2.0).round() as i64
        ));
        opts.push(format!(
            "shadowy={}",
            (shadow.offset_y * canvas_height as f64 / 2.0).round() as i64
        ));
    }

    opts.push(format!(
        "enable='between(t,{},{})'",
        fmt_secs(start_us),
        fmt_secs(end_us)
    ));

    format!("drawtext={}", opts.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        CaptionBackground, CaptionOutline, CaptionPosition, CaptionShadow, SafeMargins,
    };

    const CANVAS_W: u32 = 1920;
    const CANVAS_H: u32 = 1080;

    fn base_style() -> CaptionStyle {
        CaptionStyle {
            id: "s1".into(),
            name: "Test".into(),
            font_family: "Arial".into(),
            font_size: 40.0,
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

    #[test]
    fn escape_handles_colon_apostrophe_backslash_comma_semicolon_brackets() {
        let raw = "it's: a test, with [brackets]; semicolon\\backslash";
        let escaped = escape_drawtext_text(raw);
        // Exact byte-for-byte expected output per ffmpeg's own documented
        // two-level filtergraph escaping (doc/filters.texi's "Notes on
        // filtergraph escaping"), verified against a real ffmpeg build.
        assert_eq!(
            escaped,
            "it\\\\\\'s\\\\: a test\\, with \\[brackets\\]\\; semicolon\\\\\\\\backslash"
        );
    }

    #[test]
    fn escape_leaves_a_real_newline_byte_untouched() {
        let raw = "line1\nline2";
        let escaped = escape_drawtext_text(raw);
        assert_eq!(escaped, "line1\nline2");
    }

    #[test]
    fn escape_strips_carriage_return_but_keeps_the_newline() {
        let raw = "line1\r\nline2";
        let escaped = escape_drawtext_text(raw);
        assert_eq!(escaped, "line1\nline2");
    }

    #[test]
    fn a_caption_with_special_characters_still_produces_a_well_formed_single_drawtext_filter() {
        let style = base_style();
        let filt = caption_drawtext_filter(
            "5:00 PM - it's showtime, folks!",
            &style,
            CANVAS_W,
            CANVAS_H,
            0,
            2_000_000,
        );
        // Well-formed: exactly one `drawtext=` filter, option list is
        // colon-joined key=value pairs with no unescaped stray colons from
        // the caption text itself splitting it into extra bogus options.
        assert!(filt.starts_with("drawtext="));
        assert_eq!(filt.matches("drawtext=").count(), 1);
        assert!(filt.contains("text=5\\\\:00 PM - it\\\\\\'s showtime\\, folks!"));
        assert!(filt.contains("enable='between(t,0.000000,2.000000)'"));
    }

    #[test]
    fn no_style_field_is_silently_dropped_every_real_property_maps_to_a_drawtext_option() {
        let style = CaptionStyle {
            font_family: "Impact".into(),
            font_size: 48.0,
            bold: true,
            italic: true,
            alignment: CaptionAlignment::Left,
            text_color: Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
            },
            background: Some(CaptionBackground {
                color: Color::BLACK,
                opacity: 0.6,
            }),
            outline: Some(CaptionOutline {
                color: Color::BLACK,
                width: 0.08,
            }),
            shadow: Some(CaptionShadow {
                color: Color::BLACK,
                opacity: 0.5,
                offset_x: 0.01,
                offset_y: 0.01,
                blur: 15.0,
            }),
            opacity: 0.75,
            ..base_style()
        };
        let filt = caption_drawtext_filter("hello", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);

        assert!(filt.contains("font=Impact\\\\:style=Bold Italic"));
        assert!(filt.contains("fontsize=48.00"));
        assert!(filt.contains("fontcolor=0xFF0000"));
        assert!(filt.contains("text_align=left"));
        assert!(filt.contains("alpha=0.7500"));
        assert!(filt.contains("box=1"));
        assert!(filt.contains("boxcolor=0x000000@0.6000"));
        assert!(filt.contains(&format!("boxborderw={}", (48.0_f64 * 0.2).round() as i64)));
        assert!(filt.contains("bordercolor=0x000000"));
        assert!(filt.contains(&format!("borderw={}", (0.08_f64 * 48.0).round() as i64)));
        assert!(filt.contains("shadowcolor=0x000000@0.5000"));
        assert!(filt.contains("shadowx="));
        assert!(filt.contains("shadowy="));
    }

    #[test]
    fn no_background_outline_or_shadow_emits_none_of_their_options() {
        let style = base_style(); // background/outline/shadow all None
        let filt = caption_drawtext_filter("hello", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        assert!(!filt.contains("box="));
        assert!(!filt.contains("boxcolor="));
        assert!(!filt.contains("bordercolor="));
        assert!(!filt.contains("borderw="));
        assert!(!filt.contains("shadowcolor="));
    }

    #[test]
    fn bold_and_italic_both_off_omits_the_style_suffix_from_the_font_pattern() {
        let style = base_style();
        let filt = caption_drawtext_filter("hello", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        assert!(filt.contains("font=Arial:"));
        assert!(!filt.contains("style="));
    }

    #[test]
    fn top_anchor_y_expression_has_no_text_h_term() {
        let mut style = base_style();
        style.position.anchor = CaptionAnchor::Top;
        let filt = caption_drawtext_filter("hi", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        assert!(filt.contains("y=0.050000*h:")); // top safe margin default 0.05, offset 0
    }

    #[test]
    fn bottom_anchor_y_expression_subtracts_full_text_h() {
        let style = base_style(); // default anchor is Bottom
        let filt = caption_drawtext_filter("hi", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        assert!(filt.contains("y=0.950000*h-text_h:"));
    }

    #[test]
    fn center_anchor_y_expression_subtracts_half_text_h() {
        let mut style = base_style();
        style.position.anchor = CaptionAnchor::Center;
        let filt = caption_drawtext_filter("hi", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        assert!(filt.contains("y=0.500000*h-text_h/2:"));
    }

    #[test]
    fn x_expression_is_always_centered_within_the_safe_margin_box_regardless_of_anchor() {
        let style = base_style(); // left=right=0.05 default -> centered at 0.5
        let filt = caption_drawtext_filter("hi", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        assert!(filt.contains("x=0.500000*w-text_w/2:"));
    }

    #[test]
    fn offset_x_and_offset_y_shift_the_position_expressions() {
        let mut style = base_style();
        style.position.offset_x = 0.2;
        style.position.offset_y = 0.1;
        let filt = caption_drawtext_filter("hi", &style, CANVAS_W, CANVAS_H, 0, 1_000_000);
        // k_x = 0.5 (base center) + 0.2/2 = 0.6
        assert!(filt.contains("x=0.600000*w-text_w/2:"));
        // bottom anchor: k_y = 0.95 (base) + 0.1/2 = 1.0
        assert!(filt.contains("y=1.000000*h-text_h:"));
    }

    #[test]
    fn enable_window_uses_the_captions_own_start_and_end_time() {
        let style = base_style();
        let filt = caption_drawtext_filter("hi", &style, CANVAS_W, CANVAS_H, 1_500_000, 4_250_000);
        assert!(filt.contains("enable='between(t,1.500000,4.250000)'"));
    }
}
