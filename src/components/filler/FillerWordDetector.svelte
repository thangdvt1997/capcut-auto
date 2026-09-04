<!--
  Filler Word Detector dialog (master prompt §16). Same dialog shape as
  `components/silence/SilenceDetector.svelte` (see that file's own doc
  comment) — Detect -> candidates-first preview -> Select all/Deselect ->
  Apply -> Reset — with the one real difference the task brief calls out:
  candidates are individually checkable (`fillerWordDetector.checked`) since
  filler-word matches are named/textual, unlike generic silence regions.

  Opened from `Timeline.svelte`'s toolbar, right next to the Silence
  Detector button (same placement rationale — the user already has a clip in
  context there).
-->
<script lang="ts">
  import { fillerWordDetector, DEFAULT_EN_FILLERS, DEFAULT_VI_FILLERS } from "../../stores/fillerWordDetector.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import FillerCandidateStrip from "./FillerCandidateStrip.svelte";
  import type { Clip } from "../../types/bindings";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function clipLabel(clip: Clip): string {
    const media = clip.media_id ? timeline.mediaById.get(clip.media_id) : undefined;
    const name = media ? basename(media.source_path) : t("timelinePanel.clipEmptyLabel");
    return `${name} (${(clip.position_us / 1_000_000).toFixed(1)}s)`;
  }

  function formatSec(us: number): string {
    return `${(us / 1_000_000).toFixed(2)}s`;
  }

  let mediaDurationUs = $derived(fillerWordDetector.selectedMedia?.duration_us ?? 0);
  let checkedCount = $derived(fillerWordDetector.checkedCuts.length);

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      fillerWordDetector.close();
    }
  }
</script>

{#if fillerWordDetector.open}
  <div class="fd-backdrop" role="presentation" onclick={() => fillerWordDetector.close()}>
    <div
      class="fd-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("fillerWordDetector.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="fd-header">
        <span class="fd-title">{t("fillerWordDetector.title")}</span>
        <button class="btn btn-ghost" onclick={() => fillerWordDetector.close()} title={t("fillerWordDetector.close")}>×</button>
      </div>

      <p class="fd-explainer muted-2">{t("fillerWordDetector.explainer")}</p>

      <div class="fd-body">
        <section class="fd-section">
          <h3 class="fd-section-title">{t("fillerWordDetector.sourceSectionTitle")}</h3>
          <div class="fd-row">
            <label class="fd-label" for="fd-track">{t("silenceDetector.trackLabel")}</label>
            <select
              id="fd-track"
              class="fd-select"
              value={fillerWordDetector.trackId ?? ""}
              onchange={(e) => fillerWordDetector.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if fillerWordDetector.eligibleTracks.length === 0}
                <option value="" disabled>{t("silenceDetector.noTracks")}</option>
              {/if}
              {#each fillerWordDetector.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="fd-row">
            <label class="fd-label" for="fd-clip">{t("silenceDetector.clipLabel")}</label>
            <select
              id="fd-clip"
              class="fd-select"
              value={fillerWordDetector.clipId ?? ""}
              disabled={fillerWordDetector.clipsForSelectedTrack.length === 0}
              onchange={(e) => fillerWordDetector.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if fillerWordDetector.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("silenceDetector.noClips")}</option>
              {/if}
              {#each fillerWordDetector.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          <div class="fd-row">
            <span class="fd-label">{t("silenceDetector.applyModeLabel")}</span>
            <div class="fd-radio-group">
              <label class="fd-radio">
                <input type="radio" name="fd-apply-mode" value="clip" checked={fillerWordDetector.applyMode === "clip"} onchange={() => (fillerWordDetector.applyMode = "clip")} />
                {t("silenceDetector.applyModeClip")}
              </label>
              <label class="fd-radio">
                <input type="radio" name="fd-apply-mode" value="track" checked={fillerWordDetector.applyMode === "track"} onchange={() => (fillerWordDetector.applyMode = "track")} />
                {t("silenceDetector.applyModeTrack")}
              </label>
            </div>
          </div>
          {#if fillerWordDetector.selectedMedia && fillerWordDetector.transcriptEntries.length === 0}
            <p class="fd-hint muted-2">{t("fillerWordDetector.noTranscriptHint")}</p>
          {/if}
        </section>

        <section class="fd-section">
          <h3 class="fd-section-title">{t("fillerWordDetector.dictionarySectionTitle")}</h3>
          <label class="fd-checkbox-row">
            <input type="checkbox" bind:checked={fillerWordDetector.useDefaults} />
            {t("fillerWordDetector.useDefaultsLabel")}
          </label>
          <p class="fd-hint muted-2">
            {t("fillerWordDetector.defaultsHint", {
              en: DEFAULT_EN_FILLERS.join(", "),
              vi: DEFAULT_VI_FILLERS.join(", "),
            })}
          </p>
          <label class="fd-label" for="fd-custom-dict">{t("fillerWordDetector.customDictionaryLabel")}</label>
          <textarea
            id="fd-custom-dict"
            class="fd-textarea"
            rows="2"
            placeholder={t("fillerWordDetector.customDictionaryPlaceholder")}
            bind:value={fillerWordDetector.customDictionaryText}
          ></textarea>
        </section>

        <section class="fd-section">
          <h3 class="fd-section-title">{t("fillerWordDetector.paddingSectionTitle")}</h3>
          <div class="fd-slider-row">
            <label class="fd-label" for="fd-pad-before">{t("silenceDetector.paddingBeforeLabel")}</label>
            <input id="fd-pad-before" type="range" min="0" max="1000" step="10" bind:value={fillerWordDetector.paddingBeforeMs} />
            <span class="fd-value mono">{fillerWordDetector.paddingBeforeMs} ms</span>
          </div>
          <div class="fd-slider-row">
            <label class="fd-label" for="fd-pad-after">{t("silenceDetector.paddingAfterLabel")}</label>
            <input id="fd-pad-after" type="range" min="0" max="1000" step="10" bind:value={fillerWordDetector.paddingAfterMs} />
            <span class="fd-value mono">{fillerWordDetector.paddingAfterMs} ms</span>
          </div>
          <div class="fd-slider-row">
            <label class="fd-label" for="fd-merge-gap">{t("silenceDetector.mergeGapLabel")}</label>
            <input id="fd-merge-gap" type="range" min="0" max="3000" step="10" bind:value={fillerWordDetector.mergeGapMs} />
            <span class="fd-value mono">{fillerWordDetector.mergeGapMs} ms</span>
          </div>
        </section>

        <section class="fd-section">
          <h3 class="fd-section-title">{t("fillerWordDetector.candidatesSectionTitle")}</h3>
          {#if fillerWordDetector.candidates.length === 0}
            <p class="fd-empty muted-2">{t("fillerWordDetector.candidatesEmpty")}</p>
          {:else}
            <div class="fd-candidates-toolbar">
              <button class="btn btn-ghost" onclick={() => fillerWordDetector.selectAll()}>{t("fillerWordDetector.selectAllButton")}</button>
              <button class="btn btn-ghost" onclick={() => fillerWordDetector.deselectAll()}>{t("fillerWordDetector.deselectButton")}</button>
              <span class="fd-count muted-2">{t("fillerWordDetector.checkedCount", { checked: checkedCount, total: fillerWordDetector.candidates.length })}</span>
            </div>

            <FillerCandidateStrip durationUs={mediaDurationUs} candidates={fillerWordDetector.candidates} checked={fillerWordDetector.checked} />

            <ul class="fd-candidate-list">
              {#each fillerWordDetector.candidates as cut (cut.id)}
                {@const entry = fillerWordDetector.entryForCut(cut)}
                <li class="fd-candidate-row">
                  <label class="fd-checkbox-row">
                    <input
                      type="checkbox"
                      checked={fillerWordDetector.checked[cut.id] ?? false}
                      onchange={() => fillerWordDetector.toggleCandidate(cut.id)}
                    />
                    <span class="fd-candidate-time mono">{formatSec(cut.start_us)} – {formatSec(cut.end_us)}</span>
                    <span class="fd-candidate-text">{entry ? entry.text : t("fillerWordDetector.candidateNoText")}</span>
                  </label>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        {#if fillerWordDetector.lastError}
          <div class="fd-error">{fillerWordDetector.lastError}</div>
        {/if}
      </div>

      <div class="fd-footer">
        <button class="btn" disabled={!fillerWordDetector.canDetect} onclick={() => void fillerWordDetector.detect()}>
          {fillerWordDetector.detecting ? t("fillerWordDetector.detecting") : t("fillerWordDetector.detectButton")}
        </button>
        <button class="btn" disabled={!fillerWordDetector.canApply} onclick={() => void fillerWordDetector.applyCuts()}>
          {fillerWordDetector.applying ? t("silenceDetector.applying") : t("fillerWordDetector.applyButton")}
        </button>
        <button class="btn btn-ghost" onclick={() => void fillerWordDetector.reset()}>{t("silenceDetector.resetButton")}</button>
        <span class="fd-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => fillerWordDetector.close()}>{t("silenceDetector.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .fd-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .fd-dialog {
    width: min(760px, 94vw);
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
  .fd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .fd-title {
    font-size: 13px;
    font-weight: 600;
  }
  .fd-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .fd-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .fd-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .fd-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .fd-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .fd-slider-row {
    display: grid;
    grid-template-columns: 130px 1fr 64px;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .fd-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .fd-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .fd-textarea {
    width: 100%;
    resize: vertical;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
    padding: 6px;
  }
  .fd-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .fd-radio-group {
    display: flex;
    gap: 14px;
  }
  .fd-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .fd-checkbox-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .fd-value {
    font-size: 11px;
    text-align: right;
    color: var(--muted);
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .fd-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .fd-candidates-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .fd-count {
    font-size: 10.5px;
    margin-left: auto;
  }
  .fd-candidate-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 180px;
    overflow-y: auto;
  }
  .fd-candidate-row {
    border-radius: var(--radius-sm);
  }
  .fd-candidate-row:hover {
    background: var(--surface-2);
  }
  .fd-candidate-time {
    color: var(--muted);
    flex-shrink: 0;
  }
  .fd-candidate-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fd-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .fd-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .fd-footer-spacer {
    flex: 1;
  }
</style>
