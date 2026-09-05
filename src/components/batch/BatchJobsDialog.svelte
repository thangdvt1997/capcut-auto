<!--
  Batch Jobs dialog (master prompt §42/§43) — the Jobs table itself: Name/
  Status/Progress/Stage/Elapsed/ETA/Output columns exactly as specified,
  live-updating via the real `batch:progress` Tauri event
  (`stores/batch.svelte.ts`), never a polling timer. Per-row Pause/Resume/
  Cancel/Retry wire to the real `pause_batch_job`/`resume_batch_job`/
  `cancel_batch_job`/`retry_batch_job` commands, each enabled only when it
  makes sense for that row's current `BatchJobStatus`.

  Placement (task brief point 3): Batch processing is a dashboard-style,
  app-level concern — it isn't scoped to "the current project" the way most
  `Timeline.svelte` toolbar dialogs are (a batch can process media that was
  never opened as a project at all) — so this follows the exact same
  precedent as `ModelManagerDialog.svelte`/`CapCutSettingsDialog.svelte`/
  `AiSettingsDialog.svelte`: a standalone dialog, mounted once in
  `App.svelte`, reachable from its own TopBar button (no master prompt §46
  Settings surface exists yet to host this as a section either).

  Starting a new batch is a nested dialog (`StartBatchDialog.svelte`) opened
  from the header here, rather than folded into this same dialog — keeps the
  (already fairly tall) config form from permanently pushing the Jobs table
  itself off-screen once a batch is running.
-->
<script lang="ts">
  import { batchStore } from "../../stores/batch.svelte";
  import StartBatchDialog from "./StartBatchDialog.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import type { BatchJob, BatchJobStatus } from "../../types/bindings";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function statusLabel(status: BatchJobStatus): string {
    return t(`batchJobs.status.${status}`);
  }

  /** Elapsed/ETA are `i64` microseconds (task brief: never leak a raw
   * microsecond count into a label) — reuses the exact same duration
   * formatter the Timeline ruler already established rather than writing a
   * second one. */
  function formatDuration(us: number | null): string {
    if (us === null) return "—";
    return formatTimecode(us);
  }

  function canPause(status: BatchJobStatus): boolean {
    return status === "queued" || status === "analyzing" || status === "transcribing" || status === "editing" || status === "rendering";
  }
  function canResume(status: BatchJobStatus): boolean {
    return status === "paused";
  }
  function canCancel(status: BatchJobStatus): boolean {
    return status !== "completed" && status !== "failed" && status !== "cancelled";
  }
  function canRetry(status: BatchJobStatus): boolean {
    return status === "failed";
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      batchStore.closeJobsDialog();
    }
  }

  function jobKey(job: BatchJob): string {
    return job.id;
  }
</script>

{#if batchStore.jobsDialogOpen}
  <div class="bj-backdrop" role="presentation" onclick={() => batchStore.closeJobsDialog()}>
    <div
      class="bj-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("batchJobs.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="bj-header">
        <span class="bj-title">{t("batchJobs.title")}</span>
        <button class="btn btn-ghost" onclick={() => batchStore.closeJobsDialog()} title={t("batchJobs.close")}>×</button>
      </div>

      <div class="bj-toolbar">
        {#if batchStore.batches.length > 0}
          <select
            class="bj-batch-select"
            value={batchStore.selectedBatchId ?? ""}
            onchange={(e) => batchStore.selectBatch((e.target as HTMLSelectElement).value)}
          >
            {#each batchStore.batches as b (b.id)}
              <option value={b.id}>
                {t("batchJobs.batchOption", { count: b.fileCount, time: new Date(b.createdAtMs).toLocaleTimeString() })}
              </option>
            {/each}
          </select>
          <button class="btn btn-ghost" onclick={() => void batchStore.refreshSelectedBatch()}>
            {t("batchJobs.refreshButton")}
          </button>
        {/if}
        <span class="bj-toolbar-spacer"></span>
        <button class="btn" onclick={() => batchStore.openStartDialog()}>{t("batchJobs.startNewButton")}</button>
      </div>

      <div class="bj-body">
        {#if batchStore.batches.length === 0}
          <p class="bj-empty muted-2">{t("batchJobs.noBatchesYet")}</p>
        {:else if batchStore.jobsForSelectedBatch.length === 0}
          <p class="bj-empty muted-2">{t("batchJobs.noJobsInBatch")}</p>
        {:else}
          <div class="bj-table-wrap">
            <table class="bj-table">
              <thead>
                <tr>
                  <th>{t("batchJobs.colName")}</th>
                  <th>{t("batchJobs.colStatus")}</th>
                  <th>{t("batchJobs.colProgress")}</th>
                  <th>{t("batchJobs.colStage")}</th>
                  <th>{t("batchJobs.colElapsed")}</th>
                  <th>{t("batchJobs.colEta")}</th>
                  <th>{t("batchJobs.colOutput")}</th>
                  <th>{t("batchJobs.colActions")}</th>
                </tr>
              </thead>
              <tbody>
                {#each batchStore.jobsForSelectedBatch as job (jobKey(job))}
                  <tr>
                    <td class="bj-name" title={job.name}>{job.name}</td>
                    <td>
                      <span class="bj-status-badge" data-status={job.status}>{statusLabel(job.status)}</span>
                    </td>
                    <td class="bj-progress-cell">
                      <div class="bj-progress-track">
                        <div class="bj-progress-fill" style="width:{Math.round(job.progress * 100)}%"></div>
                      </div>
                      <span class="bj-progress-label muted-2">{Math.round(job.progress * 100)}%</span>
                    </td>
                    <td class="muted-2">{job.stage}</td>
                    <td>{formatDuration(job.elapsed_us)}</td>
                    <td>{formatDuration(job.eta_us)}</td>
                    <td class="bj-output" title={job.output_path ?? undefined}>
                      {job.output_path ? basename(job.output_path) : "—"}
                    </td>
                    <td class="bj-actions">
                      {#if canPause(job.status)}
                        <button
                          class="btn btn-ghost bj-action-btn"
                          disabled={batchStore.actionPendingByJob[job.id]}
                          onclick={() => void batchStore.pause(job.id)}
                        >
                          {t("batchJobs.pauseButton")}
                        </button>
                      {/if}
                      {#if canResume(job.status)}
                        <button
                          class="btn btn-ghost bj-action-btn"
                          disabled={batchStore.actionPendingByJob[job.id]}
                          onclick={() => void batchStore.resume(job.id)}
                        >
                          {t("batchJobs.resumeButton")}
                        </button>
                      {/if}
                      {#if canCancel(job.status)}
                        {#if batchStore.pendingCancelId === job.id}
                          <button
                            class="btn btn-danger bj-action-btn"
                            disabled={batchStore.actionPendingByJob[job.id]}
                            onclick={() => void batchStore.confirmCancel(job.id)}
                          >
                            {t("batchJobs.confirmCancelButton")}
                          </button>
                          <button class="btn btn-ghost bj-action-btn" onclick={() => batchStore.cancelCancelRequest()}>
                            {t("batchJobs.keepJobButton")}
                          </button>
                        {:else}
                          <button class="btn btn-ghost bj-action-btn" onclick={() => batchStore.requestCancel(job.id)}>
                            {t("batchJobs.cancelButton")}
                          </button>
                        {/if}
                      {/if}
                      {#if canRetry(job.status)}
                        <button
                          class="btn btn-ghost bj-action-btn"
                          disabled={batchStore.actionPendingByJob[job.id]}
                          onclick={() => void batchStore.retry(job.id)}
                        >
                          {t("batchJobs.retryButton")}
                        </button>
                      {/if}
                      {#if batchStore.actionErrorByJob[job.id]}
                        <div class="bj-row-error">{batchStore.actionErrorByJob[job.id]}</div>
                      {/if}
                      {#if job.error}
                        <div class="bj-row-error" title={job.error}>{job.error}</div>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<StartBatchDialog />

<style>
  .bj-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .bj-dialog {
    width: min(1000px, 96vw);
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
  .bj-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .bj-title {
    font-size: 13px;
    font-weight: 600;
  }
  .bj-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .bj-toolbar-spacer { flex: 1; }
  .bj-batch-select {
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
    padding: 0 6px;
    max-width: 320px;
  }
  .bj-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 12px 14px;
  }
  .bj-empty {
    margin: 0;
    font-size: 12px;
    text-align: center;
    padding: 24px 0;
  }
  .bj-table-wrap {
    overflow-x: auto;
  }
  .bj-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 11.5px;
  }
  .bj-table th {
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
  .bj-table td {
    padding: 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }
  .bj-name {
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bj-output {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bj-status-badge {
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
  .bj-status-badge[data-status="completed"] {
    color: var(--pos, #3fb950);
    border-color: hsl(140 60% 50% / 0.4);
    background: hsl(140 60% 50% / 0.1);
  }
  .bj-status-badge[data-status="failed"] {
    color: var(--neg);
    border-color: hsl(0 84% 65% / 0.4);
    background: hsl(0 84% 65% / 0.1);
  }
  .bj-status-badge[data-status="cancelled"] {
    color: var(--muted);
    border-color: var(--border-strong);
  }
  .bj-status-badge[data-status="paused"] {
    color: hsl(45 90% 60%);
    border-color: hsl(45 90% 60% / 0.4);
    background: hsl(45 90% 60% / 0.1);
  }
  .bj-status-badge[data-status="analyzing"],
  .bj-status-badge[data-status="transcribing"],
  .bj-status-badge[data-status="editing"],
  .bj-status-badge[data-status="rendering"] {
    color: var(--accent);
    border-color: hsl(213 94% 68% / 0.4);
    background: hsl(213 94% 68% / 0.1);
  }
  .bj-progress-cell {
    min-width: 140px;
  }
  .bj-progress-track {
    height: 6px;
    background: var(--surface-2);
    border-radius: 3px;
    overflow: hidden;
  }
  .bj-progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .bj-progress-label {
    display: block;
    margin-top: 3px;
    font-size: 10px;
  }
  .bj-actions {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 160px;
  }
  .bj-action-btn {
    padding: 3px 8px;
    font-size: 10.5px;
    height: auto;
  }
  .bj-row-error {
    font-size: 10px;
    color: var(--neg);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
</style>
