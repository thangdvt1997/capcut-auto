<!--
  Professional Dashboard Header (Phase D13, `STUDIO_PLAN.md` "Dashboard
  Header + Status Bar", promt.md §13 "PROFESSIONAL DASHBOARD HEADER"): a real
  Project/Queue/Workers/AI Status/Voice API Status strip, matching promt.md's
  own worked example in spirit ("Project: Movie-ES-001 · Queue 18 Running 3
  Done 41 Failed 1 · AI ● Connected · Voice ● Connected") — not pixel-for-
  pixel, and never a fabricated number: every value here comes from a real
  store already built for exactly this purpose.

  **Real data sources only, per this codebase's own discipline:**
  - **Project**: `stores/timeline.svelte.ts`'s real `project.project.name` —
    the same `ProjectMeta.name` `TopBar.svelte`'s own status chip already
    reads. `null` (no project created/loaded yet) shows an honest "no
    project open" state, never a fabricated name.
  - **Queue/Workers**: `stores/batch.svelte.ts`'s real `workerPoolStatus`
    (`{ workers, running, queued }`, off Phase D4a's `get_worker_pool_status`
    command) — reused directly, not reimplemented (task brief: "reuse the
    real get_worker_pool_status command already wired via stores/
    batch.svelte.ts... import and use the existing store").
  - **Done/Failed**: a real, honest count of every job this session's
    `batchStore.jobsById` map currently holds with a terminal `completed`/
    `failed` status — the same "this session's own memory" scope
    `BatchSummary`'s own doc comment already documents (no backend "every job
    ever run" command exists), surfaced via the `ariaLabel`/tooltip so it's
    never mistaken for an all-time total.
  - **AI Status / Voice API Status**: `stores/aiSettings.svelte.ts` /
    `stores/voiceSettings.svelte.ts`'s real `testResult` (set only by an
    actual `test_ai_connection`/`test_voice_connection` call this session) —
    `null` (no test run yet) renders as an honest "Not tested" `Badge`,
    **never** a fabricated "Connected" (task brief's own explicit
    instruction).

  Polling: none of this component's own — every value it reads is already
  either a plain derived computation over state some other component/store
  keeps fresh (`batchStore`'s own `batch:progress` listener + eager
  refreshes, matching `WorkerPoolWidget.svelte`'s own precedent) or a value
  that only changes when the user takes a real action (running a connection
  test, creating/loading a project). No new interval needed here.

  Placement: mounted once in `App.svelte`, as its own shell row directly
  below `TopBar` and above the 3-tab strip — "a persistent header surface"
  per the task brief, not scoped to any one of the 3 top-level tabs (project
  identity, queue/worker load, and AI/Voice connectivity are all
  app-level/cross-cutting concerns, the same reasoning `ActivityLogPanel`'s
  own placement doc comment already established for the bottom log panel).
-->
<script lang="ts">
  import { timeline } from "../../stores/timeline.svelte";
  import { batchStore } from "../../stores/batch.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { voiceSettingsStore } from "../../stores/voiceSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Badge from "../ui/Badge.svelte";

  type ConnStatus = "connected" | "failed" | "notTested";

  function connStatusOf(testResult: { success: boolean } | null): ConnStatus {
    if (testResult === null) return "notTested";
    return testResult.success ? "connected" : "failed";
  }

  function badgeVariantOf(status: ConnStatus): "pos" | "neg" | "neutral" {
    if (status === "connected") return "pos";
    if (status === "failed") return "neg";
    return "neutral";
  }

  function labelOf(status: ConnStatus): string {
    if (status === "connected") return t("dashboardHeader.connected");
    if (status === "failed") return t("dashboardHeader.notConnected");
    return t("dashboardHeader.notTested");
  }

  let projectName = $derived(timeline.project?.project.name ?? null);

  let aiStatus = $derived(connStatusOf(aiSettingsStore.testResult));
  let voiceStatus = $derived(connStatusOf(voiceSettingsStore.testResult));

  /** Real, session-scoped counts (see doc comment above) — no backend
   * "every job ever run" command exists, so this only ever reflects jobs
   * this session's `batchStore` already knows about, same honest scope
   * `BatchSummary`'s own doc comment documents. */
  let doneCount = $derived(
    Object.values(batchStore.jobsById).filter((j) => j.status === "completed").length,
  );
  let failedCount = $derived(
    Object.values(batchStore.jobsById).filter((j) => j.status === "failed").length,
  );

  let workerPoolStatus = $derived(batchStore.workerPoolStatus);
</script>

<div class="dh-shell" role="region" aria-label={t("dashboardHeader.ariaLabel")}>
  <span class="dh-brand">{t("common.appName")}</span>

  <span class="dh-project">
    {#if projectName}
      {t("dashboardHeader.project", { name: projectName })}
    {:else}
      <span class="muted-2">{t("dashboardHeader.noProject")}</span>
    {/if}
  </span>

  <span class="dh-divider" aria-hidden="true"></span>

  <span class="dh-group" title={t("dashboardHeader.queueGroupTooltip")}>
    <Badge variant="neutral">{t("dashboardHeader.queue", { count: workerPoolStatus?.queued ?? 0 })}</Badge>
    <Badge variant={workerPoolStatus && workerPoolStatus.running > 0 ? "accent" : "neutral"}>
      {t("workerPool.running", { count: workerPoolStatus?.running ?? 0 })}
    </Badge>
    <Badge variant="neutral">{t("dashboardHeader.workers", { count: workerPoolStatus?.workers ?? 0 })}</Badge>
    <Badge variant={doneCount > 0 ? "pos" : "neutral"}>{t("dashboardHeader.done", { count: doneCount })}</Badge>
    <Badge variant={failedCount > 0 ? "neg" : "neutral"}>{t("dashboardHeader.failed", { count: failedCount })}</Badge>
  </span>

  <span class="dh-divider" aria-hidden="true"></span>

  <span class="dh-group">
    <span class="dh-label muted-2">{t("dashboardHeader.aiLabel")}</span>
    <Badge variant={badgeVariantOf(aiStatus)}>{labelOf(aiStatus)}</Badge>
    <span class="dh-label muted-2">{t("dashboardHeader.voiceLabel")}</span>
    <Badge variant={badgeVariantOf(voiceStatus)}>{labelOf(voiceStatus)}</Badge>
  </span>
</div>

<style>
  .dh-shell {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--surface-1);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }
  .dh-brand {
    font-weight: 700;
    letter-spacing: 0.03em;
    flex-shrink: 0;
  }
  .dh-project {
    flex-shrink: 0;
  }
  .dh-divider {
    width: 1px;
    align-self: stretch;
    background: var(--border);
    flex-shrink: 0;
  }
  .dh-group {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    flex-wrap: wrap;
  }
  .dh-label {
    font-size: 10.5px;
  }
</style>
