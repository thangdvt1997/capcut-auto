<!--
  One generated short candidate card (Phase 11, master prompt §22), matching
  `HighlightCard.svelte`'s visual language (time range / score / title /
  reason — each `ShortCandidate` carries a real `Highlight`) extended with
  this pipeline's own single real action: "Load into editor" (arm/confirm —
  see `stores/shortsGenerator.svelte.ts`'s class doc comment for why there is
  no separate "Preview" action here).
-->
<script lang="ts">
  import { shortsGenerator } from "../../stores/shortsGenerator.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { ShortCandidate } from "../../types/bindings";

  let { candidate, index }: { candidate: ShortCandidate; index: number } = $props();

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
    if (score >= 80) return "sc-score-high";
    if (score >= 50) return "sc-score-mid";
    return "sc-score-low";
  }

  let highlight = $derived(candidate.highlight);
  let canvas = $derived(candidate.project.canvas);
  let clip = $derived(candidate.project.clips[0] ?? null);
  let durationUs = $derived(clip ? clip.source_out_us - clip.source_in_us : 0);

  let loading = $derived(shortsGenerator.loadingId === highlight.id);
  let loaded = $derived(shortsGenerator.loadedIds.has(highlight.id));
  let pendingConfirm = $derived(shortsGenerator.pendingLoadId === highlight.id);

  function onLoadClick(): void {
    if (pendingConfirm) {
      void shortsGenerator.confirmLoad(candidate);
    } else {
      shortsGenerator.armLoad(candidate);
    }
  }
</script>

<div class="sc-card">
  <div class="sc-header">
    <span class="sc-index">{t("shortsGenerator.cardLabel")} #{index + 1}</span>
    <span class="sc-score {scoreClass(highlight.score)}">{t("highlightDetection.scoreLabel")}: {Math.round(highlight.score)}</span>
  </div>
  <div class="sc-time mono">{formatTimecode(highlight.start_us)} → {formatTimecode(highlight.end_us)}</div>
  <div class="sc-title">{highlight.title}</div>
  <p class="sc-reason muted-2">{highlight.reason}</p>
  <div class="sc-meta muted-2">
    {t("shortsGenerator.candidateMeta", {
      width: canvas.width,
      height: canvas.height,
      seconds: (durationUs / 1_000_000).toFixed(1),
    })}
  </div>

  <div class="sc-actions">
    <button class="btn btn-ghost" disabled={loading} onclick={onLoadClick} class:sc-armed={pendingConfirm}>
      {loading
        ? t("shortsGenerator.loadingIntoEditor")
        : pendingConfirm
          ? t("shortsGenerator.loadConfirmButton")
          : loaded
            ? t("shortsGenerator.loadedIntoEditor")
            : t("shortsGenerator.loadIntoEditorButton")}
    </button>
    {#if pendingConfirm && !loading}
      <button class="btn btn-ghost" onclick={() => shortsGenerator.cancelLoad()}>
        {t("shortsGenerator.loadCancelButton")}
      </button>
    {/if}
  </div>

  {#if pendingConfirm}
    <p class="sc-note muted-2">{t("shortsGenerator.loadConfirmNote")}</p>
  {/if}
</div>

<style>
  .sc-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }
  .sc-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .sc-index {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
  }
  .sc-score {
    font-size: 11px;
    font-weight: 700;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid currentColor;
  }
  .sc-score-high {
    color: var(--pos);
  }
  .sc-score-mid {
    color: var(--warn);
  }
  .sc-score-low {
    color: var(--neg);
  }
  .sc-time {
    font-size: 13px;
    font-weight: 600;
  }
  .sc-title {
    font-size: 12.5px;
    font-weight: 600;
  }
  .sc-reason {
    margin: 0;
    font-size: 11px;
    line-height: 1.4;
  }
  .sc-meta {
    font-size: 10.5px;
  }
  .sc-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .sc-armed {
    color: var(--neg);
    border-color: var(--neg);
  }
  .sc-note {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
</style>
