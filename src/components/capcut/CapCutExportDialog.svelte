<!--
  Export to CapCut dialog (Phase 9, master prompt §31). Mode defaults to
  "Create New Draft" (a fresh subfolder, named by the user, under the
  detected/overridden draft root from `CapCutSettingsDialog.svelte`) with
  "Update Existing Draft" (browse to a real existing folder) as the
  alternative. Either path always goes through an explicit "Confirm &
  Export" step before `export_project_to_capcut_draft` is actually called —
  see `stores/capcut.svelte.ts`'s module doc comment for why this applies to
  both modes (this pass has no frontend filesystem-read capability to check
  whether a fresh "Create" name happens to collide with something real, so
  every export is treated as a potential overwrite, matching master prompt
  §30's "never overwrite user drafts without confirmation").

  Before that confirm step, a best-effort, non-blocking compatibility check
  (`capcut/compat.ts`) flags project content the Rust adapter is documented
  to not fully resolve yet (effects/animations pass through unresolved,
  keyframes outside the six supported properties are skipped) — shown as
  warnings, never as a block on exporting.

  Placement: mirrors `ExportDialog.svelte`'s own precedent — mounted once in
  `App.svelte`, reachable from `TopBar.svelte`'s File menu
  ("File > Export to CapCut…"), backed by one shared `capcutStore` instance.

  Pure UI over `stores/capcut.svelte.ts`.

  Phase D7b retrofit: the hand-rolled backdrop/dialog/header/footer shell
  now comes from `Modal`, sections from `Panel`, the target-path/confirm
  boxes from `Card`, error banners from `ErrorState`, and every button from
  `Button` (Phase D1 Design System) — every store call, `disabled` gate, and
  conditional-render expression is unchanged from the original hand-rolled
  markup. No RadioGroup component exists in the Design System yet, so the
  Create/Update mode picker stays a hand-rolled `<input type="radio">` pair
  (functionally identical to before). See this file's own `<style>` block
  and `STUDIO_PLAN.md`'s Phase D7b section for what else stayed bespoke and
  why.
-->
<script lang="ts">
  import { capcutStore } from "../../stores/capcut.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Card from "../ui/Card.svelte";
  import Button from "../ui/Button.svelte";
  import Input from "../ui/Input.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import ErrorState from "../ui/ErrorState.svelte";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }
</script>

<Modal
  open={capcutStore.exportOpen}
  title={t("capcutExport.title")}
  width={640}
  onClose={() => capcutStore.closeExport()}
>
  {#if !timeline.project}
    <EmptyState title={t("capcutExport.noProject")} />
  {:else}
    <Panel title={t("capcutExport.modeSectionTitle")}>
      <div class="ce-radio-group">
        <label class="ce-radio">
          <input
            type="radio"
            name="ce-mode"
            value="create"
            checked={capcutStore.mode === "create"}
            onchange={() => capcutStore.setMode("create")}
          />
          {t("capcutExport.modeCreate")}
        </label>
        <label class="ce-radio">
          <input
            type="radio"
            name="ce-mode"
            value="update"
            checked={capcutStore.mode === "update"}
            onchange={() => capcutStore.setMode("update")}
          />
          {t("capcutExport.modeUpdate")}
        </label>
      </div>

      {#if !capcutStore.effectiveDraftRoot}
        <div class="ce-warn">
          {t("capcutExport.noDraftRootKnown")}
          <Button variant="ghost" size="sm" onclick={() => capcutStore.openSettings()}>
            {t("capcutExport.openSettingsButton")}
          </Button>
        </div>
      {/if}

      {#if capcutStore.mode === "create"}
        <Input
          id="ce-draft-name"
          label={t("capcutExport.draftNameLabel")}
          placeholder={t("capcutExport.draftNamePlaceholder")}
          bind:value={capcutStore.draftName}
        />
      {:else}
        <div class="ce-row">
          <span class="ce-label">{t("capcutExport.existingDraftLabel")}</span>
          <Button size="sm" onclick={() => void capcutStore.browseExistingDraft()}>
            {t("capcutExport.browseButton")}
          </Button>
          <span class="ce-path muted-2" title={capcutStore.existingDraftPath ?? undefined}>
            {capcutStore.existingDraftPath ? basename(capcutStore.existingDraftPath) : t("capcutExport.noExistingDraftChosen")}
          </span>
        </div>
      {/if}

      {#if capcutStore.targetPath}
        <p class="ce-target-label muted-2">{t("capcutExport.targetPathLabel")}:</p>
        <Card padding="sm"><span class="mono ce-path-text">{capcutStore.targetPath}</span></Card>
      {/if}
    </Panel>

    <Panel title={t("capcutExport.warningsSectionTitle")}>
      {#if capcutStore.compatWarnings.length === 0}
        <p class="ce-ok">{t("capcutExport.noWarnings")}</p>
      {:else}
        <ul class="ce-warn-list">
          {#each capcutStore.compatWarnings as warning (warning.key)}
            <li>{t(warning.key, warning.params)}</li>
          {/each}
        </ul>
      {/if}
      <p class="ce-note muted-2">{t("capcutExport.limitationsNote")}</p>
    </Panel>

    {#if capcutStore.confirmingExport}
      <div class="ce-confirm">
        <Panel title={t("capcutExport.confirmTitle")}>
          <p class="ce-confirm-body">{t("capcutExport.confirmBody")}</p>
          <Card padding="sm"><span class="mono ce-path-text">{capcutStore.targetPath}</span></Card>
          <p class="ce-warn-strong">{t("capcutExport.confirmOverwriteWarning")}</p>
        </Panel>
      </div>
    {/if}

    {#if capcutStore.exportError}
      <ErrorState message={t("capcutExport.exportFailed", { error: capcutStore.exportError })} />
    {/if}

    {#if capcutStore.exportedPath}
      <p class="ce-success">{t("capcutExport.exportComplete", { path: capcutStore.exportedPath })}</p>

      <Panel title={t("capcutExport.postExportSectionTitle")}>
        <div class="ce-row">
          <Button
            variant="ghost"
            size="sm"
            disabled={capcutStore.validating}
            onclick={() => void capcutStore.validateDraft(capcutStore.exportedPath ?? "")}
          >
            {capcutStore.validating ? t("capcutExport.validating") : t("capcutExport.validateButton")}
          </Button>
          <Button variant="ghost" size="sm" onclick={() => void capcutStore.revealDraftInExplorer(capcutStore.exportedPath ?? "")}>
            {t("capcutExport.revealButton")}
          </Button>
        </div>

        {#if capcutStore.revealError}
          <ErrorState message={capcutStore.revealError} />
        {/if}

        {#if capcutStore.validationError}
          <ErrorState message={capcutStore.validationError} />
        {/if}

        {#if capcutStore.validationReport}
          {#if capcutStore.validationReport.problems.length === 0}
            <p class="ce-ok">{t("capcutExport.validationHealthy")}</p>
          {:else}
            <div class="ce-validation-problems">
              <p class="ce-warn-strong">{t("capcutExport.validationUnhealthy")}</p>
              <ul class="ce-warn-list">
                {#each capcutStore.validationReport.problems as problem (problem)}
                  <li>{problem}</li>
                {/each}
              </ul>
            </div>
          {/if}
        {/if}
      </Panel>
    {/if}
  {/if}

  {#snippet footer()}
    {#if capcutStore.exportedPath}
      <Button onclick={() => capcutStore.startNewExport()}>{t("capcutExport.exportAnotherButton")}</Button>
    {:else if capcutStore.confirmingExport}
      <Button variant="danger" disabled={capcutStore.exporting} onclick={() => void capcutStore.confirmExport()}>
        {capcutStore.exporting ? t("capcutExport.exporting") : t("capcutExport.confirmButton")}
      </Button>
      <Button variant="ghost" disabled={capcutStore.exporting} onclick={() => capcutStore.cancelExportConfirm()}>
        {t("capcutExport.cancelButton")}
      </Button>
    {:else}
      <Button disabled={!capcutStore.canExport} onclick={() => capcutStore.requestExport()}>
        {t("capcutExport.exportButton")}
      </Button>
    {/if}
    <Button variant="ghost" onclick={() => capcutStore.closeExport()}>{t("capcutExport.close")}</Button>
  {/snippet}
</Modal>

<style>
  /* No RadioGroup component exists in the Design System yet — kept as
     hand-rolled radio inputs, unchanged in behavior. */
  .ce-radio-group {
    display: flex;
    gap: 14px;
  }
  .ce-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .ce-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .ce-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .ce-path {
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ce-path-text {
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .ce-target-label {
    margin: 0;
    font-size: 11px;
  }
  /* Compound "text + inline button" warning callout — no single Design
     System component covers this shape; colors reuse the shared
     --accent-bg/--accent-border tokens (Phase D1) instead of a per-dialog
     literal. */
  .ce-warn {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 8px 10px;
    font-size: 11px;
    color: var(--accent);
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
    border-radius: var(--radius-sm);
  }
  .ce-ok {
    margin: 0;
    font-size: 11.5px;
    color: var(--pos);
  }
  /* Compatibility-warning / validation-problem lists — no Design System
     list component exists for an arbitrary-length bullet list. */
  .ce-warn-list {
    margin: 0;
    padding-left: 18px;
    font-size: 11px;
    line-height: 1.6;
    color: var(--warn);
  }
  .ce-note {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.5;
  }
  /* Tinted "you're about to overwrite something" callout wrapping the
     confirm Panel — Panel itself is borderless by design (see its own doc
     comment), so this scoped/danger-tinted wrapper stays bespoke, now using
     the shared --neg-bg/--neg-border tokens instead of a literal. */
  .ce-confirm {
    padding: 10px 12px;
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
  }
  .ce-confirm-body {
    margin: 0;
    font-size: 11.5px;
  }
  .ce-warn-strong {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--neg);
  }
  .ce-validation-problems {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ce-success {
    margin: 0;
    padding: 8px 10px;
    font-size: 11.5px;
    color: var(--pos);
    background: var(--pos-bg);
    border: 1px solid var(--pos-border);
    border-radius: var(--radius-sm);
    word-break: break-all;
  }
</style>
