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

  **Phase D7c Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), the rule list is
  now `DataTable.svelte` (Name/Folder/Condition/Action/Actions columns, real
  client-side sort added on Name/Folder — a free, honest addition the
  retrofit enables, not a behavior change), the per-rule enable toggle is
  `Checkbox.svelte` (not `Switch.svelte` — see below, a deliberate choice to
  preserve a real existing interaction), the Create Rule form's bordered box
  is `Panel.svelte` + `Card.svelte`, its name/template/preset fields are
  `Input.svelte`/`Select.svelte`, its two toggles are `Checkbox.svelte`, and
  every button is `Button.svelte`. The numeric minutes field stays a plain
  `<input type="number">` — `Input.svelte`'s own doc comment explicitly
  scopes numeric fields out (`Slider.svelte` is this pass's numeric
  primitive, but it's a drag-a-handle control, not a typed-number field —
  swapping to it would be a real interaction change, not just chrome).

  **Enable toggle: `Checkbox`, not `Switch`.** The original row's toggle was
  a native `<input type="checkbox">` wrapped in a `<label>` together with
  the rule's name — clicking the *name text itself* toggles the rule, a
  real, working bit of native label/input semantics. `Switch.svelte` renders
  a `<button role="switch">`, not an `<input>`, so a native `<label>` around
  it would NOT delegate clicks the same way — adopting it here would quietly
  drop that click target. `Checkbox.svelte` renders the exact same
  `<label><input type="checkbox">…</label>` recipe the original hand-rolled,
  so it's the byte-for-byte-behavior-preserving choice, even though
  `AutomationSettingsTab.svelte`'s own unrelated, brand-new "quick toggle"
  row (Phase D5) uses `Switch` for the same store method — that file never
  had this click-the-label-text behavior to preserve in the first place.

  **Escape-key nuance preserved.** The original's own `onKeydown` did NOT
  always close the whole dialog: if the Create Rule form was open, Escape
  closed just the form (`closeCreateForm()`); only when the form was already
  closed did Escape close the dialog itself. `Modal.svelte`'s own built-in
  Escape handler always calls `onClose` unconditionally, which would lose
  this distinction. Fixed here with a `window`-level, capture-phase keydown
  listener (registered/torn down by a real `$effect` scoped to
  `automationStore.open`, the same "component owns lifecycle, store owns
  state" pattern `stores/autoZoom.svelte.ts`'s own doc comment already
  established for an unrelated feature): capture-phase runs before any
  bubble-phase listener a click/focus location would otherwise route
  through, so this reproduces the original's "catches Escape anywhere in
  the dialog, regardless of what currently has focus" behavior exactly — a
  plain `onkeydown` attached to one wrapper `<div>` inside `Modal`'s own
  body would NOT catch Escape if focus happened to be on the footer's Close
  button instead (a sibling subtree), which is why this uses `window` +
  capture instead. When the form is open, the listener calls
  `closeCreateForm()` and stops propagation, so `Modal`'s own handler never
  fires; when the form is closed, the listener does nothing and the key
  event proceeds to `Modal`'s own handler exactly as before. Backdrop-click
  and the header's `×` always closed the whole dialog unconditionally in the
  original too, so wiring `Modal`'s `onClose` straight to
  `automationStore.close()` reproduces that half unchanged.
-->
<script lang="ts">
  import { automationStore } from "../../stores/automation.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AutomationRule } from "../../types/bindings";
  import Modal from "../ui/Modal.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Checkbox from "../ui/Checkbox.svelte";
  import Input from "../ui/Input.svelte";
  import Select from "../ui/Select.svelte";
  import type { SelectOption } from "../ui/Select.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyState from "../ui/EmptyState.svelte";

  // See the file's own doc comment ("Escape-key nuance preserved") for why
  // this is a window-level capture-phase listener rather than a plain
  // `onkeydown` on some wrapper element.
  $effect(() => {
    if (!automationStore.open) return;
    function handleEscape(e: KeyboardEvent): void {
      if (e.key === "Escape" && automationStore.showCreateForm) {
        e.preventDefault();
        e.stopPropagation();
        automationStore.closeCreateForm();
      }
    }
    window.addEventListener("keydown", handleEscape, true);
    return () => window.removeEventListener("keydown", handleEscape, true);
  });

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

  function ruleKey(rule: AutomationRule): string {
    return rule.id;
  }

  let templateSelectOptions = $derived<SelectOption[]>([
    { value: "", label: t("automationRules.form.noTemplateOption") },
    ...automationStore.templates.map((tpl) => ({ value: tpl.id, label: tpl.name })),
  ]);

  let exportPresetOptions = $derived<SelectOption[]>([
    ...((automationStore.createMultiTemplateMode
      ? automationStore.createTemplateIds.length > 0
      : automationStore.createTemplateId !== null)
      ? [{ value: "", label: t("automationRules.form.useTemplateDefaultPreset") }]
      : []),
    ...automationStore.presets.map((p) => ({ value: p.id, label: p.name })),
  ]);
</script>

{#snippet nameCell(rule: AutomationRule)}
  <div class="ar-name-cell">
    <Checkbox
      checked={rule.enabled}
      disabled={automationStore.togglingById[rule.id] ?? false}
      onchange={(checked) => void automationStore.setEnabled(rule, checked)}
    >
      <span class="ar-name">{rule.name}</span>
    </Checkbox>
    {#if automationStore.toggleErrorById[rule.id]}
      <div class="ar-error">
        {t("automationRules.list.toggleFailed", { error: automationStore.toggleErrorById[rule.id] ?? "" })}
      </div>
    {/if}
  </div>
{/snippet}
{#snippet folderCell(rule: AutomationRule)}
  <span class="mono ar-detail-value" title={rule.trigger.path}>{rule.trigger.path}</span>
{/snippet}
{#snippet conditionCell(rule: AutomationRule)}
  {conditionSummary(rule)}
{/snippet}
{#snippet actionCell(rule: AutomationRule)}
  {actionSummary(rule)}
{/snippet}
{#snippet rowActionsCell(rule: AutomationRule)}
  <div class="ar-row-actions">
    {#if automationStore.pendingDeleteId === rule.id}
      <Button
        variant="danger"
        size="sm"
        disabled={automationStore.deletingId === rule.id}
        onclick={() => void automationStore.confirmDelete(rule.id)}
      >
        {automationStore.deletingId === rule.id ? t("automationRules.list.deleting") : t("automationRules.list.deleteConfirmButton")}
      </Button>
      <Button variant="ghost" size="sm" onclick={() => automationStore.cancelDelete()}>
        {t("automationRules.list.deleteCancelButton")}
      </Button>
    {:else}
      <Button variant="ghost" size="sm" onclick={() => automationStore.armDelete(rule.id)}>
        {t("automationRules.list.deleteButton")}
      </Button>
    {/if}
  </div>
{/snippet}

<Modal open={automationStore.open} title={t("automationRules.title")} onClose={() => automationStore.close()} width={760}>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => automationStore.close()}>{t("automationRules.close")}</Button>
  {/snippet}

  <div class="ar-body">
    <p class="ar-explainer muted-2">{t("automationRules.explainer")}</p>

    {#if automationStore.loadError}
      <div class="ar-error">{t("automationRules.loadFailed", { error: automationStore.loadError })}</div>
    {/if}
    {#if automationStore.deleteError}
      <div class="ar-error">{automationStore.deleteError}</div>
    {/if}

    <div class="ar-toolbar-row">
      <span class="ar-toolbar-spacer"></span>
      <Button size="sm" onclick={() => automationStore.openCreateForm()}>
        {t("automationRules.newRuleButton")}
      </Button>
    </div>

    {#if automationStore.showCreateForm}
      <Panel title={t("automationRules.form.title")}>
        <Card>
          <div class="ar-form-row">
            <label class="ar-label" for="ar-name">{t("automationRules.form.nameLabel")}</label>
            <div class="ar-field-grow">
              <Input id="ar-name" bind:value={automationStore.createName} placeholder={t("automationRules.form.namePlaceholder")} />
            </div>
          </div>

          <div class="ar-form-row">
            <Button variant="ghost" size="sm" onclick={() => void automationStore.pickFolder()}>
              {t("automationRules.form.chooseFolderButton")}
            </Button>
            {#if automationStore.createFolderPath}
              <span class="mono ar-detail-value" title={automationStore.createFolderPath}>
                {basename(automationStore.createFolderPath)}
              </span>
            {:else}
              <span class="muted-2">{t("automationRules.form.noFolderChosen")}</span>
            {/if}
          </div>

          <span class="ar-section-title">{t("automationRules.form.conditionSectionTitle")}</span>
          <Checkbox bind:checked={automationStore.createConditionEnabled}>
            {t("automationRules.form.conditionCheckboxLabel")}
          </Checkbox>
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
          <Checkbox bind:checked={automationStore.createMultiTemplateMode}>
            {t("automationRules.form.multiTemplateToggle")}
          </Checkbox>
          {#if automationStore.createMultiTemplateMode}
            <p class="ar-hint muted-2">{t("automationRules.form.multiTemplateHint")}</p>
            {#if automationStore.templates.length > 0}
              <ul class="ar-template-list">
                {#each automationStore.templates as tpl (tpl.id)}
                  <li class="ar-template-item">
                    <Checkbox
                      checked={automationStore.createTemplateIds.includes(tpl.id)}
                      onchange={() => automationStore.toggleCreateTemplateSelection(tpl.id)}
                    >
                      {tpl.name}
                    </Checkbox>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="ar-hint muted-2">{t("automationRules.form.noTemplatesYet")}</p>
            {/if}
          {:else}
            <div class="ar-form-row">
              <label class="ar-label" for="ar-template">{t("automationRules.form.templateLabel")}</label>
              <div class="ar-field-grow">
                <Select
                  id="ar-template"
                  value={automationStore.createTemplateId ?? ""}
                  options={templateSelectOptions}
                  onchange={(v) => (automationStore.createTemplateId = v || null)}
                />
              </div>
            </div>
          {/if}

          <div class="ar-form-row">
            <label class="ar-label" for="ar-preset">{t("automationRules.form.exportPresetLabel")}</label>
            <div class="ar-field-grow">
              <Select
                id="ar-preset"
                value={automationStore.createExportPresetId ?? ""}
                options={exportPresetOptions}
                onchange={(v) => (automationStore.createExportPresetId = v || null)}
              />
            </div>
          </div>

          <p class="ar-hint muted-2">{t("automationRules.form.scopeHint")}</p>

          {#if automationStore.createError}
            <div class="ar-error">{automationStore.createError}</div>
          {/if}

          <div class="ar-form-row">
            <Button size="sm" disabled={!automationStore.canSubmitCreate} onclick={() => void automationStore.submitCreate()}>
              {automationStore.creating ? t("automationRules.form.creating") : t("automationRules.form.createButton")}
            </Button>
            <Button variant="ghost" size="sm" onclick={() => automationStore.closeCreateForm()}>
              {t("automationRules.form.cancelButton")}
            </Button>
          </div>
        </Card>
      </Panel>
    {/if}

    {#if automationStore.loading && automationStore.rules.length === 0}
      <EmptyState title={t("automationRules.loading")} />
    {:else if automationStore.rules.length === 0}
      <EmptyState title={t("automationRules.noRules")} />
    {:else}
      <DataTable
        columns={[
          { key: "name", label: t("automationRules.list.colName"), sortable: true, accessor: (r) => r.name, cell: nameCell },
          {
            key: "folder",
            label: t("automationRules.list.colFolder"),
            sortable: true,
            accessor: (r) => r.trigger.path,
            cell: folderCell,
          },
          { key: "condition", label: t("automationRules.list.colCondition"), cell: conditionCell },
          { key: "action", label: t("automationRules.list.colAction"), cell: actionCell },
          { key: "actions", label: t("automationRules.list.colActions"), cell: rowActionsCell },
        ]}
        rows={automationStore.rules}
        rowKey={ruleKey}
      />
    {/if}
  </div>
</Modal>

<style>
  /* Design System retrofit (Phase D7c, `STUDIO_PLAN.md`): the dialog shell
     (`.ar-backdrop`/`.ar-dialog`/`.ar-header`/`.ar-title`/`.ar-footer`),
     the rule list (`.ar-list`/`.ar-row*`/`.ar-toggle`), the form's bordered
     box (`.ar-form`), the `<input>`/`<select>` recipe (`.ar-input`/
     `.ar-select`), the `.ar-checkbox` recipe, and every button's sizing/
     danger override (`.btn-sm`/`.btn-danger`) are ALL gone — `Modal`/
     `DataTable`/`Panel`/`Card`/`Checkbox`/`Select`/`Input`/`Button`/
     `EmptyState` (Design System) now own that chrome. Only the handful of
     layout rules with no Design System equivalent remain: the explainer/
     section-title/hint text sizing, the toolbar row + spacer, the form
     row's label+field layout (including the narrow/growing flex sizing
     matching the original `.ar-input`'s `flex:1`), the numeric minutes
     field (kept as a plain `<input type="number">`, see the file's own doc
     comment for why), the multi-template checklist layout, and the inline
     error banners. */
  .ar-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .ar-section-title {
    margin-top: var(--space-1);
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
  .ar-form-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .ar-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
    min-width: 100px;
  }
  .ar-field-grow {
    flex: 1;
    min-width: 120px;
  }
  .ar-number {
    width: 90px;
    height: 28px;
    padding: 0 var(--space-2);
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
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
    padding: 4px var(--space-2);
    background: var(--surface);
    border-radius: var(--radius-sm);
  }
  .ar-toolbar-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .ar-toolbar-spacer {
    flex: 1;
  }
  .ar-name-cell {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .ar-name {
    font-size: 12px;
    font-weight: 600;
  }
  .ar-detail-value {
    display: inline-block;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: bottom;
  }
  .ar-row-actions {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
  }
  .ar-error {
    padding: var(--space-2) var(--space-3);
    font-size: 10.5px;
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
  }
</style>
