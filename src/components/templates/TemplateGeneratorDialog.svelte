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

  **Phase D7c Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), the two section
  headings are `Panel.svelte`, the preview box is `Card.svelte`, the
  "Custom" pill is `Badge.svelte`, and every button is `Button.svelte`. The
  fact grid itself and the free-form prompt `<textarea>` stay hand-rolled
  (no Design System table/textarea primitive fits either honestly — see the
  `<style>` block for exactly what's left and why). Every real behavior is
  unchanged: the exact same `templateGenerator` state/methods drive every
  conditional, disabled state, and click handler as before.
-->
<script lang="ts">
  import { templateGenerator } from "../../stores/templateGenerator.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyState from "../ui/EmptyState.svelte";

  function msLabel(us: number): string {
    return `${Math.round(us / 1000)}ms`;
  }
</script>

<Modal open={templateGenerator.open} title={t("templateGenerator.title")} onClose={() => templateGenerator.close()} width={720}>
  {#snippet footer()}
    <Button disabled={!templateGenerator.canGenerate} onclick={() => void templateGenerator.generate()}>
      {templateGenerator.generating ? t("templateGenerator.generating") : t("templateGenerator.generateButton")}
    </Button>
    <Button disabled={!templateGenerator.canSave} onclick={() => void templateGenerator.saveTemplate()}>
      {templateGenerator.saving ? t("templateGenerator.saving") : t("templateGenerator.saveButton")}
    </Button>
    <Button variant="ghost" onclick={() => templateGenerator.reset()}>{t("templateGenerator.resetButton")}</Button>
    <Button variant="ghost" onclick={() => templateGenerator.close()}>{t("templateGenerator.closeButton")}</Button>
  {/snippet}

  <p class="tg-explainer muted-2">{t("templateGenerator.explainer")}</p>

  <Panel title={t("templateGenerator.promptSectionTitle")}>
    <textarea
      class="tg-textarea"
      rows="3"
      placeholder={t("templateGenerator.promptPlaceholder")}
      bind:value={templateGenerator.nlPrompt}
    ></textarea>
  </Panel>

  {#if templateGenerator.lastError}
    <div class="tg-error">{templateGenerator.lastError}</div>
  {/if}

  <Panel title={t("templateGenerator.previewSectionTitle")}>
    {#if !templateGenerator.generatedTemplate}
      <EmptyState title={t("templateGenerator.previewEmpty")} />
    {:else}
      {@const tmpl = templateGenerator.generatedTemplate}
      <Card>
        <div class="tg-preview-header">
          <span class="tg-preview-name">{tmpl.name}</span>
          <Badge variant="accent">{t("templatesPanel.customBadge")}</Badge>
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
      </Card>
    {/if}

    {#if templateGenerator.saveError}
      <div class="tg-error">{templateGenerator.saveError}</div>
    {/if}
    {#if templateGenerator.savedTemplateName}
      <div class="tg-note">{t("templateGenerator.savedNote", { name: templateGenerator.savedTemplateName })}</div>
    {/if}
  </Panel>
</Modal>

<style>
  /* Design System retrofit (Phase D7c, `STUDIO_PLAN.md`): the dialog shell
     (`.tg-backdrop`/`.tg-dialog`/`.tg-header`/`.tg-title`/`.tg-footer`/
     `.tg-footer-spacer`), the section headings (`.tg-section`/
     `.tg-section-title`), the preview box's border/background
     (`.tg-preview`), and the "Custom" pill (`.tg-preview-badge`) are ALL
     gone — `Modal`/`Panel`/`Card`/`Badge`/`Button` (Design System) now own
     that chrome. Only the handful of layout rules with no Design System
     equivalent remain: the explainer text sizing, the free-form prompt
     `<textarea>` (no Design System textarea primitive exists — `Input`
     is single-line only, per its own doc comment), the preview header/
     name/description layout, the fact grid itself (a two-column label/value
     grid built from a variable set of optional facts — not a good fit for
     `DataTable`'s row-shaped model), and the error/success banners. */
  .tg-explainer {
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
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
  .tg-preview-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .tg-preview-name {
    font-size: 13px;
    font-weight: 600;
  }
  .tg-preview-desc {
    margin: var(--space-2) 0 0;
    font-size: 11px;
    line-height: 1.4;
  }
  .tg-fact-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 10px;
    margin-top: var(--space-2);
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
    padding: var(--space-2) var(--space-3);
    font-size: 11px;
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
  }
  .tg-note {
    padding: var(--space-2) var(--space-3);
    font-size: 11px;
    color: var(--pos);
    background: var(--pos-bg);
    border: 1px solid var(--pos-border);
    border-radius: var(--radius-sm);
  }
</style>
