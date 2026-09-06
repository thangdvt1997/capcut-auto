<!--
  AI Auto Template dialog (upgrade spec §7, `UPGRADE_PLAN.md` Phase U2).
  Same source track/clip picker shape as `HighlightDetectionDialog.svelte`/
  `AiCommandBox.svelte` — the panel always analyzes "the currently selected
  timeline clip's underlying media". Opened from `Timeline.svelte`'s toolbar,
  next to those two dialogs (see that component's own doc comment for the
  placement rationale).

  Shows the real recommendation (template name / reason / confidence /
  suggested aspect) as a card, then Accept / Change Template / Customize /
  Run — every one of which routes through an already-real mechanism
  (`stores/templates.svelte.ts`'s `applyToProject`/`openSaveForm`,
  `stores/render.svelte.ts`'s `openDialog`) rather than a second one. See
  `stores/autoTemplate.svelte.ts`'s own doc comment for the full design
  reasoning.
-->
<script lang="ts">
  import { autoTemplate } from "../../stores/autoTemplate.svelte";
  import { templatesStore } from "../../stores/templates.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Clip, ShortsAspect } from "../../types/bindings";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function clipLabel(clip: Clip): string {
    const media = clip.media_id ? timeline.mediaById.get(clip.media_id) : undefined;
    const name = media ? basename(media.source_path) : t("timelinePanel.clipEmptyLabel");
    return `${name} (${(clip.position_us / 1_000_000).toFixed(1)}s)`;
  }

  function aspectLabel(aspect: ShortsAspect): string {
    switch (aspect) {
      case "vertical_9x_16":
        return "9:16";
      case "square_1x_1":
        return "1:1";
      case "portrait_4x_5":
        return "4:5";
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      autoTemplate.close();
    }
  }
</script>

{#if autoTemplate.open}
  <div class="at-backdrop" role="presentation" onclick={() => autoTemplate.close()}>
    <div
      class="at-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("autoTemplate.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="at-header">
        <span class="at-title">{t("autoTemplate.title")}</span>
        <button class="btn btn-ghost" onclick={() => autoTemplate.close()} title={t("autoTemplate.close")}>×</button>
      </div>

      <p class="at-explainer muted-2">{t("autoTemplate.explainer")}</p>

      <div class="at-body">
        <section class="at-section">
          <h3 class="at-section-title">{t("autoTemplate.sourceSectionTitle")}</h3>
          <div class="at-row">
            <label class="at-label" for="at-track">{t("silenceDetector.trackLabel")}</label>
            <select
              id="at-track"
              class="at-select"
              value={autoTemplate.trackId ?? ""}
              onchange={(e) => autoTemplate.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if autoTemplate.eligibleTracks.length === 0}
                <option value="" disabled>{t("silenceDetector.noTracks")}</option>
              {/if}
              {#each autoTemplate.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="at-row">
            <label class="at-label" for="at-clip">{t("silenceDetector.clipLabel")}</label>
            <select
              id="at-clip"
              class="at-select"
              value={autoTemplate.clipId ?? ""}
              disabled={autoTemplate.clipsForSelectedTrack.length === 0}
              onchange={(e) => autoTemplate.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if autoTemplate.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("silenceDetector.noClips")}</option>
              {/if}
              {#each autoTemplate.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          {#if autoTemplate.selectedMedia && autoTemplate.transcriptEntries.length === 0}
            <p class="at-hint muted-2">{t("autoTemplate.noTranscriptNote")}</p>
          {/if}
          {#if !autoTemplate.aiConfigured}
            <p class="at-hint muted-2">
              {t("autoTemplate.aiNotConfiguredHint")} ({aiSettingsStore.provider})
            </p>
          {/if}
        </section>

        <section class="at-section">
          <h3 class="at-section-title">{t("autoTemplate.resultSectionTitle")}</h3>
          {#if !autoTemplate.result}
            <p class="at-empty muted-2">{t("autoTemplate.resultEmpty")}</p>
          {:else}
            {@const rec = autoTemplate.result}
            <div class="at-card">
              <div class="at-card-header">
                <span class="at-card-name">{rec.template_name}</span>
                <span class="at-card-confidence">{t("autoTemplate.confidenceLabel")}: {(rec.confidence * 100).toFixed(0)}%</span>
              </div>
              <p class="at-card-reason">{rec.reason}</p>
              {#if rec.suggested_aspect}
                <span class="at-card-aspect">{t("autoTemplate.suggestedAspectLabel")}: {aspectLabel(rec.suggested_aspect)}</span>
              {/if}
              {#if !autoTemplate.recommendedTemplate}
                <p class="at-hint muted-2">{t("autoTemplate.recommendationUnresolved")}</p>
              {/if}
            </div>
          {/if}

          {#if autoTemplate.applyError}
            <div class="at-error">{autoTemplate.applyError}</div>
          {/if}
          {#if autoTemplate.appliedTemplateName}
            <div class="at-note">{t("autoTemplate.appliedNote", { name: autoTemplate.appliedTemplateName })}</div>
          {/if}
        </section>

        {#if autoTemplate.browsingCatalog}
          <section class="at-section">
            <h3 class="at-section-title">{t("autoTemplate.catalogSectionTitle")}</h3>
            <div class="at-catalog-list">
              {#each templatesStore.allTemplates as template (template.id)}
                <div class="at-catalog-row">
                  <span class="at-catalog-name">{template.name}</span>
                  <button class="btn btn-ghost btn-sm" onclick={() => void autoTemplate.applyTemplate(template)}>
                    {t("autoTemplate.pickButton")}
                  </button>
                </div>
              {/each}
            </div>
          </section>
        {/if}

        {#if autoTemplate.lastError}
          <div class="at-error">{autoTemplate.lastError}</div>
        {/if}
      </div>

      <div class="at-footer">
        <button class="btn" disabled={!autoTemplate.canSuggest} onclick={() => void autoTemplate.suggest()}>
          {autoTemplate.suggesting ? t("autoTemplate.suggesting") : t("autoTemplate.suggestButton")}
        </button>
        <button class="btn" disabled={!autoTemplate.recommendedTemplate} onclick={() => void autoTemplate.accept()}>
          {t("autoTemplate.acceptButton")}
        </button>
        <button class="btn btn-ghost" disabled={!autoTemplate.result} onclick={() => autoTemplate.toggleBrowseCatalog()}>
          {t("autoTemplate.changeTemplateButton")}
        </button>
        <button class="btn btn-ghost" disabled={!autoTemplate.canCustomizeOrRun} onclick={() => autoTemplate.openCustomize()}>
          {t("autoTemplate.customizeButton")}
        </button>
        <button class="btn btn-ghost" disabled={!autoTemplate.canCustomizeOrRun} onclick={() => autoTemplate.openRun()}>
          {t("autoTemplate.runButton")}
        </button>
        <span class="at-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => autoTemplate.close()}>{t("autoTemplate.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .at-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .at-dialog {
    width: min(720px, 94vw);
    max-height: 88vh;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
    overflow: hidden;
  }
  .at-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .at-title {
    font-size: 13px;
    font-weight: 600;
  }
  .at-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .at-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .at-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .at-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .at-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .at-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .at-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .at-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .at-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .at-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .at-card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .at-card-name {
    font-size: 13px;
    font-weight: 600;
  }
  .at-card-confidence {
    font-size: 10.5px;
    color: var(--muted);
  }
  .at-card-reason {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.4;
  }
  .at-card-aspect {
    font-size: 10.5px;
    color: var(--accent);
  }
  .at-catalog-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow-y: auto;
  }
  .at-catalog-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .at-catalog-name {
    font-size: 11.5px;
  }
  .btn-sm {
    height: 24px;
    padding: 0 8px;
    font-size: 10.5px;
  }
  .at-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .at-note {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--pos);
    background: hsl(140 60% 50% / 0.08);
    border: 1px solid hsl(140 60% 50% / 0.3);
    border-radius: var(--radius-sm);
  }
  .at-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .at-footer-spacer {
    flex: 1;
  }
</style>
