<!--
  Captions panel (Phase 8 frontend pass, master prompt §26/§27/§28) — mounted
  as `RightPanel.svelte`'s "Captions" tab. Owns generation, the per-caption
  correction list (split/retime/select-for-merge/quick style), and the full
  styling panel (`CaptionStyleEditor.svelte`) and find/replace bar.

  Placement decision (see `stores/captions.svelte.ts`'s class doc comment
  for the full rationale): generation, styling, and correction all live
  together here rather than split across the Transcript Editor / a timeline
  track header, since `generate_captions` operates on the whole project's
  transcript (not one clip's), so it has no natural single-clip home.
  Captions themselves are shown as a plain list here, not as draggable
  blocks on the timeline — the task brief's documented "simpler
  non-timeline panel representation" alternative — to keep this pass's
  surface area (and risk of regressing `Timeline.svelte`/`ClipView.svelte`)
  bounded; retime's "drag boundaries" requirement is instead the numeric
  start/end + scale-words checkbox in `CaptionRow.svelte`, which is the same
  backend `retime_caption` primitive a drag gesture would call.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import CaptionRow from "./CaptionRow.svelte";
  import CaptionStyleEditor from "./CaptionStyleEditor.svelte";

  let sortedCaptions = $derived([...captionsStore.captions].sort((a, b) => a.start_us - b.start_us));
</script>

<div class="panel">
  {#if !timeline.project}
    <p class="empty muted-2">{t("captionsPanel.noProject")}</p>
  {:else}
    <section class="section">
      <h3 class="section-title">{t("captionsPanel.generateSectionTitle")}</h3>
      {#if captionsStore.captionTracks.length > 1}
        <div class="row">
          <label class="label" for="cap-track">{t("captionsPanel.trackLabel")}</label>
          <select id="cap-track" class="select" bind:value={captionsStore.genTrackId}>
            {#each captionsStore.captionTracks as trk (trk.id)}
              <option value={trk.id}>{trk.name}</option>
            {/each}
          </select>
        </div>
      {/if}
      <div class="row">
        <label class="label" for="cap-grouping">{t("captionsPanel.groupingLabel")}</label>
        <select id="cap-grouping" class="select" bind:value={captionsStore.genGrouping}>
          <option value="sentence">{t("captionsPanel.groupingSentence")}</option>
          <option value="word">{t("captionsPanel.groupingWord")}</option>
        </select>
      </div>
      <div class="row">
        <label class="label" for="cap-max-words">{t("captionsPanel.maxWordsLabel")}</label>
        <input id="cap-max-words" class="num" type="number" min="1" bind:value={captionsStore.genMaxWordsPerLine} />
        <label class="label" for="cap-max-chars">{t("captionsPanel.maxCharsLabel")}</label>
        <input id="cap-max-chars" class="num" type="number" min="1" bind:value={captionsStore.genMaxCharsPerLine} />
      </div>
      {#if !captionsStore.hasTranscript}
        <p class="hint muted-2">{t("captionsPanel.noTranscriptHint")}</p>
      {/if}
      <div class="row">
        <button
          class="btn"
          disabled={captionsStore.generating || !captionsStore.hasTranscript || !captionsStore.effectiveGenTrackId}
          onclick={() => void captionsStore.generate()}
        >
          {captionsStore.generating ? t("captionsPanel.generating") : t("captionsPanel.generateButton")}
        </button>
      </div>
      {#if captionsStore.generateError}
        <div class="error">{captionsStore.generateError}</div>
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">
        {t("captionsPanel.captionsSectionTitle", { count: sortedCaptions.length })}
      </h3>
      {#if sortedCaptions.length === 0}
        <p class="hint muted-2">{t("captionsPanel.captionsEmpty")}</p>
      {:else}
        <div class="list">
          {#each sortedCaptions as caption (caption.id)}
            <CaptionRow {caption} />
          {/each}
        </div>
        <div class="row multi-bar">
          <span class="hint muted-2">
            {t("captionsPanel.selectedForMerge", { count: captionsStore.selectedCaptionIds.size })}
          </span>
          <button class="btn btn-ghost" disabled={captionsStore.selectedCaptionIds.size === 0} onclick={() => captionsStore.clearCaptionSelection()}>
            {t("captionsPanel.clearSelectionButton")}
          </button>
          <button class="btn btn-ghost" disabled={captionsStore.selectedCaptionIds.size < 2} onclick={() => void captionsStore.mergeSelected()}>
            {t("captionsPanel.mergeButton")}
          </button>
        </div>
        {#if captionsStore.correctionError}
          <div class="error">{captionsStore.correctionError}</div>
        {/if}
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">{t("captionsPanel.findReplaceSectionTitle")}</h3>
      <div class="row">
        <input class="text-input" type="text" placeholder={t("captionsPanel.findPlaceholder")} bind:value={captionsStore.findText} />
        <input class="text-input" type="text" placeholder={t("captionsPanel.replacePlaceholder")} bind:value={captionsStore.replaceText} />
      </div>
      <div class="row">
        <label class="check"><input type="checkbox" bind:checked={captionsStore.caseSensitive} /> {t("captionsPanel.caseSensitiveLabel")}</label>
        <label class="check"><input type="checkbox" bind:checked={captionsStore.wholeWord} /> {t("captionsPanel.wholeWordLabel")}</label>
      </div>
      <div class="row">
        <span class="hint muted-2">{t("captionsPanel.matchCount", { count: captionsStore.matchCount })}</span>
        <button
          class="btn"
          disabled={captionsStore.findText === "" || captionsStore.matchCount === 0 || captionsStore.findReplaceBusy}
          onclick={() => void captionsStore.replaceAll()}
        >
          {captionsStore.findReplaceBusy ? t("captionsPanel.replacing") : t("captionsPanel.replaceAllButton")}
        </button>
      </div>
      {#if captionsStore.findReplaceError}
        <div class="error">{captionsStore.findReplaceError}</div>
      {/if}
    </section>

    <section class="section">
      <h3 class="section-title">{t("captionsPanel.styleSectionTitle")}</h3>
      <CaptionStyleEditor />
    </section>
  {/if}
</div>

<style>
  .panel {
    height: 100%;
    overflow-y: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .empty {
    margin: 0;
    font-size: 11.5px;
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
  }
  .section:last-child {
    border-bottom: none;
  }
  .section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .multi-bar {
    padding-top: 6px;
  }
  .label {
    font-size: 10.5px;
    color: var(--muted);
  }
  .select {
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
  }
  .num {
    width: 64px;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
    padding: 0 6px;
  }
  .text-input {
    flex: 1;
    min-width: 90px;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
    padding: 0 6px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
  }
  .hint {
    margin: 0;
    font-size: 10.5px;
  }
  .list {
    display: flex;
    flex-direction: column;
    max-height: 320px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
</style>
