<!--
  Caption styling panel (master prompt §26): a template picker (the six
  built-ins from `list_caption_templates`) plus override controls for every
  `CaptionStyle` field, mirroring `ExportDialog.svelte`'s preset-then-override
  pattern (`stores/render.svelte.ts`'s own doc comment). See
  `stores/captions.svelte.ts`'s class doc comment for the full seed -> edit
  -> save -> apply workflow and why "Apply to Selected" is disabled while
  the draft is dirty relative to the catalog entry it was saved under.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { colorToCss, cssColorToRgb01, rgb01ToCssHex } from "../../captions/styleCatalog";
  import { t } from "../../lib/i18n.svelte";

  let styleName = $state(captionsStore.draft.name);

  $effect(() => {
    // Resync the name buffer whenever a *different* catalog entry is loaded
    // into the draft (not on every keystroke — this only depends on
    // `draftSourceId`, not `draft.name` itself).
    void captionsStore.draftSourceId;
    styleName = captionsStore.draft.name;
  });

  function colorInputHandler(setter: (rgb: { r: number; g: number; b: number }) => void) {
    return (e: Event) => setter(cssColorToRgb01((e.currentTarget as HTMLInputElement).value));
  }
</script>

<div class="catalog-grid">
  {#each captionsStore.catalog as s (s.id)}
    <button
      class="catalog-card"
      class:selected={captionsStore.draftSourceId === s.id}
      onclick={() => captionsStore.editStyle(s.id)}
    >
      <span class="catalog-name">{s.name}</span>
      <span class="catalog-badge muted-2">
        {captionsStore.projectStyles.some((p) => p.id === s.id) ? t("captionsPanel.styleBadgeCustom") : t("captionsPanel.styleBadgeBuiltin")}
      </span>
    </button>
  {/each}
  {#if captionsStore.templatesLoading}
    <span class="muted-2">{t("captionsPanel.loadingTemplates")}</span>
  {/if}
</div>
{#if captionsStore.templatesError}
  <div class="cs-error">{captionsStore.templatesError}</div>
{/if}

<div class="save-row">
  <input class="text-input" type="text" bind:value={styleName} placeholder={t("captionsPanel.styleNamePlaceholder")} />
  <button class="btn btn-ghost" disabled={captionsStore.savingStyle} onclick={() => void captionsStore.saveDraftAsProjectStyle(styleName)}>
    {captionsStore.savingStyle ? t("captionsPanel.savingStyle") : t("captionsPanel.saveStyleButton")}
  </button>
  {#if captionsStore.draftDirty}
    <span class="dirty-hint muted-2">{t("captionsPanel.styleDirtyHint")}</span>
  {/if}
</div>
{#if captionsStore.styleError}
  <div class="cs-error">{captionsStore.styleError}</div>
{/if}

<div class="fields">
  <div class="field-row">
    <label class="field-label" for="cs-font">{t("captionsPanel.fontFamilyLabel")}</label>
    <input id="cs-font" class="text-input" type="text" bind:value={captionsStore.draft.font_family} />
  </div>
  <div class="field-row">
    <label class="field-label" for="cs-font-size">{t("captionsPanel.fontSizeLabel")}</label>
    <input id="cs-font-size" class="num" type="number" min="1" bind:value={captionsStore.draft.font_size} />
    <label class="check"><input type="checkbox" bind:checked={captionsStore.draft.bold} /> {t("captionsPanel.boldLabel")}</label>
    <label class="check"><input type="checkbox" bind:checked={captionsStore.draft.italic} /> {t("captionsPanel.italicLabel")}</label>
  </div>

  <div class="field-row">
    <label class="field-label" for="cs-align">{t("captionsPanel.alignmentLabel")}</label>
    <select id="cs-align" class="select" bind:value={captionsStore.draft.alignment}>
      <option value="left">{t("captionsPanel.alignLeft")}</option>
      <option value="center">{t("captionsPanel.alignCenter")}</option>
      <option value="right">{t("captionsPanel.alignRight")}</option>
    </select>
  </div>

  <div class="field-row">
    <label class="field-label" for="cs-anchor">{t("captionsPanel.anchorLabel")}</label>
    <select id="cs-anchor" class="select" bind:value={captionsStore.draft.position.anchor}>
      <option value="top">{t("captionsPanel.anchorTop")}</option>
      <option value="center">{t("captionsPanel.anchorCenter")}</option>
      <option value="bottom">{t("captionsPanel.anchorBottom")}</option>
    </select>
    <span class="muted-2">{t("captionsPanel.offsetXLabel")}</span>
    <input class="num" type="number" step="0.05" min="-1" max="1" bind:value={captionsStore.draft.position.offset_x} />
    <span class="muted-2">{t("captionsPanel.offsetYLabel")}</span>
    <input class="num" type="number" step="0.05" min="-1" max="1" bind:value={captionsStore.draft.position.offset_y} />
  </div>

  <div class="field-row">
    <label class="field-label" for="cs-color">{t("captionsPanel.textColorLabel")}</label>
    <input
      id="cs-color"
      class="color"
      type="color"
      value={rgb01ToCssHex(captionsStore.draft.text_color)}
      oninput={colorInputHandler((rgb) => (captionsStore.draft.text_color = rgb))}
    />
    <span class="muted-2">{t("captionsPanel.opacityLabel")}</span>
    <input class="num" type="number" step="0.05" min="0" max="1" bind:value={captionsStore.draft.opacity} />
  </div>

  <div class="field-row">
    <label class="check">
      <input type="checkbox" checked={!!captionsStore.draft.background} onchange={(e) => captionsStore.setBackgroundEnabled(e.currentTarget.checked)} />
      {t("captionsPanel.backgroundLabel")}
    </label>
    {#if captionsStore.draft.background}
      <input
        class="color"
        type="color"
        value={rgb01ToCssHex(captionsStore.draft.background.color)}
        oninput={colorInputHandler((rgb) => (captionsStore.draft.background!.color = rgb))}
      />
      <span class="muted-2">{t("captionsPanel.opacityLabel")}</span>
      <input class="num" type="number" step="0.05" min="0" max="1" bind:value={captionsStore.draft.background!.opacity} />
    {/if}
  </div>

  <div class="field-row">
    <label class="check">
      <input type="checkbox" checked={!!captionsStore.draft.outline} onchange={(e) => captionsStore.setOutlineEnabled(e.currentTarget.checked)} />
      {t("captionsPanel.outlineLabel")}
    </label>
    {#if captionsStore.draft.outline}
      <input
        class="color"
        type="color"
        value={rgb01ToCssHex(captionsStore.draft.outline.color)}
        oninput={colorInputHandler((rgb) => (captionsStore.draft.outline!.color = rgb))}
      />
      <span class="muted-2">{t("captionsPanel.outlineWidthLabel")}</span>
      <input class="num" type="number" step="0.01" min="0" max="1" bind:value={captionsStore.draft.outline!.width} />
    {/if}
  </div>

  <div class="field-row">
    <label class="check">
      <input type="checkbox" checked={!!captionsStore.draft.shadow} onchange={(e) => captionsStore.setShadowEnabled(e.currentTarget.checked)} />
      {t("captionsPanel.shadowLabel")}
    </label>
    {#if captionsStore.draft.shadow}
      <input
        class="color"
        type="color"
        value={rgb01ToCssHex(captionsStore.draft.shadow.color)}
        oninput={colorInputHandler((rgb) => (captionsStore.draft.shadow!.color = rgb))}
      />
      <span class="muted-2">{t("captionsPanel.opacityLabel")}</span>
      <input class="num" type="number" step="0.05" min="0" max="1" bind:value={captionsStore.draft.shadow!.opacity} />
      <span class="muted-2">{t("captionsPanel.blurLabel")}</span>
      <input class="num" type="number" step="1" min="0" max="100" bind:value={captionsStore.draft.shadow!.blur} />
    {/if}
  </div>

  <div class="field-row">
    <span class="field-label">{t("captionsPanel.safeMarginsLabel")}</span>
    <label class="margin-field">{t("captionsPanel.marginTop")}<input class="num" type="number" step="0.01" min="0" max="0.5" bind:value={captionsStore.draft.safe_margins.top} /></label>
    <label class="margin-field">{t("captionsPanel.marginBottom")}<input class="num" type="number" step="0.01" min="0" max="0.5" bind:value={captionsStore.draft.safe_margins.bottom} /></label>
    <label class="margin-field">{t("captionsPanel.marginLeft")}<input class="num" type="number" step="0.01" min="0" max="0.5" bind:value={captionsStore.draft.safe_margins.left} /></label>
    <label class="margin-field">{t("captionsPanel.marginRight")}<input class="num" type="number" step="0.01" min="0" max="0.5" bind:value={captionsStore.draft.safe_margins.right} /></label>
  </div>
</div>

<div class="preview-swatch" style:color={colorToCss(captionsStore.draft.text_color)} style:background={captionsStore.draft.background ? colorToCss(captionsStore.draft.background.color, captionsStore.draft.background.opacity) : "hsl(0 0% 10%)"} style:font-weight={captionsStore.draft.bold ? "700" : "400"} style:font-style={captionsStore.draft.italic ? "italic" : "normal"} style:opacity={captionsStore.draft.opacity}>
  {t("captionsPanel.previewSampleText")}
</div>

<div class="apply-row">
  <span class="muted-2">{t("captionsPanel.selectedCount", { count: captionsStore.selectedCaptionIds.size })}</span>
  <button
    class="btn"
    disabled={captionsStore.selectedCaptionIds.size === 0 || captionsStore.draftDirty || !captionsStore.draftSourceId}
    onclick={() => void captionsStore.applyStyleToSelected(Array.from(captionsStore.selectedCaptionIds))}
  >
    {t("captionsPanel.applyToSelectedButton")}
  </button>
</div>

<style>
  .catalog-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 6px;
  }
  .catalog-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .catalog-card:hover { border-color: var(--border-strong); }
  .catalog-card.selected { border-color: var(--accent); background: hsl(213 94% 68% / 0.08); }
  .catalog-name { font-size: 11px; font-weight: 600; }
  .catalog-badge { font-size: 9.5px; text-transform: uppercase; }
  .save-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .dirty-hint { font-size: 10.5px; }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
  }
  .field-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .field-label {
    font-size: 10.5px;
    color: var(--muted);
    min-width: 90px;
  }
  .text-input {
    flex: 1;
    min-width: 0;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
    padding: 0 6px;
  }
  .select {
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
  }
  .num {
    width: 64px;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
    padding: 0 6px;
  }
  .color {
    width: 32px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: none;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
  }
  .margin-field {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
  }
  .cs-error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .preview-swatch {
    margin-top: 10px;
    padding: 14px;
    border-radius: var(--radius-sm);
    text-align: center;
    font-size: 16px;
  }
  .apply-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
</style>
