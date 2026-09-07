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

  **Phase D4 Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), the plain
  `<table>` is now `DataTable.svelte` (real client-side sort added on every
  column with a natural accessor — Name/Status/Progress/Stage/Elapsed/ETA —
  a genuine, non-fabricated addition the retrofit enables for free, not a
  behavior change to anything that existed before), the status pill is
  `Badge.svelte`, the progress track is `ProgressBar.svelte`, the native
  `<select>` is `Select.svelte`, the two empty-list messages are
  `EmptyState.svelte`, and every action button is `Button.svelte`. Every
  real behavior is unchanged: live updates still come from the exact same
  `batch:progress`-driven `batchStore` state, every per-row Pause/Resume/
  Cancel/Retry condition (`canPause`/`canResume`/`canCancel`/`canRetry`) and
  the two-step Cancel confirm are byte-for-byte the same logic, just
  rendered through Design System primitives instead of hand-rolled markup.
  Also gained a real, live `WorkerPoolWidget` (Phase D4's own new
  "Workers: N · Running: R · Queued: Q" snapshot) in the toolbar.
-->
<script lang="ts">
  import { batchStore } from "../../stores/batch.svelte";
  import StartBatchDialog from "./StartBatchDialog.svelte";
  import WorkerPoolWidget from "./WorkerPoolWidget.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import Modal from "../ui/Modal.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Badge from "../ui/Badge.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import Button from "../ui/Button.svelte";
  import Select from "../ui/Select.svelte";
  import type { SelectOption } from "../ui/Select.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
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

  function jobKey(job: BatchJob): string {
    return job.id;
  }

  /** Same status -> Badge-variant mapping as `JobQueuePanel.svelte`'s own
   * `badgeVariant` (Phase D2+D3) — kept as a local copy rather than a
   * shared import since both are small, self-contained, and each file's
   * own `BatchJobStatus` switch is easier to audit inline than a shared
   * helper module for four lines of logic. */
  function badgeVariant(status: BatchJobStatus): "neutral" | "pos" | "neg" | "warn" | "accent" {
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

  let batchOptions = $derived<SelectOption[]>(
    batchStore.batches.map((b) => ({
      value: b.id,
      label: t("batchJobs.batchOption", { count: b.fileCount, time: new Date(b.createdAtMs).toLocaleTimeString() }),
    })),
  );
</script>

{#snippet nameCell(job: BatchJob)}
  <span class="bj-name" title={job.name}>{job.name}</span>
{/snippet}
{#snippet statusCell(job: BatchJob)}
  <Badge variant={badgeVariant(job.status)}>{statusLabel(job.status)}</Badge>
{/snippet}
{#snippet progressCell(job: BatchJob)}
  <div class="bj-progress-cell">
    <ProgressBar value={job.progress} label={`${Math.round(job.progress * 100)}%`} />
  </div>
{/snippet}
{#snippet stageCell(job: BatchJob)}
  <span class="muted-2">{job.stage}</span>
{/snippet}
{#snippet elapsedCell(job: BatchJob)}
  {formatDuration(job.elapsed_us)}
{/snippet}
{#snippet etaCell(job: BatchJob)}
  {formatDuration(job.eta_us)}
{/snippet}
{#snippet outputCell(job: BatchJob)}
  <span class="bj-output" title={job.output_path ?? undefined}>
    {job.output_path ? basename(job.output_path) : "—"}
  </span>
{/snippet}
{#snippet actionsCell(job: BatchJob)}
  <div class="bj-actions">
    {#if canPause(job.status)}
      <Button
        variant="ghost"
        size="sm"
        disabled={batchStore.actionPendingByJob[job.id]}
        onclick={() => void batchStore.pause(job.id)}
      >
        {t("batchJobs.pauseButton")}
      </Button>
    {/if}
    {#if canResume(job.status)}
      <Button
        variant="ghost"
        size="sm"
        disabled={batchStore.actionPendingByJob[job.id]}
        onclick={() => void batchStore.resume(job.id)}
      >
        {t("batchJobs.resumeButton")}
      </Button>
    {/if}
    {#if canCancel(job.status)}
      {#if batchStore.pendingCancelId === job.id}
        <Button
          variant="danger"
          size="sm"
          disabled={batchStore.actionPendingByJob[job.id]}
          onclick={() => void batchStore.confirmCancel(job.id)}
        >
          {t("batchJobs.confirmCancelButton")}
        </Button>
        <Button variant="ghost" size="sm" onclick={() => batchStore.cancelCancelRequest()}>
          {t("batchJobs.keepJobButton")}
        </Button>
      {:else}
        <Button variant="ghost" size="sm" onclick={() => batchStore.requestCancel(job.id)}>
          {t("batchJobs.cancelButton")}
        </Button>
      {/if}
    {/if}
    {#if canRetry(job.status)}
      <Button
        variant="ghost"
        size="sm"
        disabled={batchStore.actionPendingByJob[job.id]}
        onclick={() => void batchStore.retry(job.id)}
      >
        {t("batchJobs.retryButton")}
      </Button>
    {/if}
    {#if batchStore.actionErrorByJob[job.id]}
      <div class="bj-row-error">{batchStore.actionErrorByJob[job.id]}</div>
    {/if}
    {#if job.error}
      <div class="bj-row-error" title={job.error}>{job.error}</div>
    {/if}
  </div>
{/snippet}

<Modal open={batchStore.jobsDialogOpen} title={t("batchJobs.title")} onClose={() => batchStore.closeJobsDialog()} width={1000}>
  <div class="bj-toolbar">
    {#if batchStore.batches.length > 0}
      <Select
        value={batchStore.selectedBatchId ?? ""}
        options={batchOptions}
        onchange={(v) => batchStore.selectBatch(v)}
      />
      <Button variant="ghost" size="sm" onclick={() => void batchStore.refreshSelectedBatch()}>
        {t("batchJobs.refreshButton")}
      </Button>
    {/if}
    <span class="bj-toolbar-spacer"></span>
    <WorkerPoolWidget />
    <Button size="sm" onclick={() => batchStore.openStartDialog()}>{t("batchJobs.startNewButton")}</Button>
  </div>

  <div class="bj-body">
    {#if batchStore.batches.length === 0}
      <EmptyState title={t("batchJobs.noBatchesYet")} />
    {:else if batchStore.jobsForSelectedBatch.length === 0}
      <EmptyState title={t("batchJobs.noJobsInBatch")} />
    {:else}
      <DataTable
        columns={[
          { key: "name", label: t("batchJobs.colName"), sortable: true, accessor: (j) => j.name, cell: nameCell },
          { key: "status", label: t("batchJobs.colStatus"), sortable: true, accessor: (j) => j.status, cell: statusCell },
          {
            key: "progress",
            label: t("batchJobs.colProgress"),
            sortable: true,
            accessor: (j) => j.progress,
            cell: progressCell,
          },
          { key: "stage", label: t("batchJobs.colStage"), sortable: true, accessor: (j) => j.stage, cell: stageCell },
          {
            key: "elapsed",
            label: t("batchJobs.colElapsed"),
            sortable: true,
            accessor: (j) => j.elapsed_us ?? -1,
            cell: elapsedCell,
          },
          { key: "eta", label: t("batchJobs.colEta"), sortable: true, accessor: (j) => j.eta_us ?? -1, cell: etaCell },
          { key: "output", label: t("batchJobs.colOutput"), cell: outputCell },
          { key: "actions", label: t("batchJobs.colActions"), cell: actionsCell },
        ]}
        rows={batchStore.jobsForSelectedBatch}
        rowKey={jobKey}
      />
    {/if}
  </div>
</Modal>

<StartBatchDialog />

<style>
  /* Design System retrofit (Phase D4, `STUDIO_PLAN.md`): the dialog shell
     (`.bj-backdrop`/`.bj-dialog`/`.bj-header`/`.bj-title`), the plain
     `<table>` markup (`.bj-table*`), the hand-rolled status pill
     (`.bj-status-badge*`), the hand-rolled progress track
     (`.bj-progress-track`/`.bj-progress-fill`/`.bj-progress-label`), the
     hand-rolled `<select>` (`.bj-batch-select`), the empty-state paragraph
     (`.bj-empty`), the action-button sizing (`.bj-action-btn`), and the
     danger-button override (`.btn-danger`) are ALL gone — `Modal`/
     `DataTable`/`Badge`/`ProgressBar`/`Select`/`EmptyState`/`Button` (Design
     System, Phase D1) now own that chrome. Only the handful of layout
     classes with no Design System equivalent remain: the toolbar row, its
     spacer, and three cell-content tweaks (name/output ellipsis, the
     progress cell's min-width, the actions cell's column layout, the
     inline row-error text). */
  .bj-toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .bj-toolbar-spacer {
    flex: 1;
  }
  .bj-body {
    min-width: 0;
  }
  .bj-name {
    display: block;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bj-output {
    display: block;
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bj-progress-cell {
    min-width: 140px;
  }
  .bj-actions {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    min-width: 140px;
  }
  .bj-row-error {
    font-size: 10px;
    color: var(--neg);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
