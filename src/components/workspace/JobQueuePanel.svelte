<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): Tab 1 ("Workspace")'s Job Queue summary —
  promt.md §28's docked "JOB QUEUE" panel.

  **Integration approach (per the task brief, explicitly allowed):** this
  is a real, live, `DataTable`-based summary reusing `stores/batch.svelte.ts`
  as-is (same `batch:progress`-driven `jobsById`, no new backend surface,
  no polling) — NOT a redesign of `BatchJobsDialog.svelte`'s own Jobs table
  (that's Phase D4's separate, larger scope: pause/resume/cancel/retry
  actions, the batch picker, `StartBatchDialog`). "Open Job Queue" opens the
  existing dialog unchanged for anyone who needs those per-row actions —
  Tab 1 itself just needs a real, honest "what's running right now" glance
  plus a real path to the full dialog, which this provides.

  Rows are every job across every batch this session has seen a
  `batch:progress` event for (`Object.values(jobsById)`), not scoped to one
  "selected" batch — Tab 1 is a dashboard-style glance, not a per-batch
  drill-down (that's what the dialog itself is for).
-->
<script lang="ts">
  import { batchStore } from "../../stores/batch.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Panel from "../ui/Panel.svelte";
  import Button from "../ui/Button.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Badge from "../ui/Badge.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import type { BatchJob, BatchJobStatus } from "../../types/bindings";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

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

  let allJobs = $derived(Object.values(batchStore.jobsById));
</script>

{#snippet nameCell(row: BatchJob)}
  <span class="job-name" title={row.name}>{basename(row.name)}</span>
{/snippet}
{#snippet stageCell(row: BatchJob)}
  <span class="muted-2">{row.stage}</span>
{/snippet}
{#snippet progressCell(row: BatchJob)}
  <ProgressBar value={row.progress} label={`${Math.round(row.progress * 100)}%`} />
{/snippet}
{#snippet statusCell(row: BatchJob)}
  <Badge variant={badgeVariant(row.status)}>{t(`batchJobs.status.${row.status}`)}</Badge>
{/snippet}

<Panel title={t("workspaceTab.jobQueue.title")}>
  {#snippet actions()}
    <Button size="sm" onclick={() => batchStore.openJobsDialog()}>{t("workspaceTab.jobQueue.openButton")}</Button>
  {/snippet}

  <DataTable
    columns={[
      { key: "name", label: t("workspaceTab.jobQueue.colVideo"), cell: nameCell },
      { key: "stage", label: t("workspaceTab.jobQueue.colStage"), cell: stageCell },
      { key: "progress", label: t("workspaceTab.jobQueue.colProgress"), cell: progressCell },
      { key: "status", label: t("workspaceTab.jobQueue.colStatus"), cell: statusCell },
    ]}
    rows={allJobs}
    rowKey={(j) => j.id}
    emptyMessage={t("workspaceTab.jobQueue.empty")}
  />
</Panel>

<style>
  .job-name {
    display: block;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
