<!--
  Preset Manager dialog (`STUDIO_PLAN.md` Phase D17, `promt.md` §18 — see
  `stores/presets.svelte.ts`'s own module doc comment for the full "what a
  Preset bundles, and why not every §18 category" reasoning). Pure UI over
  `presetsStore`: list every saved preset (name + a compact non-secret
  summary of what it bundles), "Save current settings as preset…" (name
  input + Save), and per-row Apply/Export/Delete — plus a toolbar-level
  Import.

  Structurally mirrors `AutomationRulesDialog.svelte` (list + inline create
  form + two-step delete confirm, `Modal`/`DataTable`/`Panel`/`Card`/`Input`/
  `Button`/`EmptyState`), the closest existing precedent for "a handful of
  named, user-created, deletable items."

  Mounted in `App.svelte` alongside the other standalone TopBar/Tab-2
  dialogs; opened from `AutomationSettingsTab.svelte`'s new Presets card
  (Tab 2 — presets bundle AI/Translation/Voice settings, the same settings
  areas that tab already surfaces as status cards).
-->
<script lang="ts">
  import { presetsStore } from "../../stores/presets.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Preset } from "../../stores/presets.svelte";
  import Modal from "../ui/Modal.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Input from "../ui/Input.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyState from "../ui/EmptyState.svelte";

  function providerLabel(kind: string): string {
    switch (kind) {
      case "open_ai":
        return t("aiSettings.providerOpenAi");
      case "ollama":
        return t("aiSettings.providerOllama");
      case "custom_open_ai_compatible":
        return t("aiSettings.providerCustom");
      case "anthropic":
        return t("aiSettings.providerAnthropic");
      case "gemini":
        return t("aiSettings.providerGemini");
      case "custom_api":
        return t("voiceSettings.providerCustomApi");
      case "nts_gen_ai":
        return t("voiceSettings.providerNtsGenAi");
      case "gpt_so_vits":
        return t("voiceSettings.providerGptSoVits");
      default:
        return kind;
    }
  }

  /** Compact, honest, non-secret summary of everything a preset bundles —
   * shown as the DataTable's one "Summary" column rather than five separate
   * narrow columns, which would crowd out the Name/Actions columns for what
   * is, in practice, always a handful of presets (`promt.md`'s own worked
   * example names four). */
  function summary(preset: Preset): string {
    const parts = [
      t("presetManager.summary.ai", {
        provider: providerLabel(preset.aiConfig.provider),
        model: preset.aiConfig.model || "—",
      }),
      t("presetManager.summary.translation", {
        lang: preset.translationConfig.targetLanguage || "—",
        genre: preset.translationConfig.genre || t("presetManager.summary.noGenre"),
      }),
      t("presetManager.summary.voice", {
        provider: providerLabel(preset.voiceConfig.provider),
        count: preset.voiceConfig.roleMappings.length,
      }),
      t("presetManager.summary.export", {
        preset: presetsStore.renderPresetName(preset.exportPresetId) ?? t("presetManager.summary.noExportPreset"),
      }),
      t("presetManager.summary.capcut", {
        path: preset.capcutConfig.manualDraftRoot ?? t("presetManager.summary.autoDetected"),
      }),
    ];
    return parts.join(" · ");
  }

  function presetKey(preset: Preset): string {
    return preset.id;
  }
</script>

{#snippet nameCell(preset: Preset)}
  <span class="pm-name" title={preset.name}>{preset.name}</span>
{/snippet}
{#snippet summaryCell(preset: Preset)}
  <span class="pm-summary" title={summary(preset)}>{summary(preset)}</span>
{/snippet}
{#snippet actionsCell(preset: Preset)}
  <div class="pm-row-actions">
    {#if presetsStore.pendingDeleteId === preset.id}
      <Button variant="danger" size="sm" onclick={() => presetsStore.remove(preset.id)}>
        {t("presetManager.list.deleteConfirmButton")}
      </Button>
      <Button variant="ghost" size="sm" onclick={() => presetsStore.cancelDelete()}>
        {t("presetManager.list.deleteCancelButton")}
      </Button>
    {:else}
      <Button
        size="sm"
        disabled={presetsStore.applyingId === preset.id}
        onclick={() => void presetsStore.apply(preset.id)}
      >
        {presetsStore.applyingId === preset.id ? t("presetManager.list.applying") : t("presetManager.list.applyButton")}
      </Button>
      <Button
        variant="ghost"
        size="sm"
        disabled={presetsStore.exportingId === preset.id}
        onclick={() => void presetsStore.exportToFile(preset.id)}
      >
        {presetsStore.exportingId === preset.id
          ? t("presetManager.list.exporting")
          : t("presetManager.list.exportButton")}
      </Button>
      <Button variant="ghost" size="sm" onclick={() => presetsStore.armDelete(preset.id)}>
        {t("presetManager.list.deleteButton")}
      </Button>
    {/if}
  </div>
{/snippet}

<Modal open={presetsStore.open} title={t("presetManager.title")} onClose={() => presetsStore.close()} width={820}>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => void presetsStore.importFromFile()} disabled={presetsStore.importing}>
      {presetsStore.importing ? t("presetManager.importing") : t("presetManager.importButton")}
    </Button>
    <Button variant="ghost" onclick={() => presetsStore.close()}>{t("presetManager.close")}</Button>
  {/snippet}

  <div class="pm-body">
    <p class="pm-explainer muted-2">{t("presetManager.explainer")}</p>

    {#if presetsStore.importError}
      <div class="pm-error">{t("presetManager.importFailed", { error: presetsStore.importError })}</div>
    {/if}
    {#if presetsStore.exportError}
      <div class="pm-error">{t("presetManager.exportFailed", { error: presetsStore.exportError })}</div>
    {/if}

    <Panel title={t("presetManager.saveForm.title")}>
      <Card>
        <div class="pm-form-row">
          <label class="pm-label" for="pm-save-name">{t("presetManager.saveForm.nameLabel")}</label>
          <div class="pm-field-grow">
            <Input
              id="pm-save-name"
              bind:value={presetsStore.saveNameDraft}
              placeholder={t("presetManager.saveForm.namePlaceholder")}
            />
          </div>
          <Button size="sm" disabled={!presetsStore.canSave} onclick={() => presetsStore.submitSave()}>
            {t("presetManager.saveForm.saveButton")}
          </Button>
        </div>
        <p class="pm-hint muted-2">{t("presetManager.saveForm.hint")}</p>
      </Card>
    </Panel>

    {#if presetsStore.presets.length === 0}
      <EmptyState title={t("presetManager.list.empty")} />
    {:else}
      <DataTable
        columns={[
          { key: "name", label: t("presetManager.list.colName"), sortable: true, accessor: (p) => p.name, cell: nameCell },
          { key: "summary", label: t("presetManager.list.colSummary"), cell: summaryCell },
          { key: "actions", label: t("presetManager.list.colActions"), cell: actionsCell },
        ]}
        rows={presetsStore.presets}
        rowKey={presetKey}
      />
    {/if}
  </div>
</Modal>

<style>
  .pm-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .pm-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .pm-form-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .pm-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .pm-field-grow {
    flex: 1;
    min-width: 160px;
  }
  .pm-hint {
    margin: var(--space-1) 0 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .pm-name {
    font-size: 12px;
    font-weight: 600;
    display: inline-block;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }
  .pm-summary {
    font-size: 10.5px;
    color: var(--muted);
    display: inline-block;
    max-width: 420px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }
  .pm-row-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
  }
  .pm-error {
    padding: var(--space-2) var(--space-3);
    font-size: 10.5px;
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
  }
</style>
