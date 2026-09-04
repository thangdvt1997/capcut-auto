<!--
  Silence Detector dialog (master prompt §12/§13). Opened from
  `Timeline.svelte`'s toolbar (the "detect silence on whatever's already on
  the timeline" entry point) and from `MediaLibrary.svelte`'s per-item action
  (which first places that media on the timeline via the existing
  `timeline.addMediaAsClip` bridge, then opens straight to it) — see each
  caller's comment for why both exist. Placement decision recorded here and
  in `IMPLEMENTATION_PLAN.md`'s Phase 5 notes.

  Pure UI over `stores/silenceDetector.svelte.ts`: every slider is bound
  directly to that store's ms/percent state (never raw microseconds), and
  every workflow button (Analyze/Preview Cuts/Apply Cuts/Reset) just calls
  the store's matching method.
-->
<script lang="ts">
  import { silenceDetector } from "../../stores/silenceDetector.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import SpeechRegionStrip from "./SpeechRegionStrip.svelte";
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

  let removedTotalUs = $derived(silenceDetector.cuts.reduce((sum, c) => sum + (c.end_us - c.start_us), 0));
  let mediaDurationUs = $derived(silenceDetector.selectedMedia?.duration_us ?? 0);

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      silenceDetector.close();
    }
  }
</script>

{#if silenceDetector.open}
  <div class="sd-backdrop" role="presentation" onclick={() => silenceDetector.close()}>
    <div
      class="sd-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("silenceDetector.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="sd-header">
        <span class="sd-title">{t("silenceDetector.title")}</span>
        <button class="btn btn-ghost" onclick={() => silenceDetector.close()} title={t("silenceDetector.close")}>×</button>
      </div>

      <p class="sd-explainer muted-2">{t("silenceDetector.explainer")}</p>

      <div class="sd-body">
        <section class="sd-section">
          <h3 class="sd-section-title">{t("silenceDetector.sourceSectionTitle")}</h3>
          <div class="sd-row">
            <label class="sd-label" for="sd-track">{t("silenceDetector.trackLabel")}</label>
            <select
              id="sd-track"
              class="sd-select"
              value={silenceDetector.trackId ?? ""}
              onchange={(e) => silenceDetector.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if silenceDetector.eligibleTracks.length === 0}
                <option value="" disabled>{t("silenceDetector.noTracks")}</option>
              {/if}
              {#each silenceDetector.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="sd-row">
            <label class="sd-label" for="sd-clip">{t("silenceDetector.clipLabel")}</label>
            <select
              id="sd-clip"
              class="sd-select"
              value={silenceDetector.clipId ?? ""}
              disabled={silenceDetector.clipsForSelectedTrack.length === 0}
              onchange={(e) => silenceDetector.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if silenceDetector.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("silenceDetector.noClips")}</option>
              {/if}
              {#each silenceDetector.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          <div class="sd-row">
            <span class="sd-label">{t("silenceDetector.applyModeLabel")}</span>
            <div class="sd-radio-group">
              <label class="sd-radio">
                <input type="radio" name="sd-apply-mode" value="clip" checked={silenceDetector.applyMode === "clip"} onchange={() => (silenceDetector.applyMode = "clip")} />
                {t("silenceDetector.applyModeClip")}
              </label>
              <label class="sd-radio">
                <input type="radio" name="sd-apply-mode" value="track" checked={silenceDetector.applyMode === "track"} onchange={() => (silenceDetector.applyMode = "track")} />
                {t("silenceDetector.applyModeTrack")}
              </label>
            </div>
          </div>
          <div class="sd-row">
            <label class="sd-label" for="sd-channel">{t("silenceDetector.channelLabel")}</label>
            <select id="sd-channel" class="sd-select" disabled title={t("silenceDetector.channelUnsupportedTooltip")}>
              <option>{t("silenceDetector.channelMonoOnly")}</option>
            </select>
            <span class="sd-hint muted-2">{t("silenceDetector.channelUnsupportedHint")}</span>
          </div>
        </section>

        <section class="sd-section">
          <h3 class="sd-section-title">{t("silenceDetector.paramsSectionTitle")}</h3>

          <div class="sd-slider-row">
            <label class="sd-label" for="sd-threshold">{t("silenceDetector.thresholdLabel")}</label>
            <input id="sd-threshold" type="range" min="1" max="99" step="1" bind:value={silenceDetector.thresholdPct} />
            <span class="sd-value mono">{silenceDetector.thresholdPct}%</span>
          </div>

          <div class="sd-slider-row">
            <label class="sd-label" for="sd-min-silence">{t("silenceDetector.minSilenceLabel")}</label>
            <input id="sd-min-silence" type="range" min="0" max="3000" step="10" bind:value={silenceDetector.minSilenceMs} />
            <span class="sd-value mono">{silenceDetector.minSilenceMs} ms</span>
          </div>

          <div class="sd-slider-row">
            <label class="sd-label" for="sd-min-speech">{t("silenceDetector.minSpeechLabel")}</label>
            <input id="sd-min-speech" type="range" min="0" max="3000" step="10" bind:value={silenceDetector.minSpeechMs} />
            <span class="sd-value mono">{silenceDetector.minSpeechMs} ms</span>
          </div>

          <div class="sd-slider-row">
            <label class="sd-label" for="sd-pad-before">{t("silenceDetector.paddingBeforeLabel")}</label>
            <input id="sd-pad-before" type="range" min="0" max="1000" step="10" bind:value={silenceDetector.paddingBeforeMs} />
            <span class="sd-value mono">{silenceDetector.paddingBeforeMs} ms</span>
          </div>

          <div class="sd-slider-row">
            <label class="sd-label" for="sd-pad-after">{t("silenceDetector.paddingAfterLabel")}</label>
            <input id="sd-pad-after" type="range" min="0" max="1000" step="10" bind:value={silenceDetector.paddingAfterMs} />
            <span class="sd-value mono">{silenceDetector.paddingAfterMs} ms</span>
          </div>

          <div class="sd-slider-row">
            <label class="sd-label" for="sd-merge-gap">{t("silenceDetector.mergeGapLabel")}</label>
            <input id="sd-merge-gap" type="range" min="0" max="3000" step="10" bind:value={silenceDetector.mergeGapMs} />
            <span class="sd-value mono">{silenceDetector.mergeGapMs} ms</span>
          </div>
        </section>

        <section class="sd-section">
          <h3 class="sd-section-title">{t("silenceDetector.regionsSectionTitle")}</h3>
          {#if !silenceDetector.scoreSummary}
            <p class="sd-empty muted-2">{t("silenceDetector.regionsEmpty")}</p>
          {:else}
            <SpeechRegionStrip durationUs={mediaDurationUs} segments={silenceDetector.segments} cuts={silenceDetector.cuts} />
            <p class="sd-summary muted-2">
              {t("silenceDetector.summaryLine", {
                segments: silenceDetector.segments.length,
                removed: formatSec(removedTotalUs),
                total: formatSec(mediaDurationUs),
              })}
            </p>
          {/if}
        </section>

        {#if silenceDetector.lastError}
          <div class="sd-error">{silenceDetector.lastError}</div>
        {/if}
      </div>

      <div class="sd-footer">
        <button
          class="btn"
          disabled={!silenceDetector.canAnalyze}
          onclick={() => void silenceDetector.analyze()}
        >
          {silenceDetector.analyzing ? t("silenceDetector.analyzing") : t("silenceDetector.analyzeButton")}
        </button>
        <button
          class="btn"
          disabled={!silenceDetector.canPreview}
          onclick={() => void silenceDetector.previewCuts()}
        >
          {silenceDetector.previewLoading ? t("silenceDetector.previewing") : t("silenceDetector.previewButton")}
        </button>
        <button
          class="btn"
          disabled={!silenceDetector.canApply}
          onclick={() => void silenceDetector.applyCuts()}
        >
          {silenceDetector.applying ? t("silenceDetector.applying") : t("silenceDetector.applyButton")}
        </button>
        <button class="btn btn-ghost" onclick={() => void silenceDetector.reset()}>
          {t("silenceDetector.resetButton")}
        </button>
        <span class="sd-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => silenceDetector.close()}>{t("silenceDetector.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .sd-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .sd-dialog {
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
  .sd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sd-title {
    font-size: 13px;
    font-weight: 600;
  }
  .sd-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .sd-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .sd-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .sd-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sd-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .sd-slider-row {
    display: grid;
    grid-template-columns: 130px 1fr 64px;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .sd-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .sd-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .sd-select:disabled { opacity: 0.6; cursor: not-allowed; }
  .sd-hint {
    font-size: 10.5px;
  }
  .sd-radio-group {
    display: flex;
    gap: 14px;
  }
  .sd-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .sd-value {
    font-size: 11px;
    text-align: right;
    color: var(--muted);
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .sd-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .sd-summary {
    margin: 0;
    font-size: 11px;
  }
  .sd-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .sd-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sd-footer-spacer {
    flex: 1;
  }
</style>
