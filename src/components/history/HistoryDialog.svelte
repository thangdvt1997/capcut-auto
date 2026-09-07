<!--
  Video Processing History dialog (upgrade spec §21, `UPGRADE_PLAN.md` Phase
  U3 — the real, already-shipped History backend: `src-tauri/src/history/`,
  `commands/history.rs`). A second table, this time of finished/past jobs
  instead of live ones — deliberately reuses `BatchJobsDialog.svelte`'s own
  table/status-badge visual language (same class-naming convention, "hd-"
  prefix instead of "bj-" — Svelte scoped styles don't cross components, so a
  small amount of CSS duplication here matches how `StartBatchDialog.svelte`
  ("sb-") and `BatchJobsDialog.svelte` ("bj-") already each carry their own
  scoped styles despite a near-identical look).

  Placement: a standalone dialog reachable from its own "History…" TopBar
  button, same "no master prompt §46 Settings surface exists yet, and this
  isn't scoped to a particular open project" rationale as
  `BatchJobsDialog.svelte`'s own doc comment — a finished job's history
  outlives whatever project happened to be open when it ran.

  Per-row actions map 1:1 onto `stores/history.svelte.ts`'s own methods — see
  that module's doc comment for exactly which of §21's actions ("View,
  Download output, Re-run, Clone settings, Run with another template, View
  logs") map onto a real backend command vs. an existing mechanism elsewhere
  (most notably: "Download output" is honestly "copy the real output path",
  not a fabricated download; see that store's own writeup for the gap this
  leaves — a real "reveal in file explorer" action needs new backend surface
  this frontend-only pass could not add).

  **Phase D7c Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), the status pill is
  `Badge.svelte`, the inline template-picker `<select>` is `Select.svelte`,
  every button is `Button.svelte`, and the two empty-list messages are
  `EmptyState.svelte`. The main `<table>` itself deliberately stays
  hand-rolled (not `DataTable.svelte`): this table's per-row "View" toggle
  reveals a real detail sub-row spanning all 8 columns directly under that
  row — `DataTable.svelte`'s own Phase D1-documented scope has no concept of
  a per-row expandable sub-row (only one `<tr>` per data row), and forcing
  this through it would mean either modifying the shared component (used by
  other concurrently-retrofitted dialogs, out of scope here) or moving the
  detail view somewhere else on the page (a real layout/UX change the
  retrofit's own "chrome only, never behavior" rule forbids). Every real
  behavior is unchanged: the exact same `historyStore` state/methods drive
  every conditional, disabled state, and click handler as before.
-->
<script lang="ts">
  import { historyStore } from "../../stores/history.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import Modal from "../ui/Modal.svelte";
  import Badge from "../ui/Badge.svelte";
  import Button from "../ui/Button.svelte";
  import Select from "../ui/Select.svelte";
  import type { SelectOption } from "../ui/Select.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import type { HistoryEntry } from "../../types/bindings";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function statusLabel(status: HistoryEntry["status"]): string {
    // Reuses the exact same status-label keys `BatchJobsDialog.svelte`
    // already established (`HistoryEntry::status` is the same
    // `BatchJobStatus` type `BatchJob::status` is) — no duplicated string
    // set to keep in sync across two locales.
    return t(`batchJobs.status.${status}`);
  }

  /** Same status -> Badge-variant mapping as `BatchJobsDialog.svelte`'s own
   * `badgeVariant` (Phase D4/D7c) — kept as a local copy rather than a
   * shared import, matching that file's own precedent for this same small,
   * self-contained bit of logic. */
  function badgeVariant(status: HistoryEntry["status"]): "neutral" | "pos" | "neg" | "warn" | "accent" {
    switch (status) {
      case "completed":
        return "pos";
      case "failed":
      case "cancelled":
        return "neg";
      case "paused":
        return "warn";
      case "queued":
        return "neutral";
      default:
        return "accent";
    }
  }

  function formatDuration(us: number | null): string {
    if (us === null) return "—";
    return formatTimecode(us);
  }

  function formatStarted(rfc3339: string): string {
    const parsed = new Date(rfc3339);
    return Number.isNaN(parsed.getTime()) ? rfc3339 : parsed.toLocaleString();
  }

  function msFromUs(us: number): number {
    return Math.round(us / 1000);
  }

  let templateOptions = $derived<SelectOption[]>(
    historyStore.templates.map((tpl) => ({ value: tpl.id, label: tpl.name })),
  );
</script>

<Modal open={historyStore.dialogOpen} title={t("history.title")} onClose={() => historyStore.closeDialog()} width={1100}>
  <div class="hd-toolbar">
    <Button variant="ghost" disabled={historyStore.loading} onclick={() => void historyStore.refresh()}>
      {t("history.refreshButton")}
    </Button>
    <span class="hd-toolbar-spacer"></span>
    <Button variant="ghost" onclick={() => void historyStore.viewLogs()}>
      {t("history.viewLogsButton")}
    </Button>
  </div>

  {#if historyStore.loadError}
    <div class="hd-error">{t("history.loadFailed", { error: historyStore.loadError })}</div>
  {/if}

  {#if historyStore.entries.length === 0}
    <EmptyState title={historyStore.loading ? t("history.loading") : t("history.empty")} />
  {:else}
    <div class="hd-table-wrap">
      <table class="hd-table">
        <thead>
          <tr>
            <th>{t("history.colJob")}</th>
            <th>{t("history.colTemplate")}</th>
            <th>{t("history.colStatus")}</th>
            <th>{t("history.colDuration")}</th>
            <th>{t("history.colOutput")}</th>
            <th>{t("history.colRetries")}</th>
            <th>{t("history.colStarted")}</th>
            <th>{t("history.colActions")}</th>
          </tr>
        </thead>
        <tbody>
          {#each historyStore.entries as entry (entry.id)}
            <tr>
              <td class="hd-name" title={entry.job_name}>{entry.job_name}</td>
              <td class="muted-2">
                {#if entry.template_id}
                  {historyStore.templateName(entry.template_id)}
                  {#if entry.template_version !== null}
                    <span class="hd-version">v{entry.template_version}</span>
                  {/if}
                {:else}
                  {t("history.noTemplate")}
                {/if}
              </td>
              <td><Badge variant={badgeVariant(entry.status)}>{statusLabel(entry.status)}</Badge></td>
              <td>{formatDuration(entry.duration_us)}</td>
              <td class="hd-output" title={entry.output_path ?? undefined}>
                {entry.output_path ? basename(entry.output_path) : t("history.noOutput")}
              </td>
              <td>{entry.retry_count}</td>
              <td class="muted-2">{formatStarted(entry.started_at)}</td>
              <td class="hd-actions">
                <Button variant="ghost" size="sm" onclick={() => historyStore.toggleExpand(entry.id)}>
                  {historyStore.expandedId === entry.id ? t("history.hideButton") : t("history.viewButton")}
                </Button>

                <Button
                  variant="ghost"
                  size="sm"
                  disabled={!entry.output_path}
                  onclick={() => void historyStore.copyOutputPath(entry)}
                >
                  {historyStore.copiedId === entry.id ? t("history.copied") : t("history.copyOutputPathButton")}
                </Button>

                <Button
                  variant="ghost"
                  size="sm"
                  disabled={historyStore.actionPendingById[entry.id]}
                  onclick={() => void historyStore.rerun(entry)}
                >
                  {t("history.rerunButton")}
                </Button>

                {#if historyStore.pickingTemplateForId === entry.id}
                  <div class="hd-template-picker">
                    <Select
                      value={historyStore.pickedTemplateId ?? ""}
                      options={templateOptions}
                      placeholder={t("history.selectTemplate")}
                      onchange={(v) => historyStore.setPickedTemplateId(v)}
                    />
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={!historyStore.pickedTemplateId}
                      onclick={() => void historyStore.confirmRerunWithTemplate(entry)}
                    >
                      {t("history.runButton")}
                    </Button>
                    <Button variant="ghost" size="sm" onclick={() => historyStore.cancelTemplatePicker()}>
                      {t("history.cancelButton")}
                    </Button>
                  </div>
                {:else}
                  <Button variant="ghost" size="sm" onclick={() => historyStore.openTemplatePicker(entry)}>
                    {t("history.rerunWithTemplateButton")}
                  </Button>
                {/if}

                <Button
                  variant="ghost"
                  size="sm"
                  disabled={historyStore.actionPendingById[entry.id]}
                  onclick={() => void historyStore.cloneSettings(entry)}
                >
                  {t("history.cloneSettingsButton")}
                </Button>

                {#if historyStore.pendingDeleteId === entry.id}
                  <Button variant="danger" size="sm" onclick={() => void historyStore.confirmDelete(entry.id)}>
                    {t("history.confirmDeleteButton")}
                  </Button>
                  <Button variant="ghost" size="sm" onclick={() => historyStore.cancelDelete()}>
                    {t("history.keepButton")}
                  </Button>
                {:else}
                  <Button variant="ghost" size="sm" onclick={() => historyStore.requestDelete(entry.id)}>
                    {t("history.deleteButton")}
                  </Button>
                {/if}

                {#if historyStore.actionErrorById[entry.id]}
                  <div class="hd-row-error">{historyStore.actionErrorById[entry.id]}</div>
                {/if}
              </td>
            </tr>
            {#if historyStore.expandedId === entry.id}
              <tr class="hd-detail-row">
                <td colspan="8">
                  <div class="hd-detail">
                    <p class="hd-detail-line">
                      <span class="hd-label">{t("history.detailInput")}:</span>
                      <span class="hd-value">{entry.input_path}</span>
                    </p>
                    <p class="hd-detail-line">
                      <span class="hd-label">{t("history.detailOutput")}:</span>
                      <span class="hd-value">{entry.output_path ?? t("history.noOutput")}</span>
                    </p>
                    {#if entry.error}
                      <p class="hd-detail-line">
                        <span class="hd-label">{t("history.detailError")}:</span>
                        <span class="hd-value hd-error-text">{entry.error}</span>
                      </p>
                    {/if}
                    <p class="hd-detail-line hd-detail-subtitle">{t("history.detailExecutionPlan")}</p>
                    <p class="hd-detail-line">
                      <span class="hd-label">{t("history.detailSilenceRemoval")}:</span>
                      <span class="hd-value">
                        {#if entry.execution_plan.remove_silence}
                          {t("history.enabled")} —
                          {t("history.paddingBefore", { ms: msFromUs(entry.execution_plan.remove_silence.padding_before_us) })},
                          {t("history.paddingAfter", { ms: msFromUs(entry.execution_plan.remove_silence.padding_after_us) })},
                          {t("history.mergeGap", { ms: msFromUs(entry.execution_plan.remove_silence.merge_gap_us) })}
                        {:else}
                          {t("history.disabled")}
                        {/if}
                      </span>
                    </p>
                    <p class="hd-detail-line">
                      <span class="hd-label">{t("history.detailCaptions")}:</span>
                      <span class="hd-value">
                        {#if entry.execution_plan.captions}
                          {t("history.enabled")} — {entry.execution_plan.transcription_model_id ?? "—"}
                        {:else}
                          {t("history.disabled")}
                        {/if}
                      </span>
                    </p>
                    <p class="hd-detail-line">
                      <span class="hd-label">{t("history.detailExportPreset")}:</span>
                      <span class="hd-value">{entry.execution_plan.export_preset_id ?? "—"}</span>
                    </p>
                  </div>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>

    {#if historyStore.hasMore}
      <div class="hd-load-more">
        <Button variant="ghost" disabled={historyStore.loading} onclick={() => void historyStore.loadMore()}>
          {historyStore.loading ? t("history.loading") : t("history.loadMoreButton")}
        </Button>
      </div>
    {/if}
  {/if}
</Modal>

<style>
  /* Design System retrofit (Phase D7c, `STUDIO_PLAN.md`): the dialog shell
     (`.hd-backdrop`/`.hd-dialog`/`.hd-header`/`.hd-title`), the hand-rolled
     status pill (`.hd-status-badge*`), the hand-rolled `<select>`
     (`.hd-select`), the empty-state paragraph (`.hd-empty`), the action-
     button sizing (`.hd-action-btn`), and the danger-button override
     (`.btn-danger`) are ALL gone — `Modal`/`Badge`/`Select`/`Button`/
     `EmptyState` (Design System) now own that chrome. The plain `<table>`
     itself stays (see the file's own doc comment for why: a per-row
     full-width expandable detail sub-row has no `DataTable.svelte`
     equivalent this pass could use without changing behavior). Only the
     handful of layout rules with no Design System equivalent remain: the
     toolbar row, the table's own header/cell padding, the name/output cell
     ellipsis truncation, the actions cell's column layout, the template-
     picker's inline row layout, the detail sub-row's own text layout, and
     the inline row-error text truncation. */
  .hd-toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .hd-toolbar-spacer {
    flex: 1;
  }
  .hd-table-wrap {
    overflow-x: auto;
  }
  .hd-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11.5px;
  }
  .hd-table th {
    text-align: left;
    padding: 6px var(--space-2);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .hd-table td {
    padding: var(--space-2);
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  .hd-name {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hd-output {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hd-version {
    margin-left: 4px;
    font-size: 10px;
    color: var(--muted);
  }
  .hd-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-1);
    min-width: 220px;
  }
  .hd-template-picker {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
  }
  .hd-row-error {
    font-size: 10px;
    color: var(--neg);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    width: 100%;
  }
  .hd-detail-row td {
    background: var(--surface-2);
  }
  .hd-detail {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 4px;
  }
  .hd-detail-line {
    margin: 0;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .hd-detail-subtitle {
    margin-top: 4px;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--muted);
  }
  .hd-label {
    color: var(--muted);
    margin-right: 4px;
  }
  .hd-error-text {
    color: var(--neg);
  }
  .hd-load-more {
    display: flex;
    justify-content: center;
    padding-top: 4px;
  }
  .hd-error {
    padding: var(--space-2) var(--space-3);
    font-size: 11px;
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
  }
</style>
