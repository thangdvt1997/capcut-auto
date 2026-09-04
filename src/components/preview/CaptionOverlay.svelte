<!--
  Karaoke/active-word caption overlay (master prompt §27), mounted inside
  `VideoPlayer.svelte`'s `.canvas-frame` so it sits directly on top of the
  actual preview frame at the same aspect-ratio-locked box.

  Efficiency model (see `stores/captions.svelte.ts`/`captions/karaoke.ts`
  doc comments for the full rationale): this component never generates one
  DOM node per word. It reads exactly two small `$derived` primitives off
  `captionsStore` — the active caption's id and the active word's index —
  plus the (reference-stable, only changes when the active caption itself
  changes) `Caption` object, and renders the caption's text as three static
  string segments (before / active / after the current word) inside ONE
  text node, splitting only at the two word-boundary indices that matter
  right now. The whole tree here re-renders only when the active caption or
  the active word index actually changes (Svelte 5's own `$derived`
  dependency tracking already skips unnecessary re-runs when a read value's
  identity is unchanged) — not on every `timeline.playheadUs` tick.

  Position/size mapping (approximate, not byte-for-byte CapCut/backend
  renderer parity — Phase 9's CapCut adapter owns the exact conversion for
  export): `CaptionPosition.offset_x/offset_y` are "half-canvas-width/height
  units" (`project::types::CaptionPosition` doc comment) — converted here to
  pixels via the measured on-screen frame size (`rootW`/`rootH` below) so a
  value of `1.0` always means "half the canvas' own width/height", matching
  the documented convention regardless of the preview's current on-screen
  size. `CaptionStyle.safe_margins` are rendered as a percentage inset from
  each edge that the caption box's centering/anchoring region stays within.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { colorToCss } from "../../captions/styleCatalog";

  let rootW = $state(0);
  let rootH = $state(0);

  // Reference-stable across playhead ticks that don't change the active
  // caption (see `stores/captions.svelte.ts::activeCaption`'s own doc
  // comment) — reading it here does not defeat that, it only means this
  // component's own re-render is gated on the same identity check.
  let caption = $derived(captionsStore.activeCaption);
  let wordIndex = $derived(captionsStore.activeWordIndex);
  let style = $derived(captionsStore.activeCaptionStyle);

  let segments = $derived.by((): { before: string; active: string; after: string } | null => {
    const c = caption;
    if (!c) return null;
    if (wordIndex < 0 || c.words.length === 0) {
      return { before: c.text, active: "", after: "" };
    }
    const before = c.words
      .slice(0, wordIndex)
      .map((w) => w.text)
      .join(" ");
    const active = c.words[wordIndex]?.text ?? "";
    const after = c.words
      .slice(wordIndex + 1)
      .map((w) => w.text)
      .join(" ");
    return { before, active, after };
  });

  function toPx(unit: number, dimPx: number): number {
    // "half-canvas-width/height units" — see file doc comment.
    return (unit * dimPx) / 2;
  }

  let justify = $derived(
    style.position.anchor === "top" ? "flex-start" : style.position.anchor === "bottom" ? "flex-end" : "center",
  );
  let translateX = $derived(toPx(style.position.offset_x, rootW));
  let translateY = $derived(toPx(style.position.offset_y, rootH));

  let outlineStyle = $derived(
    style.outline ? `-webkit-text-stroke: ${(style.outline.width * style.font_size).toFixed(2)}px ${colorToCss(style.outline.color)};` : "",
  );
  let shadowStyle = $derived(
    style.shadow
      ? `text-shadow: ${toPx(style.shadow.offset_x, rootW).toFixed(1)}px ${toPx(style.shadow.offset_y, rootH).toFixed(1)}px ${style.shadow.blur}px ${colorToCss(style.shadow.color, style.shadow.opacity)};`
      : "",
  );
  let backgroundStyle = $derived(
    style.background ? `background: ${colorToCss(style.background.color, style.background.opacity)}; padding: 0.15em 0.5em; border-radius: 0.2em;` : "",
  );
</script>

<div class="ov-root" bind:clientWidth={rootW} bind:clientHeight={rootH}>
  {#if segments}
    <div
      class="ov-safe"
      style:top="{style.safe_margins.top * 100}%"
      style:bottom="{style.safe_margins.bottom * 100}%"
      style:left="{style.safe_margins.left * 100}%"
      style:right="{style.safe_margins.right * 100}%"
      style:justify-content={justify}
    >
      <div
        class="ov-box"
        style="{outlineStyle}{shadowStyle}"
        style:transform="translate({translateX}px, {translateY}px)"
        style:text-align={style.alignment}
        style:font-family={style.font_family}
        style:font-size="{style.font_size}px"
        style:font-weight={style.bold ? "700" : "400"}
        style:font-style={style.italic ? "italic" : "normal"}
        style:color={colorToCss(style.text_color)}
        style:opacity={style.opacity}
      >
        <span class="ov-text" style="{backgroundStyle}">{segments.before}{segments.before ? " " : ""}<span
            class="ov-active">{segments.active}</span>{segments.after ? " " : ""}{segments.after}</span>
      </div>
    </div>
  {/if}
</div>

<style>
  .ov-root {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .ov-safe {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    overflow: hidden;
  }
  .ov-box {
    max-width: 100%;
    line-height: 1.3;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .ov-text {
    display: inline;
  }
  .ov-active {
    color: hsl(45 96% 60%);
    font-weight: 700;
  }
</style>
