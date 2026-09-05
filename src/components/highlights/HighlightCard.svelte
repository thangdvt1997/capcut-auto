<!--
  One highlight card, matching master prompt §21's own UI mock almost
  verbatim:

    Highlight #1
    00:03:14 → 00:03:52
    Score: 92

  plus title/reason and the four listed actions (Preview / Add to timeline /
  Create new project / Export clip) — see `stores/highlightDetection
  .svelte.ts`'s class doc comment for exactly how each of those four is
  wired (Preview/Add to timeline for real through existing mechanisms;
  Create new project/Export clip also for real, but through this pass's own
  documented client-side bridges, since neither has a dedicated backend
  command).
-->
<script lang="ts">
  import { highlightDetection } from "../../stores/highlightDetection.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { Highlight } from "../../types/bindings";

  let { highlight, index }: { highlight: Highlight; index: number } = $props();

  function formatTimecode(us: number): string {
    const totalSeconds = Math.floor(us / 1_000_000);
    const h = Math.floor(totalSeconds / 3600);
    const m = Math.floor((totalSeconds % 3600) / 60);
    const s = totalSeconds % 60;
    const mm = String(m).padStart(2, "0");
    const ss = String(s).padStart(2, "0");
    return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
  }

  function scoreClass(score: number): string {
    if (score >= 80) return "hc-score-high";
    if (score >= 50) return "hc-score-mid";
    return "hc-score-low";
  }

  let adding = $derived(highlightDetection.addingId === highlight.id);
  let added = $derived(highlightDetection.addedIds.has(highlight.id));
  let creating = $derived(highlightDetection.creatingProjectId === highlight.id);
  let pendingConfirm = $derived(highlightDetection.pendingCreateProjectId === highlight.id);
  let exporting = $derived(highlightDetection.exportingId === highlight.id);
  let exportProgress = $derived(highlightDetection.exportProgressFor(highlight.id));
  let exportedPath = $derived(highlightDetection.exportedPathByHighlight[highlight.id] ?? null);

  function onCreateProjectClick(): void {
    if (pendingConfirm) {
      void highlightDetection.confirmCreateProject(highlight);
    } else {
      highlightDetection.armCreateProject(highlight);
    }
  }
</script>

<div class="hc-card">
  <div class="hc-header">
    <span class="hc-index">{t("highlightDetection.cardLabel")} #{index + 1}</span>
    <span class="hc-score {scoreClass(highlight.score)}">{t("highlightDetection.scoreLabel")}: {Math.round(highlight.score)}</span>
  </div>
  <div class="hc-time mono">{formatTimecode(highlight.start_us)} → {formatTimecode(highlight.end_us)}</div>
  <div class="hc-title">{highlight.title}</div>
  <p class="hc-reason muted-2">{highlight.reason}</p>

  <div class="hc-actions">
    <button class="btn btn-ghost" onclick={() => highlightDetection.preview(highlight)}>
      {t("highlightDetection.previewButton")}
    </button>
    <button class="btn btn-ghost" disabled={adding} onclick={() => void highlightDetection.addToTimeline(highlight)}>
      {adding ? t("highlightDetection.addingToTimeline") : added ? t("highlightDetection.addedToTimeline") : t("highlightDetection.addToTimelineButton")}
    </button>
    <button class="btn btn-ghost" disabled={creating} onclick={onCreateProjectClick} class:hc-armed={pendingConfirm}>
      {creating ? t("highlightDetection.creatingProject") : pendingConfirm ? t("highlightDetection.createProjectConfirmButton") : t("highlightDetection.createProjectButton")}
    </button>
    {#if pendingConfirm && !creating}
      <button class="btn btn-ghost" onclick={() => highlightDetection.cancelCreateProject()}>
        {t("highlightDetection.createProjectCancelButton")}
      </button>
    {/if}
    <button class="btn btn-ghost" disabled={exporting} onclick={() => void highlightDetection.exportClip(highlight)}>
      {exporting ? t("highlightDetection.exportingClip") : t("highlightDetection.exportClipButton")}
    </button>
  </div>

  {#if pendingConfirm}
    <p class="hc-note muted-2">{t("highlightDetection.createProjectNote")}</p>
  {/if}
  {#if exportProgress && !exportProgress.done}
    <p class="hc-note muted-2">
      {t("highlightDetection.exportProgress", { percent: Math.round((exportProgress.fraction ?? 0) * 100) })}
    </p>
  {/if}
  {#if exportProgress?.done && !exportProgress.error && exportedPath}
    <p class="hc-note hc-note-ok">{t("highlightDetection.exportClipDone", { path: exportedPath })}</p>
  {/if}
  {#if exportProgress?.error}
    <p class="hc-note hc-note-error">{t("highlightDetection.exportClipError", { error: exportProgress.error })}</p>
  {/if}
</div>

<style>
  .hc-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .hc-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .hc-index {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .hc-score {
    font-size: 11px;
    font-weight: 700;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid currentColor;
  }
  .hc-score-high {
    color: var(--pos);
  }
  .hc-score-mid {
    color: var(--warn);
  }
  .hc-score-low {
    color: var(--neg);
  }
  .hc-time {
    font-size: 13px;
    font-weight: 600;
  }
  .hc-title {
    font-size: 12.5px;
    font-weight: 600;
  }
  .hc-reason {
    margin: 0;
    font-size: 11px;
    line-height: 1.4;
  }
  .hc-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .hc-armed {
    color: var(--neg);
    border-color: var(--neg);
  }
  .hc-note {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .hc-note-ok {
    color: var(--pos);
  }
  .hc-note-error {
    color: var(--neg);
  }
</style>
