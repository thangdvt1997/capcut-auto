<!--
  Templates panel (master prompt §36/§37). Mounted as `LeftPanel.svelte`'s
  real "Templates" tab — see `stores/templates.svelte.ts`'s class doc
  comment for the full "what Apply actually pushes into the live project vs.
  what it only pre-fills" design decision, and for Save as Template's
  matching capture logic.
-->
<script lang="ts">
  import { templatesStore, ALL_SMART_EDIT_CATEGORIES } from "../../stores/templates.svelte";
  import { captionsStore } from "../../stores/captions.svelte";
  import { renderStore } from "../../stores/render.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { TransitionType, ZoomIntensity } from "../../types/bindings";

  const ZOOM_INTENSITIES: ZoomIntensity[] = ["off", "low", "medium", "high"];
  const TRANSITION_TYPES: TransitionType[] = ["cut", "cross_fade"];

  function presetName(id: string): string {
    return renderStore.presets.find((p) => p.id === id)?.name ?? id;
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      templatesStore.closeSaveForm();
    }
  }
</script>

<div class="tp-panel">
  <div class="tp-toolbar">
    <button class="btn btn-ghost btn-sm" disabled={templatesStore.importing} onclick={() => void templatesStore.importTemplate()}>
      {templatesStore.importing ? t("templatesPanel.importing") : t("templatesPanel.importButton")}
    </button>
    <button class="btn btn-ghost btn-sm" disabled={!timeline.project} onclick={() => templatesStore.openSaveForm()}>
      {t("templatesPanel.saveAsButton")}
    </button>
  </div>

  {#if templatesStore.importError}
    <div class="tp-error">{templatesStore.importError}</div>
  {/if}
  {#if templatesStore.exportError}
    <div class="tp-error">{templatesStore.exportError}</div>
  {/if}
  {#if templatesStore.deleteError}
    <div class="tp-error">{templatesStore.deleteError}</div>
  {/if}
  {#if templatesStore.applyError}
    <div class="tp-error">{templatesStore.applyError}</div>
  {/if}
  {#if templatesStore.lastAppliedName}
    <div class="tp-note">{t("templatesPanel.appliedNote", { name: templatesStore.lastAppliedName })}</div>
  {/if}
  {#if templatesStore.loadError}
    <div class="tp-error">{templatesStore.loadError}</div>
  {/if}

  <div class="tp-body">
    {#if templatesStore.loading && templatesStore.allTemplates.length === 0}
      <p class="tp-empty muted-2">{t("templatesPanel.loading")}</p>
    {/if}

    <h3 class="tp-group-title">{t("templatesPanel.builtInSectionTitle")}</h3>
    <div class="tp-grid">
      {#each templatesStore.catalog.built_in as tmpl (tmpl.id)}
        {@const card = tmpl}
        <div class="tp-card">
          <div class="tp-card-header">
            <span class="tp-name">{card.name}</span>
            <span class="tp-badge">{t("templatesPanel.builtInBadge")}</span>
          </div>
          <p class="tp-desc muted-2">{card.description}</p>
          <div class="tp-meta">
            <span class="tp-meta-item mono">{card.canvas.ratio_preset}</span>
            <span class="tp-meta-item">{card.caption_style.name}</span>
            <span class="tp-meta-item">{t(`autoZoom.intensity.${card.zoom_intensity}`)}</span>
            <span class="tp-meta-item">{presetName(card.export_preset_id)}</span>
          </div>
          <div class="tp-card-actions">
            <button
              class="btn btn-sm"
              disabled={!timeline.project || templatesStore.applyingId === card.id}
              onclick={() => void templatesStore.applyToProject(card)}
            >
              {templatesStore.applyingId === card.id ? t("templatesPanel.applying") : t("templatesPanel.applyButton")}
            </button>
            <button
              class="btn btn-ghost btn-sm"
              disabled={templatesStore.exportingId === card.id}
              onclick={() => void templatesStore.exportTemplate(card)}
            >
              {templatesStore.exportingId === card.id ? t("templatesPanel.exporting") : t("templatesPanel.exportButton")}
            </button>
          </div>
        </div>
      {/each}
    </div>

    <h3 class="tp-group-title">{t("templatesPanel.customSectionTitle")}</h3>
    {#if templatesStore.catalog.custom.length === 0}
      <p class="tp-empty muted-2">{t("templatesPanel.noCustomTemplates")}</p>
    {:else}
      <div class="tp-grid">
        {#each templatesStore.catalog.custom as tmpl (tmpl.id)}
          {@const card = tmpl}
          <div class="tp-card">
            <div class="tp-card-header">
              <span class="tp-name">{card.name}</span>
              <span class="tp-badge tp-badge-custom">{t("templatesPanel.customBadge")}</span>
            </div>
            <p class="tp-desc muted-2">{card.description}</p>
            <div class="tp-meta">
              <span class="tp-meta-item mono">{card.canvas.ratio_preset}</span>
              <span class="tp-meta-item">{card.caption_style.name}</span>
              <span class="tp-meta-item">{t(`autoZoom.intensity.${card.zoom_intensity}`)}</span>
              <span class="tp-meta-item">{presetName(card.export_preset_id)}</span>
            </div>
            <div class="tp-card-actions">
              <button
                class="btn btn-sm"
                disabled={!timeline.project || templatesStore.applyingId === card.id}
                onclick={() => void templatesStore.applyToProject(card)}
              >
                {templatesStore.applyingId === card.id ? t("templatesPanel.applying") : t("templatesPanel.applyButton")}
              </button>
              <button
                class="btn btn-ghost btn-sm"
                disabled={templatesStore.exportingId === card.id}
                onclick={() => void templatesStore.exportTemplate(card)}
              >
                {templatesStore.exportingId === card.id ? t("templatesPanel.exporting") : t("templatesPanel.exportButton")}
              </button>
              {#if templatesStore.pendingDeleteId === card.id}
                <button class="btn btn-danger btn-sm" disabled={templatesStore.deletingId === card.id} onclick={() => void templatesStore.confirmDelete(card.id)}>
                  {t("templatesPanel.deleteConfirmButton")}
                </button>
                <button class="btn btn-ghost btn-sm" onclick={() => templatesStore.cancelDelete()}>{t("templatesPanel.deleteCancelButton")}</button>
              {:else}
                <button class="btn btn-ghost btn-sm" onclick={() => templatesStore.armDelete(card.id)}>{t("templatesPanel.deleteButton")}</button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

{#if templatesStore.saveFormOpen}
  <div class="tp-backdrop" role="presentation" onclick={() => templatesStore.closeSaveForm()}>
    <div
      class="tp-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("templatesPanel.saveFormTitle")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="tp-dialog-header">
        <span class="tp-dialog-title">{t("templatesPanel.saveFormTitle")}</span>
        <button class="btn btn-ghost" onclick={() => templatesStore.closeSaveForm()}>×</button>
      </div>
      <div class="tp-dialog-body">
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-name">{t("templatesPanel.nameLabel")}</label>
          <input id="tp-name" class="tp-input" type="text" bind:value={templatesStore.saveName} />
        </div>
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-desc">{t("templatesPanel.descriptionLabel")}</label>
          <input id="tp-desc" class="tp-input" type="text" bind:value={templatesStore.saveDescription} />
        </div>
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-caption-style">{t("templatesPanel.captionStyleLabel")}</label>
          <select id="tp-caption-style" class="tp-input" bind:value={templatesStore.saveCaptionStyleId}>
            {#each captionsStore.catalog as style (style.id)}
              <option value={style.id}>{style.name}</option>
            {/each}
          </select>
        </div>
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-zoom">{t("templatesPanel.zoomIntensityLabel")}</label>
          <select id="tp-zoom" class="tp-input" bind:value={templatesStore.saveZoomIntensity}>
            {#each ZOOM_INTENSITIES as i (i)}
              <option value={i}>{t(`autoZoom.intensity.${i}`)}</option>
            {/each}
          </select>
        </div>
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-preset">{t("templatesPanel.exportPresetLabel")}</label>
          <select id="tp-preset" class="tp-input" bind:value={templatesStore.saveExportPresetId}>
            {#each renderStore.presets as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
        </div>
        <div class="tp-field-row">
          <span class="tp-field-label">{t("templatesPanel.silenceSettingsLabel")}</span>
          <span class="tp-static-value muted-2">{t("templatesPanel.silenceSettingsFromDetector")}</span>
        </div>
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-transition">{t("templatesPanel.transitionLabel")}</label>
          <select id="tp-transition" class="tp-input tp-input-narrow" bind:value={templatesStore.saveTransitionType}>
            {#each TRANSITION_TYPES as tt (tt)}
              <option value={tt}>{t(`templatesPanel.transitionType.${tt}`)}</option>
            {/each}
          </select>
          {#if templatesStore.saveTransitionType === "cross_fade"}
            <input class="tp-input tp-input-narrow" type="number" min="0" step="10" bind:value={templatesStore.saveTransitionDurationMs} />
            <span class="muted-2">ms</span>
          {/if}
        </div>
        <div class="tp-field-row tp-field-row-wrap">
          <span class="tp-field-label">{t("templatesPanel.aiCategoriesLabel")}</span>
          <div class="tp-category-list">
            {#each ALL_SMART_EDIT_CATEGORIES as cat (cat)}
              <label class="tp-check">
                <input
                  type="checkbox"
                  checked={templatesStore.saveEmphasizedCategories.has(cat)}
                  onchange={() => templatesStore.toggleEmphasizedCategory(cat)}
                />
                {t(`smartEdit.category.${cat}`)}
              </label>
            {/each}
          </div>
        </div>
        <div class="tp-field-row">
          <label class="tp-field-label" for="tp-prompt-prefix">{t("templatesPanel.systemPromptPrefixLabel")}</label>
          <input id="tp-prompt-prefix" class="tp-input" type="text" bind:value={templatesStore.saveSystemPromptPrefix} />
        </div>
        <label class="tp-check">
          <input type="checkbox" bind:checked={templatesStore.saveIncludeSportsOverlay} />
          {t("templatesPanel.includeSportsOverlayLabel")}
        </label>
        {#if templatesStore.saveError}
          <div class="tp-error">{templatesStore.saveError}</div>
        {/if}
      </div>
      <div class="tp-dialog-footer">
        <button class="btn" disabled={templatesStore.saving} onclick={() => void templatesStore.submitSave()}>
          {templatesStore.saving ? t("templatesPanel.saving") : t("templatesPanel.saveButton")}
        </button>
        <span class="tp-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => templatesStore.closeSaveForm()}>{t("templatesPanel.cancelButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .tp-panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    min-height: 0;
  }
  .tp-toolbar {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .btn-sm {
    height: 24px;
    padding: 0 8px;
    font-size: 10.5px;
  }
  .tp-error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .tp-note {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--pos);
    background: hsl(140 60% 50% / 0.08);
    border: 1px solid hsl(140 60% 50% / 0.3);
    border-radius: var(--radius-sm);
  }
  .tp-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tp-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .tp-group-title {
    margin: 6px 0 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .tp-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 8px;
  }
  .tp-card {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .tp-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .tp-name {
    font-size: 12px;
    font-weight: 600;
  }
  .tp-badge {
    font-size: 9.5px;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted);
    white-space: nowrap;
  }
  .tp-badge-custom {
    color: var(--accent);
    border-color: var(--accent);
  }
  .tp-desc {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .tp-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .tp-meta-item {
    font-size: 9.5px;
    padding: 1px 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--muted);
  }
  .tp-card-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.16);
    border: 1px solid hsl(0 84% 65% / 0.5);
    color: var(--foreground);
  }

  .tp-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .tp-dialog {
    width: min(480px, 94vw);
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
    overflow: hidden;
  }
  .tp-dialog-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tp-dialog-title {
    font-size: 13px;
    font-weight: 600;
  }
  .tp-dialog-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .tp-field-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .tp-field-row-wrap {
    align-items: flex-start;
  }
  .tp-field-label {
    font-size: 11px;
    color: var(--muted);
    width: 110px;
    flex-shrink: 0;
  }
  .tp-input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .tp-input-narrow {
    flex: 0 0 auto;
    width: 90px;
  }
  .tp-static-value {
    font-size: 11px;
  }
  .tp-category-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .tp-check {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    cursor: pointer;
  }
  .tp-dialog-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tp-footer-spacer {
    flex: 1;
  }
</style>
