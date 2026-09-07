<!--
  Automation Rules dialog (upgrade spec §27, `UPGRADE_PLAN.md` Phase U4 —
  frontend for the already-shipped backend rule engine: `src-tauri/src/
  automation/`, `commands/automation.rs`). Pure UI over
  `stores/automation.svelte.ts`: list every persisted rule (name, enabled
  toggle, watched folder, condition summary, action summary), a "New Rule…"
  form, and delete (two-step confirm — deleting a rule stops a live
  filesystem watcher, deliberately not a single-click action, unlike the
  enabled toggle which is immediate and reversible).

  Placement: a standalone dialog reachable from `TopBar.svelte`'s
  "Automation…" button — same "no master prompt §46 Settings surface exists
  yet" rationale every other standalone TopBar dialog in this codebase
  already documents (see `AssetLibraryDialog.svelte`'s own doc comment).
  Mounted once in `App.svelte`, alongside those other dialogs.

  Create Rule form scope (see `stores/automation.svelte.ts`'s own doc
  comment for the full reasoning): inlines `StartBatchDialog.svelte`'s own
  established single/multi-template toggle and export-preset picker
  verbatim, but deliberately does NOT expose silence-removal or caption
  settings in v1 — a small, honest `scopeHint` note says so in the form
  itself, and `UPGRADE_PLAN.md`'s Phase U4 frontend writeup documents the
  same call.

  No in-place "edit rule" flow is offered (per this pass's own task brief):
  `update_automation_rule`'s condition parameter can only be left-unchanged
  or replaced, never explicitly cleared back to "no condition" — rather than
  build a partial editor that can't really do that, this dialog only offers
  Create + toggle-enabled + delete. A rule that needs different settings is
  deleted and recreated.
-->
<script lang="ts">
  import { automationStore } from "../../stores/automation.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AutomationRule } from "../../types/bindings";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      if (automationStore.showCreateForm) {
        automationStore.closeCreateForm();
      } else {
        automationStore.close();
      }
    }
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  /** "At least N min long" / "No condition" — the exact `min_seconds` ->
   * minutes conversion, formatted with one decimal only when it isn't a
   * whole number of minutes (a rule created with a fractional-minute
   * condition is possible in principle, even though this dialog's own form
   * only ever writes whole minutes). */
  function conditionSummary(rule: AutomationRule): string {
    if (!rule.condition) return t("automationRules.list.conditionNone");
    const minutesRaw = rule.condition.min_seconds / 60;
    const minutes = Number.isInteger(minutesRaw) ? minutesRaw : Math.round(minutesRaw * 10) / 10;
    return t("automationRules.list.conditionMinDuration", { minutes });
  }

  function actionSummary(rule: AutomationRule): string {
    const ids = rule.action.template_ids;
    const templatePart =
      ids && ids.length > 0
        ? ids.map((id) => automationStore.templateName(id)).join(", ")
        : rule.action.config.template_id
          ? automationStore.templateName(rule.action.config.template_id)
          : t("automationRules.list.actionNoTemplate");
    const presetId = rule.action.config.export_preset_id;
    if (!presetId) return templatePart;
    const preset = automationStore.presets.find((p) => p.id === presetId);
    return `${templatePart} ${t("automationRules.list.actionPresetSuffix", { preset: preset?.name ?? presetId })}`;
  }
</script>

{#snippet ruleRow(rule: AutomationRule)}
  <div class="ar-row">
    <div class="ar-row-main">
      <div class="ar-row-header">
        <label class="ar-toggle">
          <input
            type="checkbox"
            checked={rule.enabled}
            disabled={automationStore.togglingById[rule.id] ?? false}
            onchange={(e) => void automationStore.setEnabled(rule, (e.target as HTMLInputElement).checked)}
          />
          <span class="ar-name">{rule.name}</span>
        </label>
      </div>
      <div class="ar-detail">
        <span class="ar-detail-label">{t("automationRules.list.watchedFolderLabel")}:</span>
        <span class="mono ar-detail-value" title={rule.trigger.path}>{rule.trigger.path}</span>
      </div>
      <div class="ar-detail">
        <span class="ar-detail-label">{t("automationRules.list.conditionLabel")}:</span>
        <span class="ar-detail-value">{conditionSummary(rule)}</span>
      </div>
      <div class="ar-detail">
        <span class="ar-detail-label">{t("automationRules.list.actionLabel")}:</span>
        <span class="ar-detail-value">{actionSummary(rule)}</span>
      </div>
      {#if automationStore.toggleErrorById[rule.id]}
        <div class="ar-error">{t("automationRules.list.toggleFailed", { error: automationStore.toggleErrorById[rule.id] ?? "" })}</div>
      {/if}
    </div>
    <div class="ar-row-actions">
      {#if automationStore.pendingDeleteId === rule.id}
        <button
          class="btn btn-danger btn-sm"
          disabled={automationStore.deletingId === rule.id}
          onclick={() => void automationStore.confirmDelete(rule.id)}
        >
          {automationStore.deletingId === rule.id ? t("automationRules.list.deleting") : t("automationRules.list.deleteConfirmButton")}
        </button>
        <button class="btn btn-ghost btn-sm" onclick={() => automationStore.cancelDelete()}>
          {t("automationRules.list.deleteCancelButton")}
        </button>
      {:else}
        <button class="btn btn-ghost btn-sm" onclick={() => automationStore.armDelete(rule.id)}>
          {t("automationRules.list.deleteButton")}
        </button>
      {/if}
    </div>
  </div>
{/snippet}

{#if automationStore.open}
  <div class="ar-backdrop" role="presentation" onclick={() => automationStore.close()}>
    <div
      class="ar-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("automationRules.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="ar-header">
        <span class="ar-title">{t("automationRules.title")}</span>
        <button class="btn btn-ghost" onclick={() => automationStore.close()} title={t("automationRules.close")}>×</button>
      </div>

      <div class="ar-body">
        <p class="ar-explainer muted-2">{t("automationRules.explainer")}</p>

        {#if automationStore.loadError}
          <div class="ar-error">{t("automationRules.loadFailed", { error: automationStore.loadError })}</div>
        {/if}
        {#if automationStore.deleteError}
          <div class="ar-error">{automationStore.deleteError}</div>
        {/if}

        <div class="ar-toolbar-row">
          <span class="ar-footer-spacer"></span>
          <button class="btn btn-sm" onclick={() => automationStore.openCreateForm()}>
            {t("automationRules.newRuleButton")}
          </button>
        </div>

        {#if automationStore.showCreateForm}
          <div class="ar-form">
            <span class="ar-section-title">{t("automationRules.form.title")}</span>

            <div class="ar-form-row">
              <label class="ar-label" for="ar-name">{t("automationRules.form.nameLabel")}</label>
              <input
                id="ar-name"
                class="ar-input"
                type="text"
                placeholder={t("automationRules.form.namePlaceholder")}
                bind:value={automationStore.createName}
              />
            </div>

            <div class="ar-form-row">
              <button class="btn btn-ghost btn-sm" onclick={() => void automationStore.pickFolder()}>
                {t("automationRules.form.chooseFolderButton")}
              </button>
              {#if automationStore.createFolderPath}
                <span class="mono ar-detail-value" title={automationStore.createFolderPath}>
                  {basename(automationStore.createFolderPath)}
                </span>
              {:else}
                <span class="muted-2">{t("automationRules.form.noFolderChosen")}</span>
              {/if}
            </div>

            <span class="ar-section-title">{t("automationRules.form.conditionSectionTitle")}</span>
            <label class="ar-checkbox">
              <input type="checkbox" bind:checked={automationStore.createConditionEnabled} />
              {t("automationRules.form.conditionCheckboxLabel")}
            </label>
            {#if automationStore.createConditionEnabled}
              <div class="ar-form-row">
                <input
                  class="ar-number"
                  type="number"
                  min="0"
                  step="1"
                  bind:value={automationStore.createMinDurationMinutes}
                />
                <span class="muted-2">{t("automationRules.form.minDurationSuffix")}</span>
              </div>
            {/if}

            <span class="ar-section-title">{t("automationRules.form.actionSectionTitle")}</span>
            <label class="ar-checkbox">
              <input type="checkbox" bind:checked={automationStore.createMultiTemplateMode} />
              {t("automationRules.form.multiTemplateToggle")}
            </label>
            {#if automationStore.createMultiTemplateMode}
              <p class="ar-hint muted-2">{t("automationRules.form.multiTemplateHint")}</p>
              {#if automationStore.templates.length > 0}
                <ul class="ar-template-list">
                  {#each automationStore.templates as tpl (tpl.id)}
                    <li class="ar-template-item">
                      <label class="ar-checkbox">
                        <input
                          type="checkbox"
                          checked={automationStore.createTemplateIds.includes(tpl.id)}
                          onchange={() => automationStore.toggleCreateTemplateSelection(tpl.id)}
                        />
                        {tpl.name}
                      </label>
                    </li>
                  {/each}
                </ul>
              {:else}
                <p class="ar-hint muted-2">{t("automationRules.form.noTemplatesYet")}</p>
              {/if}
            {:else}
              <div class="ar-form-row">
                <label class="ar-label" for="ar-template">{t("automationRules.form.templateLabel")}</label>
                <select
                  id="ar-template"
                  class="ar-select"
                  value={automationStore.createTemplateId ?? ""}
                  onchange={(e) => (automationStore.createTemplateId = (e.target as HTMLSelectElement).value || null)}
                >
                  <option value="">{t("automationRules.form.noTemplateOption")}</option>
                  {#each automationStore.templates as tpl (tpl.id)}
                    <option value={tpl.id}>{tpl.name}</option>
                  {/each}
                </select>
              </div>
            {/if}

            <div class="ar-form-row">
              <label class="ar-label" for="ar-preset">{t("automationRules.form.exportPresetLabel")}</label>
              <select
                id="ar-preset"
                class="ar-select"
                value={automationStore.createExportPresetId ?? ""}
                onchange={(e) => (automationStore.createExportPresetId = (e.target as HTMLSelectElement).value || null)}
              >
                {#if automationStore.createMultiTemplateMode ? automationStore.createTemplateIds.length > 0 : automationStore.createTemplateId !== null}
                  <option value="">{t("automationRules.form.useTemplateDefaultPreset")}</option>
                {/if}
                {#each automationStore.presets as p (p.id)}
                  <option value={p.id}>{p.name}</option>
                {/each}
              </select>
            </div>

            <p class="ar-hint muted-2">{t("automationRules.form.scopeHint")}</p>

            {#if automationStore.createError}
              <div class="ar-error">{automationStore.createError}</div>
            {/if}

            <div class="ar-form-row">
              <button
                class="btn btn-sm"
                disabled={!automationStore.canSubmitCreate}
                onclick={() => void automationStore.submitCreate()}
              >
                {automationStore.creating ? t("automationRules.form.creating") : t("automationRules.form.createButton")}
              </button>
              <button class="btn btn-ghost btn-sm" onclick={() => automationStore.closeCreateForm()}>
                {t("automationRules.form.cancelButton")}
              </button>
            </div>
          </div>
        {/if}

        {#if automationStore.loading && automationStore.rules.length === 0}
          <p class="ar-empty muted-2">{t("automationRules.loading")}</p>
        {:else if automationStore.rules.length === 0}
          <p class="ar-empty muted-2">{t("automationRules.noRules")}</p>
        {:else}
          <div class="ar-list">
            {#each automationStore.rules as rule (rule.id)}
              {@render ruleRow(rule)}
            {/each}
          </div>
        {/if}
      </div>

      <div class="ar-footer">
        <span class="ar-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => automationStore.close()}>{t("automationRules.close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ar-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .ar-dialog {
    width: min(680px, 94vw);
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
  .ar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ar-title {
    font-size: 13px;
    font-weight: 600;
  }
  .ar-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ar-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .ar-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .ar-section-title {
    margin-top: 6px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .ar-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .ar-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .ar-form-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .ar-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
    min-width: 100px;
  }
  .ar-input,
  .ar-select {
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
    flex: 1;
    min-width: 120px;
  }
  .ar-number {
    width: 90px;
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .ar-checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    cursor: pointer;
    width: fit-content;
  }
  .ar-toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .ar-template-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 140px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ar-template-item {
    padding: 4px 8px;
    background: var(--surface);
    border-radius: var(--radius-sm);
  }
  .ar-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ar-toolbar-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ar-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .ar-row-main {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }
  .ar-row-header {
    display: flex;
    align-items: center;
  }
  .ar-name {
    font-size: 12px;
    font-weight: 600;
  }
  .ar-detail {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 10.5px;
    min-width: 0;
  }
  .ar-detail-label {
    color: var(--muted);
    flex-shrink: 0;
  }
  .ar-detail-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .ar-row-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }
  .btn-sm {
    height: 24px;
    padding: 0 8px;
    font-size: 10.5px;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
  .ar-error {
    padding: 6px 10px;
    font-size: 10.5px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .ar-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ar-footer-spacer {
    flex: 1;
  }
</style>
