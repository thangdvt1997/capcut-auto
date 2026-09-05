<!--
  Highlight Detection dialog (Phase 10 follow-up, master prompt §21,
  `src-tauri/src/highlights/`). Same source track/clip picker shape as
  `SmartEditDialog.svelte`/`FillerWordDetector.svelte` — the panel always
  analyzes "the currently selected timeline clip's underlying media".
  Results render as a `HighlightCard.svelte` list (master prompt §21's own
  UI mock), with an honest "AI-enhanced" vs "local signals only" badge
  driven by `HighlightDetectionResult.used_ai_semantic_signal` — never
  implied AI involvement when the backend actually fell back to local
  signals only (no AI configured, or `useAi` toggled off here).

  Opened from `Timeline.svelte`'s toolbar, matching Smart Edit/Silence
  Detector/Filler Words/AI Command's placement.
-->
<script lang="ts">
  import { highlightDetection } from "../../stores/highlightDetection.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import HighlightCard from "./HighlightCard.svelte";
  import type { Clip } from "../../types/bindings";

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function clipLabel(clip: Clip): string {
    const media = clip.media_id ? timeline.mediaById.get(clip.media_id) : undefined;
    const name = media ? basename(media.source_path) : t("timelinePanel.clipEmptyLabel");
    return `${name} (${(clip.position_us / 1_000_000).toFixed(1)}s)`;
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      highlightDetection.close();
    }
  }
</script>

{#if highlightDetection.open}
  <div class="hd-backdrop" role="presentation" onclick={() => highlightDetection.close()}>
    <div
      class="hd-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("highlightDetection.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="hd-header">
        <span class="hd-title">{t("highlightDetection.title")}</span>
        <button class="btn btn-ghost" onclick={() => highlightDetection.close()} title={t("highlightDetection.close")}>×</button>
      </div>

      <p class="hd-explainer muted-2">{t("highlightDetection.explainer")}</p>

      <div class="hd-body">
        <section class="hd-section">
          <h3 class="hd-section-title">{t("highlightDetection.sourceSectionTitle")}</h3>
          <div class="hd-row">
            <label class="hd-label" for="hd-track">{t("silenceDetector.trackLabel")}</label>
            <select
              id="hd-track"
              class="hd-select"
              value={highlightDetection.trackId ?? ""}
              onchange={(e) => highlightDetection.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if highlightDetection.eligibleTracks.length === 0}
                <option value="" disabled>{t("silenceDetector.noTracks")}</option>
              {/if}
              {#each highlightDetection.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="hd-row">
            <label class="hd-label" for="hd-clip">{t("silenceDetector.clipLabel")}</label>
            <select
              id="hd-clip"
              class="hd-select"
              value={highlightDetection.clipId ?? ""}
              disabled={highlightDetection.clipsForSelectedTrack.length === 0}
              onchange={(e) => highlightDetection.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if highlightDetection.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("silenceDetector.noClips")}</option>
              {/if}
              {#each highlightDetection.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          {#if highlightDetection.selectedMedia && highlightDetection.transcriptEntries.length === 0}
            <p class="hd-hint muted-2">{t("highlightDetection.noTranscriptNote")}</p>
          {/if}
        </section>

        <section class="hd-section">
          <h3 class="hd-section-title">{t("highlightDetection.settingsSectionTitle")}</h3>
          <div class="hd-row">
            <label class="hd-label" for="hd-max">{t("highlightDetection.maxHighlightsLabel")}</label>
            <input
              id="hd-max"
              type="number"
              min="1"
              max="20"
              class="hd-number"
              value={highlightDetection.maxHighlights}
              onchange={(e) => (highlightDetection.maxHighlights = Math.max(1, Number((e.target as HTMLInputElement).value) || 1))}
            />
          </div>
          <label class="hd-checkbox-row">
            <input type="checkbox" bind:checked={highlightDetection.useAi} />
            {t("highlightDetection.useAiLabel")}
          </label>
          {#if highlightDetection.useAi && !highlightDetection.aiConfigured}
            <p class="hd-hint muted-2">
              {t("highlightDetection.aiNotConfiguredHint")} ({aiSettingsStore.provider})
            </p>
          {/if}
        </section>

        <section class="hd-section">
          <div class="hd-results-header">
            <h3 class="hd-section-title">{t("highlightDetection.resultsSectionTitle")}</h3>
            {#if highlightDetection.result}
              <span class="hd-signal-badge" class:hd-signal-ai={highlightDetection.usedAiSemanticSignal}>
                {highlightDetection.usedAiSemanticSignal ? t("highlightDetection.aiEnhancedBadge") : t("highlightDetection.localSignalsBadge")}
              </span>
            {/if}
          </div>
          {#if highlightDetection.highlights.length === 0}
            <p class="hd-empty muted-2">{t("highlightDetection.resultsEmpty")}</p>
          {:else}
            <div class="hd-card-list">
              {#each highlightDetection.highlights as highlight, index (highlight.id)}
                <HighlightCard {highlight} {index} />
              {/each}
            </div>
          {/if}
          {#if highlightDetection.seekMissed}
            <p class="hd-hint muted-2">{t("highlightDetection.seekMissed")}</p>
          {/if}
          {#if highlightDetection.addError}
            <div class="hd-error">{highlightDetection.addError}</div>
          {/if}
          {#if highlightDetection.createProjectError}
            <div class="hd-error">{t("highlightDetection.createProjectError", { error: highlightDetection.createProjectError })}</div>
          {/if}
          {#if highlightDetection.exportError}
            <div class="hd-error">{t("highlightDetection.exportClipError", { error: highlightDetection.exportError })}</div>
          {/if}
        </section>

        {#if highlightDetection.lastError}
          <div class="hd-error">{highlightDetection.lastError}</div>
        {/if}
      </div>

      <div class="hd-footer">
        <button class="btn" disabled={!highlightDetection.canDetect} onclick={() => void highlightDetection.detect()}>
          {highlightDetection.detecting ? t("highlightDetection.detecting") : t("highlightDetection.detectButton")}
        </button>
        <span class="hd-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => highlightDetection.close()}>{t("highlightDetection.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .hd-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .hd-dialog {
    width: min(820px, 94vw);
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
  .hd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .hd-title {
    font-size: 13px;
    font-weight: 600;
  }
  .hd-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .hd-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .hd-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .hd-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .hd-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .hd-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .hd-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .hd-number {
    width: 64px;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
    padding: 0 6px;
  }
  .hd-checkbox-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .hd-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .hd-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .hd-results-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .hd-signal-badge {
    font-size: 10px;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .hd-signal-badge.hd-signal-ai {
    color: var(--accent);
    border-color: var(--accent);
  }
  .hd-card-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 420px;
    overflow-y: auto;
  }
  .hd-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .hd-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .hd-footer-spacer {
    flex: 1;
  }
</style>
