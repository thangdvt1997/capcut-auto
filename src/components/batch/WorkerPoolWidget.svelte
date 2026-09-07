<!--
  Phase D4 (`STUDIO_PLAN.md`): the real "Workers: N · Running: R ·
  Queued: Q" Worker/Slot status widget — the frontend half of Phase D4a's
  `get_worker_pool_status` backend rearchitecture. Mounted in two places
  (task brief): `JobQueuePanel.svelte` (Tab 1's glance panel) and
  `BatchJobsDialog.svelte` (the full dialog) — both just drop in
  `<WorkerPoolWidget />`, no props, since both read the exact same
  `batchStore` singleton.

  Polling contract (task brief: "poll on a short interval only while
  there's reason to... don't poll forever when idle"):
  - One immediate, unconditional fetch on mount, so the widget shows a
    real snapshot (not a blank dash) the instant it appears, even if no
    job is active right now.
  - A 2s `setInterval` that starts/stops as `batchStore.anyJobActive`
    flips true/false — driven from *this* component's own `$effect`, not
    the store: a plain store class has no lifecycle of its own to hook a
    timer into (see `stores/autoZoom.svelte.ts`'s own doc comment for this
    exact precedent), so the component that's actually mounted/unmounted
    owns the interval.
  - The store itself also does two more eager, non-interval refreshes:
    right after a batch is started (`startBatch`/`startMultiTemplateBatch`/
    `adoptExternalBatch`) and right after any job newly reaches a terminal
    status (a worker slot just freed up) — see `stores/batch.svelte.ts`'s
    own comments for why those two moments specifically, and not every
    in-flight progress tick.

  If both this dialog and Tab 1's panel are mounted at once (Job Queue
  dialog opened while Tab 1 is still on screen), two independent 2s
  intervals run — a small, deliberately-accepted duplication rather than
  centralizing interval ownership in the store for what's at most one
  extra IPC call every 2s.
-->
<script lang="ts">
  import { batchStore } from "../../stores/batch.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Badge from "../ui/Badge.svelte";

  const POLL_INTERVAL_MS = 2000;

  // Unconditional first fetch on mount — no reactive dependency read here,
  // so this `$effect` runs exactly once and never re-fires on its own.
  $effect(() => {
    void batchStore.refreshWorkerPoolStatus();
  });

  // Poll only while there's a real reason to (task brief) — starts/stops
  // as `anyJobActive` changes.
  $effect(() => {
    if (!batchStore.anyJobActive) return;
    const id = setInterval(() => void batchStore.refreshWorkerPoolStatus(), POLL_INTERVAL_MS);
    return () => clearInterval(id);
  });

  let status = $derived(batchStore.workerPoolStatus);
</script>

<div class="wp-widget" role="status" aria-label={t("workerPool.ariaLabel")} title={batchStore.workerPoolStatusError ?? undefined}>
  <Badge variant="neutral">{t("workerPool.workers", { count: status?.workers ?? 0 })}</Badge>
  <Badge variant={status && status.running > 0 ? "accent" : "neutral"}
    >{t("workerPool.running", { count: status?.running ?? 0 })}</Badge
  >
  <Badge variant={status && status.queued > 0 ? "warn" : "neutral"}
    >{t("workerPool.queued", { count: status?.queued ?? 0 })}</Badge
  >
</div>

<style>
  .wp-widget {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
</style>
