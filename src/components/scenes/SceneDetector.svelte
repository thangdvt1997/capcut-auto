<!--
  Scene Detector dialog (master prompt §25, `src-tauri/src/media/scene.rs` /
  `src-tauri/src/commands/scenes.rs`). Same source track/clip picker shape as
  `SilenceDetector.svelte`/`FillerWordDetector.svelte`/
  `HighlightDetectionDialog.svelte` — the panel always analyzes "the
  currently selected timeline clip's underlying media". Detected scenes
  render as a thumbnail+score card grid (master prompt §25's
  `Scene{start, end, thumbnail, score}`), each individually checkable (same
  per-candidate checkbox shape `FillerWordDetector.svelte` established) and
  feeding three actions: Split at Selected / Remove Selected / Generate
  Highlights from Scenes (the last one handing off to the existing
  Highlight Detection dialog — see `stores/highlightDetection.svelte.ts`'s
  `showExternalHighlights` doc comment).

  Opened from `Timeline.svelte`'s toolbar, matching Silence Detector/Filler
  Words/Smart Edit/Highlight Detection's placement.
-->
<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { sceneDetector } from "../../stores/sceneDetector.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
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

  function scoreClass(score: number): string {
    if (score >= 0.66) return "sc-score-high";
    if (score >= 0.33) return "sc-score-mid";
    return "sc-score-low";
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      sceneDetector.close();
    }
  }
</script>

{#if sceneDetector.open}
  <div class="scn-backdrop" role="presentation" onclick={() => sceneDetector.close()}>
    <div
      class="scn-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("sceneDetector.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="scn-header">
        <span class="scn-title">{t("sceneDetector.title")}</span>
        <button class="btn btn-ghost" onclick={() => sceneDetector.close()} title={t("sceneDetector.close")}>×</button>
      </div>

      <p class="scn-explainer muted-2">{t("sceneDetector.explainer")}</p>

      <div class="scn-body">
        <section class="scn-section">
          <h3 class="scn-section-title">{t("sceneDetector.sourceSectionTitle")}</h3>
          <div class="scn-row">
            <label class="scn-label" for="scn-track">{t("silenceDetector.trackLabel")}</label>
            <select
              id="scn-track"
              class="scn-select"
              value={sceneDetector.trackId ?? ""}
              onchange={(e) => sceneDetector.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if sceneDetector.eligibleTracks.length === 0}
                <option value="" disabled>{t("silenceDetector.noTracks")}</option>
              {/if}
              {#each sceneDetector.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="scn-row">
            <label class="scn-label" for="scn-clip">{t("silenceDetector.clipLabel")}</label>
            <select
              id="scn-clip"
              class="scn-select"
              value={sceneDetector.clipId ?? ""}
              disabled={sceneDetector.clipsForSelectedTrack.length === 0}
              onchange={(e) => sceneDetector.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if sceneDetector.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("silenceDetector.noClips")}</option>
              {/if}
              {#each sceneDetector.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          <div class="scn-row">
            <span class="scn-label">{t("silenceDetector.applyModeLabel")}</span>
            <div class="scn-radio-group">
              <label class="scn-radio">
                <input type="radio" name="scn-apply-mode" value="clip" checked={sceneDetector.applyMode === "clip"} onchange={() => (sceneDetector.applyMode = "clip")} />
                {t("silenceDetector.applyModeClip")}
              </label>
              <label class="scn-radio">
                <input type="radio" name="scn-apply-mode" value="track" checked={sceneDetector.applyMode === "track"} onchange={() => (sceneDetector.applyMode = "track")} />
                {t("silenceDetector.applyModeTrack")}
              </label>
            </div>
            <span class="scn-hint muted-2">{t("sceneDetector.applyModeHint")}</span>
          </div>
        </section>

        <section class="scn-section">
          <h3 class="scn-section-title">{t("sceneDetector.paramsSectionTitle")}</h3>
          <div class="scn-slider-row">
            <label class="scn-label" for="scn-threshold">{t("sceneDetector.thresholdLabel")}</label>
            <input id="scn-threshold" type="range" min="1" max="99" step="1" bind:value={sceneDetector.thresholdPct} />
            <span class="scn-value mono">{sceneDetector.thresholdPct}%</span>
          </div>
          <p class="scn-hint muted-2">{t("sceneDetector.thresholdHint")}</p>
        </section>

        <section class="scn-section">
          <div class="scn-results-header">
            <h3 class="scn-section-title">{t("sceneDetector.resultsSectionTitle")}</h3>
            {#if sceneDetector.scenes.length > 0}
              <span class="scn-select-actions">
                <button class="btn btn-ghost btn-sm" onclick={() => sceneDetector.selectAll()}>{t("sceneDetector.selectAllButton")}</button>
                <button class="btn btn-ghost btn-sm" onclick={() => sceneDetector.deselectAll()}>{t("sceneDetector.deselectAllButton")}</button>
              </span>
            {/if}
          </div>
          {#if sceneDetector.scenes.length === 0}
            <p class="scn-empty muted-2">{t("sceneDetector.resultsEmpty")}</p>
          {:else}
            <p class="scn-summary muted-2">{t("sceneDetector.summaryLine", { count: sceneDetector.scenes.length, checked: sceneDetector.checkedScenes.length })}</p>
            <div class="scn-card-grid">
              {#each sceneDetector.scenes as scene, index (scene.id)}
                {@const isChecked = sceneDetector.checked[scene.id] ?? false}
                <label class="scn-card" class:checked={isChecked}>
                  <input type="checkbox" checked={isChecked} onchange={() => sceneDetector.toggleScene(scene.id)} />
                  <div class="scn-thumb">
                    {#if scene.thumbnail_path}
                      <img src={convertFileSrc(scene.thumbnail_path)} alt="" loading="lazy" />
                    {:else}
                      <span class="scn-thumb-fallback muted-2">{t("sceneDetector.noThumbnail")}</span>
                    {/if}
                  </div>
                  <div class="scn-card-meta">
                    <span class="scn-card-index">{t("sceneDetector.sceneLabel")} #{index + 1}</span>
                    <span class="scn-time mono">{formatSec(scene.start_us)} → {formatSec(scene.end_us)}</span>
                    <span class="scn-score {scoreClass(scene.score)}">{t("sceneDetector.scoreLabel")}: {scene.score.toFixed(2)}</span>
                  </div>
                </label>
              {/each}
            </div>
          {/if}
        </section>

        {#if sceneDetector.lastError}
          <div class="scn-error">{sceneDetector.lastError}</div>
        {/if}
      </div>

      <div class="scn-footer">
        <button class="btn" disabled={!sceneDetector.canDetect} onclick={() => void sceneDetector.detect()}>
          {sceneDetector.detecting ? t("sceneDetector.detecting") : t("sceneDetector.detectButton")}
        </button>
        <button class="btn btn-ghost" disabled={!sceneDetector.canSplit} onclick={() => void sceneDetector.splitAtSelected()}>
          {sceneDetector.splitting ? t("sceneDetector.splitting") : t("sceneDetector.splitButton")}
        </button>
        <button class="btn btn-ghost" disabled={!sceneDetector.canRemove} onclick={() => void sceneDetector.removeSelected()}>
          {sceneDetector.removing ? t("sceneDetector.removing") : t("sceneDetector.removeButton")}
        </button>
        <button class="btn btn-ghost" disabled={!sceneDetector.canGenerateHighlights} onclick={() => void sceneDetector.generateHighlights()}>
          {sceneDetector.generatingHighlights ? t("sceneDetector.generatingHighlights") : t("sceneDetector.generateHighlightsButton")}
        </button>
        <span class="scn-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => sceneDetector.close()}>{t("sceneDetector.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .scn-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .scn-dialog {
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
  .scn-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .scn-title {
    font-size: 13px;
    font-weight: 600;
  }
  .scn-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .scn-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .scn-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .scn-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .scn-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex-wrap: wrap;
  }
  .scn-slider-row {
    display: grid;
    grid-template-columns: 130px 1fr 64px;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .scn-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .scn-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .scn-radio-group {
    display: flex;
    gap: 14px;
  }
  .scn-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .scn-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .scn-value {
    font-size: 11px;
    text-align: right;
    color: var(--muted);
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .scn-results-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 6px;
  }
  .scn-select-actions {
    display: flex;
    gap: 6px;
  }
  .btn-sm {
    height: 22px;
    padding: 0 8px;
    font-size: 10.5px;
  }
  .scn-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .scn-summary {
    margin: 0;
    font-size: 11px;
  }
  .scn-card-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
    max-height: 420px;
    overflow-y: auto;
  }
  .scn-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    position: relative;
  }
  .scn-card.checked {
    border-color: var(--accent);
    background: hsl(213 94% 68% / 0.08);
  }
  .scn-card input[type="checkbox"] {
    position: absolute;
    top: 6px;
    left: 6px;
    z-index: 1;
  }
  .scn-thumb {
    aspect-ratio: 16 / 9;
    background: var(--elevated);
    border-radius: var(--radius-sm);
    overflow: hidden;
    display: grid;
    place-items: center;
  }
  .scn-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .scn-thumb-fallback {
    font-size: 10px;
    text-align: center;
    padding: 4px;
  }
  .scn-card-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .scn-card-index {
    font-size: 10.5px;
    font-weight: 600;
    color: var(--muted);
  }
  .scn-time {
    font-size: 11px;
    font-weight: 600;
  }
  .scn-score {
    font-size: 10px;
    font-weight: 700;
    align-self: flex-start;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid currentColor;
  }
  .scn-score-high {
    color: var(--pos);
  }
  .scn-score-mid {
    color: var(--warn);
  }
  .scn-score-low {
    color: var(--neg);
  }
  .scn-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .scn-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .scn-footer-spacer {
    flex: 1;
  }
</style>
