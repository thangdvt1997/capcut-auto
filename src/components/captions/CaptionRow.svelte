<!--
  One caption entry in `CaptionsPanel.svelte`'s list (master prompt §28's
  correction tools — split/retime/bulk-style/select-for-merge — plus
  `stores/captions.svelte.ts`'s per-row quick style apply).

  Retime/scale-words is exposed here as plain numeric start/end (seconds)
  inputs + a checkbox, per the task brief's documented "simpler
  non-timeline panel representation" alternative to a timeline drag handle
  — see `CaptionsPanel.svelte`'s own doc comment for the full placement
  rationale (captions are shown in a dedicated panel, not as draggable
  timeline blocks, in this pass).
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import type { Caption } from "../../types/bindings";
  import { formatTimecode, secToUs, usToSec } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";

  let { caption }: { caption: Caption } = $props();

  // Seeded to 0 here (not from `caption.start_us`/`end_us` directly — that
  // would only capture Svelte 5's initial prop value, `state_referenced_locally`)
  // and populated for real by the `$effect` below, which also fires once on
  // mount.
  let startSecBuffer = $state(0);
  let endSecBuffer = $state(0);
  let scaleWords = $state(true);

  // Resyncs the local edit buffers whenever the backend's own start/end for
  // this caption changes (a successful retime, an undo/redo, a split/merge
  // elsewhere) — never on the buffers' own edits, since those are local
  // `$state`, not reads of `caption` itself.
  $effect(() => {
    startSecBuffer = usToSec(caption.start_us);
    endSecBuffer = usToSec(caption.end_us);
  });

  let selected = $derived(captionsStore.selectedCaptionIds.has(caption.id));
  let busy = $derived(captionsStore.busyCaptionId === caption.id);
  // Real "current subtitle" signal (promt.md §3.1) — `activeCaptionId` was
  // already computed off the real playhead (`findActiveCaption`, karaoke
  // overlay's own data source), just never consumed for a visual highlight
  // anywhere until this pass. `playheadInside` below is a distinct, narrower
  // check (exact caption span, used to gate the Split action) that happens
  // to compute the same boundary condition `findActiveCaption` uses — see
  // that function's own doc comment for why they're not literally the same
  // derivation (hidden-track handling).
  let isActive = $derived(captionsStore.activeCaptionId === caption.id);
  let playheadInside = $derived(timeline.playheadUs >= caption.start_us && timeline.playheadUs < caption.end_us);
  let canSplit = $derived(playheadInside && caption.words.length > 0 && !busy);
  let retimeDirty = $derived(startSecBuffer !== usToSec(caption.start_us) || endSecBuffer !== usToSec(caption.end_us));
  let retimeValid = $derived(endSecBuffer > startSecBuffer);

  function commitRetime(): void {
    if (!retimeDirty || !retimeValid || busy) return;
    void captionsStore.retime(caption, secToUs(startSecBuffer), secToUs(endSecBuffer), scaleWords);
  }
</script>

<div class="row" class:selected class:active={isActive}>
  <input
    type="checkbox"
    checked={selected}
    onchange={() => captionsStore.toggleCaptionSelected(caption.id, true)}
    title={t("captionsPanel.selectForMerge")}
  />

  <div class="main">
    <div class="time-line">
      <!--
        Jump-to-caption (promt.md §3.1's "Jump đến subtitle khi click subtitle
        row") — the timecode is the natural non-interactive click target in
        this row (the split button/style select right next to it already own
        clicks for their own actions). `title` doubles as the affordance hint
        since this plain `<span>` has no other visual "this is clickable" cue.
      -->
      <button
        type="button"
        class="time-jump mono"
        onclick={() => captionsStore.seekToCaption(caption)}
        title={t("captionsPanel.jumpToCaption")}
      >{formatTimecode(caption.start_us)} – {formatTimecode(caption.end_us)}</button>
      <button class="btn btn-ghost btn-xs" disabled={!canSplit} onclick={() => void captionsStore.splitAtPlayhead(caption)} title={t("captionsPanel.splitAtPlayhead")}>
        {t("captionsPanel.splitButton")}
      </button>
      <select
        class="style-select"
        value={caption.style_id ?? ""}
        onchange={(e) => void captionsStore.setCaptionStyle(caption.id, (e.currentTarget.value || null))}
      >
        <option value="">{t("captionsPanel.styleNone")}</option>
        {#each captionsStore.catalog as s (s.id)}
          <option value={s.id}>{s.name}</option>
        {/each}
      </select>
    </div>

    <p class="text" title={caption.text}>{caption.text}</p>

    <div class="retime-line">
      <label class="retime-field">
        <span class="muted-2">{t("captionsPanel.retimeStart")}</span>
        <input class="num" type="number" step="0.01" min="0" bind:value={startSecBuffer} onblur={commitRetime} />
      </label>
      <label class="retime-field">
        <span class="muted-2">{t("captionsPanel.retimeEnd")}</span>
        <input class="num" type="number" step="0.01" min="0" bind:value={endSecBuffer} onblur={commitRetime} />
      </label>
      <label class="retime-scale">
        <input type="checkbox" bind:checked={scaleWords} />
        {t("captionsPanel.retimeScaleWords")}
      </label>
      {#if retimeDirty}
        <button class="btn btn-ghost btn-xs" disabled={!retimeValid || busy} onclick={commitRetime}>
          {t("captionsPanel.retimeApply")}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .row {
    display: flex;
    gap: 8px;
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
  }
  .row.selected {
    background: hsl(213 94% 68% / 0.08);
  }
  /* Real "current subtitle" highlight (promt.md §3.1) — `isActive` above is
     computed from the real playhead via `captionsStore.activeCaptionId`, not
     a fabricated/timed animation. Left border rather than a competing
     background so it stays legible alongside `.selected`'s own background
     tint when a caption is both the active one and multi-selected. */
  .row.active {
    border-left: 2px solid var(--accent);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .time-line {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .time-jump {
    font-size: 10.5px;
    color: var(--muted);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
  }
  .time-jump:hover {
    color: var(--foreground);
    text-decoration: underline;
  }
  .style-select {
    margin-left: auto;
    height: 22px;
    max-width: 120px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 10.5px;
  }
  .text {
    margin: 0;
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .retime-line {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .retime-field {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
  }
  .retime-scale {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
  }
  .num {
    width: 64px;
    height: 20px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 10.5px;
    padding: 0 4px;
  }
  .btn-xs {
    height: 20px;
    padding: 0 6px;
    font-size: 10.5px;
  }
</style>
