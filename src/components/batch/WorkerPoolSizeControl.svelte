<!--
  Phase D11 (`STUDIO_PLAN.md`): a real control for `max_concurrent_jobs` —
  closes Phase D4a's own explicitly-flagged gap ("no settings-persistence UI
  exists yet for this value"). Mounted next to `WorkerPoolWidget` in
  `BatchJobsDialog.svelte`'s toolbar (task brief's own suggested placement).

  **This is a real, honest "restart to apply" setting, not a live-resizable
  one** — see `src-tauri/src/batch/settings.rs` module doc comment for the
  full reasoning (short version: safely stopping exactly N of the
  currently-running M worker threads would require changing
  `BatchJobManager::pop_blocking`'s own condvar loop, which this pass was
  explicitly told not to touch; growing the pool live would have been safe,
  but a setting that only works in one direction is worse than one that
  honestly always says "restart to apply"). So saving a new value here never
  touches `batchStore.workerPoolStatus` (the live pool) — only the value
  the *next* app startup will read.

  The input always shows the persisted value (what the next restart will
  use); the `restartToApply` badge appears only while the persisted value
  and the currently-*active* pool size actually differ — so a user who
  hasn't changed anything since their last restart never sees a stale
  warning.

  Uses `NumberInput` (Phase D9's own real numeric-input Design System
  primitive — checked before hand-rolling anything) with one-way `value` +
  `onchange` (commit on blur/Enter, not every keystroke), the exact same
  interaction shape `NumberInput.svelte`'s own doc comment documents for
  `AiSettingsDialog.svelte`'s Timeout field.
-->
<script lang="ts">
  import { batchStore } from "../../stores/batch.svelte";
  import { t } from "../../lib/i18n.svelte";
  import NumberInput from "../ui/NumberInput.svelte";
  import Tooltip from "../ui/Tooltip.svelte";
  import Badge from "../ui/Badge.svelte";

  const INPUT_ID = "wp-max-concurrent-jobs-input";

  // One unconditional fetch on mount — same "show a real snapshot
  // immediately" precedent `WorkerPoolWidget.svelte`'s own first `$effect`
  // already establishes for the sibling status widget.
  $effect(() => {
    void batchStore.refreshMaxConcurrentJobsSetting();
  });

  let draft = $state<number | null>(null);

  // Resyncs `draft` from the store's own real `persisted` value whenever it
  // changes (the first fetch, or right after a save this component itself
  // triggered) — but never while the user is actively focused in the field,
  // the same "don't clobber what's mid-edit" guard
  // `CapCutSettingsDialog.svelte`'s own manual-override resync `$effect`
  // already established (re-read before writing this).
  $effect(() => {
    const persisted = batchStore.maxConcurrentJobsSetting?.persisted;
    if (persisted === undefined) return;
    if (typeof document !== "undefined" && document.activeElement?.id === INPUT_ID) return;
    draft = persisted;
  });

  let min = $derived(batchStore.maxConcurrentJobsSetting?.min ?? 1);
  let max = $derived(batchStore.maxConcurrentJobsSetting?.max ?? 16);
  let needsRestart = $derived.by(() => {
    const setting = batchStore.maxConcurrentJobsSetting;
    return setting !== null && setting.persisted !== setting.active;
  });

  async function commit(rawValue: number): Promise<void> {
    if (!Number.isFinite(rawValue)) return;
    const clamped = Math.min(max, Math.max(min, Math.round(rawValue)));
    draft = clamped;
    await batchStore.setMaxConcurrentJobs(clamped);
  }
</script>

<div class="wpc-control">
  <Tooltip text={t("workerPool.maxConcurrentJobsHint")}>
    <label class="wpc-label" for={INPUT_ID}>{t("workerPool.maxConcurrentJobsLabel")}</label>
  </Tooltip>
  <NumberInput
    id={INPUT_ID}
    value={draft ?? min}
    {min}
    {max}
    step={1}
    disabled={batchStore.savingMaxConcurrentJobs}
    ariaLabel={t("workerPool.maxConcurrentJobsLabel")}
    onchange={(v) => void commit(v)}
  />
  {#if needsRestart}
    <Tooltip text={t("workerPool.maxConcurrentJobsHint")}>
      <Badge variant="warn"
        >{t("workerPool.restartToApply", { value: batchStore.maxConcurrentJobsSetting?.persisted ?? 0 })}</Badge
      >
    </Tooltip>
  {/if}
  {#if batchStore.maxConcurrentJobsError}
    <span class="wpc-error">{t("workerPool.saveFailed", { error: batchStore.maxConcurrentJobsError })}</span>
  {/if}
</div>

<style>
  .wpc-control {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  .wpc-label {
    font-size: 11.5px;
    color: var(--muted);
    white-space: nowrap;
  }
  .wpc-control :global(.ui-field) {
    width: 64px;
  }
  .wpc-error {
    font-size: 10px;
    color: var(--neg);
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
