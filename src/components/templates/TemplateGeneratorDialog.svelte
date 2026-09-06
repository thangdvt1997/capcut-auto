<!--
  AI Template Generator dialog (upgrade spec §8, `UPGRADE_PLAN.md` Phase U2):
  "Natural language -> Template Definition -> Validate -> Template Builder ->
  Preview -> Save Template". Opened from `TopBar.svelte`'s own "Template
  Generator…" button — a standalone entry point (same "no dedicated Settings
  surface yet" rationale `TopBar.svelte` already documents for several of its
  other buttons), chosen over adding a button to `TemplatesPanel.svelte`
  itself since that panel may be under concurrent edit by the Asset
  Library/Template-versioning UI pass (task brief).

  Pure UI over `stores/templateGenerator.svelte.ts`. "Generate" already
  covers upgrade spec §8's Definition -> Validate -> Builder pipeline
  (chained server-side by `generate_template_from_prompt`) — this dialog's
  own job is a real, structured Preview (not a JSON dump) of the resulting
  `Template`, and a separate, explicit "Save Template" click
  (`save_generated_template`) before it becomes reusable in the normal
  template gallery.
-->
<script lang="ts">
  import { templateGenerator } from "../../stores/templateGenerator.svelte";
  import { t } from "../../lib/i18n.svelte";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      templateGenerator.close();
    }
  }

  function msLabel(us: number): string {
    return `${Math.round(us / 1000)}ms`;
  }
</script>

{#if templateGenerator.open}
  <div class="tg-backdrop" role="presentation" onclick={() => templateGenerator.close()}>
    <div
      class="tg-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("templateGenerator.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="tg-header">
        <span class="tg-title">{t("templateGenerator.title")}</span>
        <button class="btn btn-ghost" onclick={() => templateGenerator.close()} title={t("templateGenerator.close")}>×</button>
      </div>

      <p class="tg-explainer muted-2">{t("templateGenerator.explainer")}</p>

      <div class="tg-body">
        <section class="tg-section">
          <h3 class="tg-section-title">{t("templateGenerator.promptSectionTitle")}</h3>
          <textarea
            class="tg-textarea"
            rows="3"
            placeholder={t("templateGenerator.promptPlaceholder")}
            bind:value={templateGenerator.nlPrompt}
          ></textarea>
        </section>

        {#if templateGenerator.lastError}
          <div class="tg-error">{templateGenerator.lastError}</div>
        {/if}

        <section class="tg-section">
          <h3 class="tg-section-title">{t("templateGenerator.previewSectionTitle")}</h3>
          {#if !templateGenerator.generatedTemplate}
            <p class="tg-empty muted-2">{t("templateGenerator.previewEmpty")}</p>
          {:else}
            {@const tmpl = templateGenerator.generatedTemplate}
            <div class="tg-preview">
              <div class="tg-preview-header">
                <span class="tg-preview-name">{tmpl.name}</span>
                <span class="tg-preview-badge">{t("templatesPanel.customBadge")}</span>
              </div>
              <p class="tg-preview-desc muted-2">{tmpl.description}</p>

              <div class="tg-fact-grid">
                <span class="tg-fact-label">{t("templateGenerator.factAspect")}</span>
                <span class="tg-fact-value mono">{tmpl.canvas.ratio_preset} ({tmpl.canvas.width}×{tmpl.canvas.height})</span>

                <span class="tg-fact-label">{t("templateGenerator.factCaptionStyle")}</span>
                <span class="tg-fact-value">{tmpl.caption_style.name}</span>

                <span class="tg-fact-label">{t("templateGenerator.factZoom")}</span>
                <span class="tg-fact-value">{t(`autoZoom.intensity.${tmpl.zoom_intensity}`)}</span>

                <span class="tg-fact-label">{t("templateGenerator.factSilence")}</span>
                <span class="tg-fact-value">
                  {t("templateGenerator.factSilenceValue", {
                    before: msLabel(tmpl.silence_settings.padding_before_us),
                    after: msLabel(tmpl.silence_settings.padding_after_us),
                    merge: msLabel(tmpl.silence_settings.merge_gap_us),
                  })}
                </span>

                <span class="tg-fact-label">{t("templateGenerator.factTransition")}</span>
                <span class="tg-fact-value">
                  {t(`templatesPanel.transitionType.${tmpl.transition_settings.transition_type}`)}
                  {#if tmpl.transition_settings.transition_type === "cross_fade"}
                    · {msLabel(tmpl.transition_settings.duration_us)}
                  {/if}
                </span>

                <span class="tg-fact-label">{t("templateGenerator.factExportPreset")}</span>
                <span class="tg-fact-value">{templateGenerator.presetLabel(tmpl.export_preset_id)}</span>

                {#if tmpl.ai_prompt_config.emphasized_categories.length > 0}
                  <span class="tg-fact-label">{t("templateGenerator.factEmphasis")}</span>
                  <span class="tg-fact-value">
                    {tmpl.ai_prompt_config.emphasized_categories.map((c) => t(`smartEdit.category.${c}`)).join(", ")}
                  </span>
                {/if}

                {#if tmpl.ai_prompt_config.system_prompt_prefix}
                  <span class="tg-fact-label">{t("templateGenerator.factPromptPrefix")}</span>
                  <span class="tg-fact-value">{tmpl.ai_prompt_config.system_prompt_prefix}</span>
                {/if}

                {#if tmpl.intro}
                  <span class="tg-fact-label">{t("templateGenerator.factIntro")}</span>
                  <span class="tg-fact-value">{templateGenerator.assetLabel(tmpl.intro.asset_id)}</span>
                {/if}

                {#if tmpl.outro}
                  <span class="tg-fact-label">{t("templateGenerator.factOutro")}</span>
                  <span class="tg-fact-value">{templateGenerator.assetLabel(tmpl.outro.asset_id)}</span>
                {/if}

                {#if tmpl.watermark}
                  <span class="tg-fact-label">{t("templateGenerator.factWatermark")}</span>
                  <span class="tg-fact-value">
                    {templateGenerator.assetLabel(tmpl.watermark.asset_id)} · {t(`templateGenerator.watermarkPosition.${tmpl.watermark.position}`)}
                  </span>
                {/if}

                {#if tmpl.background_music}
                  <span class="tg-fact-label">{t("templateGenerator.factBackgroundMusic")}</span>
                  <span class="tg-fact-value">
                    {templateGenerator.assetLabel(tmpl.background_music.asset_id)} · {t("templateGenerator.factVolume", { volume: tmpl.background_music.volume.toFixed(2) })}
                  </span>
                {/if}

                {#if tmpl.sports_overlay}
                  <span class="tg-fact-label">{t("templateGenerator.factSportsOverlay")}</span>
                  <span class="tg-fact-value">
                    {t(`templateGenerator.audioRole.${tmpl.sports_overlay.music_role}`)} ·
                    {t("templateGenerator.factDuckLevel", { level: (tmpl.sports_overlay.music_ducking.duck_level * 100).toFixed(0) })}
                  </span>
                {/if}
              </div>
            </div>
          {/if}

          {#if templateGenerator.saveError}
            <div class="tg-error">{templateGenerator.saveError}</div>
          {/if}
          {#if templateGenerator.savedTemplateName}
            <div class="tg-note">{t("templateGenerator.savedNote", { name: templateGenerator.savedTemplateName })}</div>
          {/if}
        </section>
      </div>

      <div class="tg-footer">
        <button class="btn" disabled={!templateGenerator.canGenerate} onclick={() => void templateGenerator.generate()}>
          {templateGenerator.generating ? t("templateGenerator.generating") : t("templateGenerator.generateButton")}
        </button>
        <button class="btn" disabled={!templateGenerator.canSave} onclick={() => void templateGenerator.saveTemplate()}>
          {templateGenerator.saving ? t("templateGenerator.saving") : t("templateGenerator.saveButton")}
        </button>
        <button class="btn btn-ghost" onclick={() => templateGenerator.reset()}>{t("templateGenerator.resetButton")}</button>
        <span class="tg-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => templateGenerator.close()}>{t("templateGenerator.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .tg-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .tg-dialog {
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
  .tg-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .tg-title {
    font-size: 13px;
    font-weight: 600;
  }
  .tg-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .tg-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .tg-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .tg-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .tg-textarea {
    width: 100%;
    min-height: 64px;
    padding: 8px 10px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    resize: vertical;
  }
  .tg-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .tg-preview {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .tg-preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .tg-preview-name {
    font-size: 13px;
    font-weight: 600;
  }
  .tg-preview-badge {
    font-size: 9.5px;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--accent);
    color: var(--accent);
    white-space: nowrap;
  }
  .tg-preview-desc {
    margin: 0;
    font-size: 11px;
    line-height: 1.4;
  }
  .tg-fact-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 10px;
  }
  .tg-fact-label {
    font-size: 10.5px;
    color: var(--muted);
    white-space: nowrap;
  }
  .tg-fact-value {
    font-size: 11.5px;
  }
  .tg-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .tg-note {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--pos);
    background: hsl(140 60% 50% / 0.08);
    border: 1px solid hsl(140 60% 50% / 0.3);
    border-radius: var(--radius-sm);
  }
  .tg-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .tg-footer-spacer {
    flex: 1;
  }
</style>
