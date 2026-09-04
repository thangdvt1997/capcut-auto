// Pure helpers over `CaptionStyle` (master prompt §26's exact field list) —
// the "catalog" a caption's `style_id` resolves against is the concatenation
// of the six read-only built-in templates (`list_caption_templates`) and the
// current project's own editable `caption_styles` (`set_caption_styles`).
// Neither the backend nor `Caption::style_id` itself validates that a
// `style_id` actually resolves to something (see
// `src-tauri/src/timeline/captions.rs::bulk_set_caption_style` — it stores
// whatever string it's given) — resolution is entirely a frontend rendering
// concern, done here rather than duplicated in every component that needs a
// caption's effective style.

import type { CaptionStyle } from "../types/bindings";

/** Used whenever a `Caption` has `style_id: null` (or one that doesn't
 * resolve against the current catalog) — plain white centered text, no
 * background/outline/shadow, matching a bare `<video>`'s native captions
 * look rather than silently rendering nothing. Not persisted anywhere; a
 * pure fallback value. */
export const FALLBACK_CAPTION_STYLE: CaptionStyle = {
  id: "__fallback__",
  name: "Default",
  font_family: "system-ui",
  font_size: 32,
  bold: false,
  italic: false,
  alignment: "center",
  position: { anchor: "bottom", offset_x: 0, offset_y: 0 },
  text_color: { r: 1, g: 1, b: 1 },
  background: null,
  outline: null,
  shadow: null,
  opacity: 1,
  safe_margins: { top: 0.05, bottom: 0.05, left: 0.05, right: 0.05 },
};

/** `templates` first, then the project's own custom styles — built-ins are
 * always offered even in a brand-new project with an empty
 * `caption_styles` catalog. IDs are assumed unique across both lists (the
 * six built-in ids are the stable `template_*` ids `styles.rs` documents;
 * custom styles get a fresh `crypto.randomUUID()`-derived id, see
 * `stores/captions.svelte.ts::saveDraftAsProjectStyle`), so a plain
 * concatenation is enough — no de-duplication pass needed. */
export function buildStyleCatalog(templates: CaptionStyle[], projectStyles: CaptionStyle[]): CaptionStyle[] {
  return [...templates, ...projectStyles];
}

export function resolveCaptionStyle(catalog: readonly CaptionStyle[], styleId: string | null): CaptionStyle {
  if (styleId === null) return FALLBACK_CAPTION_STYLE;
  return catalog.find((s) => s.id === styleId) ?? FALLBACK_CAPTION_STYLE;
}

/** Deep value equality for the "has the draft been edited since it was
 * loaded/saved" dirty check (`stores/captions.svelte.ts`) — `CaptionStyle`
 * is plain JSON-serializable data (no functions/dates/cycles), so
 * `JSON.stringify` comparison is exact and far simpler than a hand-rolled
 * structural walk. */
export function stylesEqual(a: CaptionStyle, b: CaptionStyle): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function cloneStyle(style: CaptionStyle): CaptionStyle {
  return structuredClone(style);
}

/** `r/g/b` in `Color`'s native `[0,1]` linear range (matching capcut-mate's
 * own convention, see `project::types::Color` doc comment) to a CSS
 * `rgb()`/`rgba()` color string. */
export function colorToCss(color: { r: number; g: number; b: number }, alpha = 1): string {
  const to255 = (c: number) => Math.round(Math.max(0, Math.min(1, c)) * 255);
  return `rgba(${to255(color.r)}, ${to255(color.g)}, ${to255(color.b)}, ${alpha})`;
}

export function cssColorToRgb01(hex: string): { r: number; g: number; b: number } {
  const m = /^#?([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$/.exec(hex);
  if (!m) return { r: 1, g: 1, b: 1 };
  return {
    r: parseInt(m[1]!, 16) / 255,
    g: parseInt(m[2]!, 16) / 255,
    b: parseInt(m[3]!, 16) / 255,
  };
}

export function rgb01ToCssHex(color: { r: number; g: number; b: number }): string {
  const to2 = (c: number) =>
    Math.round(Math.max(0, Math.min(1, c)) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${to2(color.r)}${to2(color.g)}${to2(color.b)}`;
}
