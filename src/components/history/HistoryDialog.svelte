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
-->
<script lang="ts">
  import { historyStore } from "../../stores/history.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
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

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      historyStore.closeDialog();
    }
  }
</script>

{#if historyStore.dialogOpen}
  <div class="hd-backdrop" role="presentation" onclick={() => historyStore.closeDialog()}>
    <div
      class="hd-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("history.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="hd-header">
        <span class="hd-title">{t("history.title")}</span>
        <button class="btn btn-ghost" onclick={() => historyStore.closeDialog()} title={t("history.close")}>×</button>
      </div>

      <div class="hd-toolbar">
        <button class="btn btn-ghost" disabled={historyStore.loading} onclick={() => void historyStore.refresh()}>
          {t("history.refreshButton")}
        </button>
        <span class="hd-toolbar-spacer"></span>
        <button class="btn btn-ghost" onclick={() => void historyStore.viewLogs()}>
          {t("history.viewLogsButton")}
        </button>
      </div>

      <div class="hd-body">
        {#if historyStore.loadError}
          <div class="hd-error">{t("history.loadFailed", { error: historyStore.loadError })}</div>
        {/if}

        {#if historyStore.entries.length === 0}
          <p class="hd-empty muted-2">
            {historyStore.loading ? t("history.loading") : t("history.empty")}
          </p>
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
                    <td><span class="hd-status-badge" data-status={entry.status}>{statusLabel(entry.status)}</span></td>
                    <td>{formatDuration(entry.duration_us)}</td>
                    <td class="hd-output" title={entry.output_path ?? undefined}>
                      {entry.output_path ? basename(entry.output_path) : t("history.noOutput")}
                    </td>
                    <td>{entry.retry_count}</td>
                    <td class="muted-2">{formatStarted(entry.started_at)}</td>
                    <td class="hd-actions">
                      <button class="btn btn-ghost hd-action-btn" onclick={() => historyStore.toggleExpand(entry.id)}>
                        {historyStore.expandedId === entry.id ? t("history.hideButton") : t("history.viewButton")}
                      </button>

                      <button
                        class="btn btn-ghost hd-action-btn"
                        disabled={!entry.output_path}
                        onclick={() => void historyStore.copyOutputPath(entry)}
                      >
                        {historyStore.copiedId === entry.id ? t("history.copied") : t("history.copyOutputPathButton")}
                      </button>

                      <button
                        class="btn btn-ghost hd-action-btn"
                        disabled={historyStore.actionPendingById[entry.id]}
                        onclick={() => void historyStore.rerun(entry)}
                      >
                        {t("history.rerunButton")}
                      </button>

                      {#if historyStore.pickingTemplateForId === entry.id}
                        <div class="hd-template-picker">
                          <select
                            class="hd-select"
                            value={historyStore.pickedTemplateId ?? ""}
                            onchange={(e) => historyStore.setPickedTemplateId((e.target as HTMLSelectElement).value)}
                          >
                            <option value="" disabled>{t("history.selectTemplate")}</option>
                            {#each historyStore.templates as tpl (tpl.id)}
                              <option value={tpl.id}>{tpl.name}</option>
                            {/each}
                          </select>
                          <button
                            class="btn btn-ghost hd-action-btn"
                            disabled={!historyStore.pickedTemplateId}
                            onclick={() => void historyStore.confirmRerunWithTemplate(entry)}
                          >
                            {t("history.runButton")}
                          </button>
                          <button class="btn btn-ghost hd-action-btn" onclick={() => historyStore.cancelTemplatePicker()}>
                            {t("history.cancelButton")}
                          </button>
                        </div>
                      {:else}
                        <button class="btn btn-ghost hd-action-btn" onclick={() => historyStore.openTemplatePicker(entry)}>
                          {t("history.rerunWithTemplateButton")}
                        </button>
                      {/if}

                      <button
                        class="btn btn-ghost hd-action-btn"
                        disabled={historyStore.actionPendingById[entry.id]}
                        onclick={() => void historyStore.cloneSettings(entry)}
                      >
                        {t("history.cloneSettingsButton")}
                      </button>

                      {#if historyStore.pendingDeleteId === entry.id}
                        <button class="btn btn-danger hd-action-btn" onclick={() => void historyStore.confirmDelete(entry.id)}>
                          {t("history.confirmDeleteButton")}
                        </button>
                        <button class="btn btn-ghost hd-action-btn" onclick={() => historyStore.cancelDelete()}>
                          {t("history.keepButton")}
                        </button>
                      {:else}
                        <button class="btn btn-ghost hd-action-btn" onclick={() => historyStore.requestDelete(entry.id)}>
                          {t("history.deleteButton")}
                        </button>
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
              <button class="btn btn-ghost" disabled={historyStore.loading} onclick={() => void historyStore.loadMore()}>
                {historyStore.loading ? t("history.loading") : t("history.loadMoreButton")}
              </button>
            </div>
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .hd-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .hd-dialog {
    width: min(1100px, 96vw);
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
  .hd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .hd-title {
    font-size: 13px;
    font-weight: 600;
  }
  .hd-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .hd-toolbar-spacer { flex: 1; }
  .hd-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hd-empty {
    margin: 0;
    font-size: 12px;
    text-align: center;
    padding: 24px 0;
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
    padding: 6px 8px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .hd-table td {
    padding: 8px;
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
  .hd-status-badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 600;
    white-space: nowrap;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .hd-status-badge[data-status="completed"] {
    color: var(--pos, #3fb950);
    border-color: hsl(140 60% 50% / 0.4);
    background: hsl(140 60% 50% / 0.1);
  }
  .hd-status-badge[data-status="failed"] {
    color: var(--neg);
    border-color: hsl(0 84% 65% / 0.4);
    background: hsl(0 84% 65% / 0.1);
  }
  .hd-status-badge[data-status="cancelled"] {
    color: var(--muted);
    border-color: var(--border-strong);
  }
  .hd-status-badge[data-status="paused"] {
    color: hsl(45 90% 60%);
    border-color: hsl(45 90% 60% / 0.4);
    background: hsl(45 90% 60% / 0.1);
  }
  .hd-status-badge[data-status="analyzing"],
  .hd-status-badge[data-status="transcribing"],
  .hd-status-badge[data-status="editing"],
  .hd-status-badge[data-status="rendering"] {
    color: var(--accent);
    border-color: hsl(213 94% 68% / 0.4);
    background: hsl(213 94% 68% / 0.1);
  }
  .hd-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 4px;
    min-width: 220px;
  }
  .hd-action-btn {
    padding: 3px 8px;
    font-size: 10.5px;
    height: auto;
  }
  .hd-template-picker {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }
  .hd-select {
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 10.5px;
    padding: 0 4px;
    max-width: 140px;
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
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
</style>
