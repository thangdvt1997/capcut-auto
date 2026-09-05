<!--
  Short Video Generator wizard (Phase 11 flagship feature, master prompt
  §22, `src-tauri/src/shorts/`, `commands::shorts::generate_shorts`). Two
  steps: Settings (source clip + duration/aspect/clip-count/auto-zoom/AI) ->
  Results (one `ShortCandidateCard.svelte` per generated `ShortCandidate`,
  "Load into editor" per card — see `stores/shortsGenerator.svelte.ts`'s
  class doc comment for the full design rationale, including why there is
  no separate "Preview" action and how "no transcript yet" is handled).

  Opened from `Timeline.svelte`'s toolbar, alongside Silence Detector/Filler
  Words/AI Command/Smart Edit/Highlights — this is the natural home for
  every "analyze the selected clip's media, propose something" dialog in
  this app, and this feature composes directly on top of Highlight
  Detection (Phase 10), so it belongs in the same toolbar group rather than
  a new, separate entry point (e.g. the File menu) that would split this
  family of features across two different places in the UI.
-->
<script lang="ts">
  import { shortsGenerator, ASPECT_OPTIONS, CLIP_COUNT_PRESETS } from "../../stores/shortsGenerator.svelte";
  import type { DurationPresetKind } from "../../stores/shortsGenerator.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import ShortCandidateCard from "./ShortCandidateCard.svelte";
  import type { Clip, ShortsAspect } from "../../types/bindings";

  const DURATION_PRESETS: DurationPresetKind[] = ["fixed_15", "fixed_30", "fixed_60", "fixed_90", "custom"];

  function durationLabel(preset: DurationPresetKind): string {
    switch (preset) {
      case "fixed_15":
        return "15s";
      case "fixed_30":
        return "30s";
      case "fixed_60":
        return "60s";
      case "fixed_90":
        return "90s";
      case "custom":
        return t("shortsGenerator.durationCustom");
    }
  }

  function aspectLabel(aspect: ShortsAspect): string {
    switch (aspect) {
      case "vertical_9x_16":
        return "9:16";
      case "square_1x_1":
        return "1:1";
      case "portrait_4x_5":
        return "4:5";
    }
  }

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
      shortsGenerator.close();
    }
  }
</script>

{#if shortsGenerator.open}
  <div class="sg-backdrop" role="presentation" onclick={() => shortsGenerator.close()}>
    <div
      class="sg-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("shortsGenerator.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="sg-header">
        <span class="sg-title">{t("shortsGenerator.title")}</span>
        <button class="btn btn-ghost" onclick={() => shortsGenerator.close()} title={t("shortsGenerator.close")}>×</button>
      </div>

      <p class="sg-explainer muted-2">{t("shortsGenerator.explainer")}</p>

      {#if shortsGenerator.step === "settings"}
        <div class="sg-body">
          <section class="sg-section">
            <h3 class="sg-section-title">{t("highlightDetection.sourceSectionTitle")}</h3>
            <div class="sg-row">
              <label class="sg-label" for="sg-track">{t("silenceDetector.trackLabel")}</label>
              <select
                id="sg-track"
                class="sg-select"
                value={shortsGenerator.trackId ?? ""}
                onchange={(e) => shortsGenerator.setTrack((e.target as HTMLSelectElement).value)}
              >
                {#if shortsGenerator.eligibleTracks.length === 0}
                  <option value="" disabled>{t("silenceDetector.noTracks")}</option>
                {/if}
                {#each shortsGenerator.eligibleTracks as track (track.id)}
                  <option value={track.id}>{track.name} ({track.kind})</option>
                {/each}
              </select>
            </div>
            <div class="sg-row">
              <label class="sg-label" for="sg-clip">{t("silenceDetector.clipLabel")}</label>
              <select
                id="sg-clip"
                class="sg-select"
                value={shortsGenerator.clipId ?? ""}
                disabled={shortsGenerator.clipsForSelectedTrack.length === 0}
                onchange={(e) => shortsGenerator.setClip((e.target as HTMLSelectElement).value)}
              >
                {#if shortsGenerator.clipsForSelectedTrack.length === 0}
                  <option value="" disabled>{t("silenceDetector.noClips")}</option>
                {/if}
                {#each shortsGenerator.clipsForSelectedTrack as clip (clip.id)}
                  <option value={clip.id}>{clipLabel(clip)}</option>
                {/each}
              </select>
            </div>
            {#if shortsGenerator.selectedMedia && !shortsGenerator.hasTranscript}
              <p class="sg-hint sg-hint-warn">{t("shortsGenerator.noTranscriptNote")}</p>
            {/if}
          </section>

          <section class="sg-section">
            <h3 class="sg-section-title">{t("shortsGenerator.durationSectionTitle")}</h3>
            <div class="sg-pill-row">
              {#each DURATION_PRESETS as preset (preset)}
                <button
                  class="sg-pill"
                  class:selected={shortsGenerator.durationPreset === preset}
                  onclick={() => shortsGenerator.setDurationPreset(preset)}
                >
                  {durationLabel(preset)}
                </button>
              {/each}
            </div>
            {#if shortsGenerator.durationPreset === "custom"}
              <div class="sg-row">
                <label class="sg-label" for="sg-custom-seconds">{t("shortsGenerator.customSecondsLabel")}</label>
                <input
                  id="sg-custom-seconds"
                  type="number"
                  min="1"
                  max="600"
                  class="sg-number"
                  value={shortsGenerator.customSeconds}
                  onchange={(e) => shortsGenerator.setCustomSeconds(Number((e.target as HTMLInputElement).value))}
                />
              </div>
            {/if}
          </section>

          <section class="sg-section">
            <h3 class="sg-section-title">{t("shortsGenerator.aspectSectionTitle")}</h3>
            <div class="sg-pill-row">
              {#each ASPECT_OPTIONS as aspect (aspect)}
                <button
                  class="sg-pill"
                  class:selected={shortsGenerator.aspect === aspect}
                  onclick={() => shortsGenerator.setAspect(aspect)}
                >
                  {aspectLabel(aspect)}
                </button>
              {/each}
            </div>
          </section>

          <section class="sg-section">
            <h3 class="sg-section-title">{t("shortsGenerator.clipCountSectionTitle")}</h3>
            <div class="sg-pill-row">
              {#each CLIP_COUNT_PRESETS as count (count)}
                <button
                  class="sg-pill"
                  class:selected={shortsGenerator.clipCount === count}
                  onclick={() => shortsGenerator.setClipCount(count)}
                >
                  {count}
                </button>
              {/each}
            </div>
          </section>

          <section class="sg-section">
            <h3 class="sg-section-title">{t("shortsGenerator.optionsSectionTitle")}</h3>
            <label class="sg-checkbox-row">
              <input type="checkbox" bind:checked={shortsGenerator.applyZoom} />
              {t("shortsGenerator.applyZoomLabel")}
            </label>
            <label class="sg-checkbox-row">
              <input type="checkbox" bind:checked={shortsGenerator.useAi} />
              {t("highlightDetection.useAiLabel")}
            </label>
            {#if shortsGenerator.useAi && !shortsGenerator.aiConfigured}
              <p class="sg-hint muted-2">
                {t("highlightDetection.aiNotConfiguredHint")} ({aiSettingsStore.provider})
              </p>
            {/if}
          </section>

          {#if shortsGenerator.lastError}
            <div class="sg-error">{shortsGenerator.lastError}</div>
          {/if}
        </div>

        <div class="sg-footer">
          <button class="btn" disabled={!shortsGenerator.canGenerate} onclick={() => void shortsGenerator.generate()}>
            {shortsGenerator.generating ? t("shortsGenerator.generating") : t("shortsGenerator.generateButton")}
          </button>
          <span class="sg-footer-spacer"></span>
          <button class="btn btn-ghost" onclick={() => shortsGenerator.close()}>{t("shortsGenerator.closeButton")}</button>
        </div>
      {:else}
        <div class="sg-body">
          <section class="sg-section">
            <div class="sg-results-header">
              <h3 class="sg-section-title">{t("shortsGenerator.resultsSectionTitle")}</h3>
              <span class="sg-results-count muted-2">{t("shortsGenerator.resultsCount", { count: shortsGenerator.candidates.length })}</span>
            </div>
            {#if shortsGenerator.candidates.length === 0}
              <p class="sg-empty muted-2">{t("shortsGenerator.resultsEmpty")}</p>
            {:else}
              <div class="sg-card-list">
                {#each shortsGenerator.candidates as candidate, index (candidate.highlight.id)}
                  <ShortCandidateCard {candidate} {index} />
                {/each}
              </div>
            {/if}
            {#if shortsGenerator.loadError}
              <div class="sg-error">{t("shortsGenerator.loadError", { error: shortsGenerator.loadError })}</div>
            {/if}
          </section>
        </div>

        <div class="sg-footer">
          <button class="btn btn-ghost" onclick={() => shortsGenerator.backToSettings()}>{t("shortsGenerator.backButton")}</button>
          <span class="sg-footer-spacer"></span>
          <button class="btn btn-ghost" onclick={() => shortsGenerator.close()}>{t("shortsGenerator.closeButton")}</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .sg-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .sg-dialog {
    width: min(860px, 94vw);
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
  .sg-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sg-title {
    font-size: 13px;
    font-weight: 600;
  }
  .sg-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .sg-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .sg-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .sg-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sg-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .sg-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .sg-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .sg-number {
    width: 72px;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
    padding: 0 6px;
  }
  .sg-pill-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .sg-pill {
    height: 26px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--foreground);
    font-size: 11.5px;
    cursor: pointer;
  }
  .sg-pill.selected {
    border-color: var(--accent);
    color: var(--accent);
    background: hsl(213 94% 68% / 0.1);
  }
  .sg-checkbox-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .sg-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .sg-hint-warn {
    color: var(--warn);
  }
  .sg-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .sg-results-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .sg-results-count {
    font-size: 10.5px;
  }
  .sg-card-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 520px;
    overflow-y: auto;
  }
  .sg-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .sg-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sg-footer-spacer {
    flex: 1;
  }
</style>
