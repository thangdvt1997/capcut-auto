<!--
  Smart Edit dialog (Phase 10 follow-up, master prompt §19,
  `src-tauri/src/ai/smart_edit.rs`). Same dialog shape as
  `components/filler/FillerWordDetector.svelte` (source track/clip picker ->
  Analyze -> candidates-first review -> Apply -> Reset) — see that
  component's own doc comment for the shared rationale. The one real
  structural difference: each row is a *recommendation* with its own
  AI-suggested action (Keep/Remove/Shorten/Highlight — `SmartEditAction`),
  shown via a button-group the user can override per row, not a single
  checkbox. The live "Preview (what will actually be cut)" section re-runs
  `build_cuts_from_smart_edit_recommendations` against the *effective*
  (possibly overridden) actions, reusing `FillerCandidateStrip.svelte`'s
  existing time-scaled band visualization (it only needs a `Cut[]` +
  `durationUs`, nothing filler-word-specific) rather than a second strip
  component for the same visual idea.

  Opened from `Timeline.svelte`'s toolbar, right next to Silence
  Detector/Filler Words/AI Command (same placement rationale — the user
  already has a clip in context there).
-->
<script lang="ts">
  import { smartEdit } from "../../stores/smartEdit.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import FillerCandidateStrip from "../filler/FillerCandidateStrip.svelte";
  import type { Clip, SmartEditAction, SmartEditRecommendation } from "../../types/bindings";

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

  function categoryLabel(rec: SmartEditRecommendation): string {
    return t(`smartEdit.category.${rec.category}`);
  }

  const ACTIONS: SmartEditAction["type"][] = ["keep", "remove", "shorten", "highlight"];
  function actionLabel(type: SmartEditAction["type"]): string {
    switch (type) {
      case "keep":
        return t("smartEdit.actionKeep");
      case "remove":
        return t("smartEdit.actionRemove");
      case "shorten":
        return t("smartEdit.actionShorten");
      case "highlight":
        return t("smartEdit.actionHighlight");
    }
  }

  function onActionClick(rec: SmartEditRecommendation, type: SmartEditAction["type"]): void {
    if (type === "shorten") {
      smartEdit.setActionToShorten(rec);
    } else {
      smartEdit.setAction(rec, { type });
    }
  }

  let mediaDurationUs = $derived(smartEdit.selectedMedia?.duration_us ?? 0);
  /** `FillerCandidateStrip` wants a `checked` map — every `previewCuts`
   * entry here already *is* something that will be cut (there's no
   * separate checked/unchecked state at the cut level, only at the
   * recommendation-action level above), so it's just an all-true map. */
  let previewChecked = $derived<Record<string, boolean>>(
    Object.fromEntries(smartEdit.previewCuts.map((c) => [c.id, true])),
  );

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      smartEdit.close();
    }
  }
</script>

{#if smartEdit.open}
  <div class="se-backdrop" role="presentation" onclick={() => smartEdit.close()}>
    <div
      class="se-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("smartEdit.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="se-header">
        <span class="se-title">{t("smartEdit.title")}</span>
        <button class="btn btn-ghost" onclick={() => smartEdit.close()} title={t("smartEdit.close")}>×</button>
      </div>

      <p class="se-explainer muted-2">{t("smartEdit.explainer")}</p>

      <div class="se-body">
        <section class="se-section">
          <h3 class="se-section-title">{t("smartEdit.sourceSectionTitle")}</h3>
          <div class="se-row">
            <label class="se-label" for="se-track">{t("silenceDetector.trackLabel")}</label>
            <select
              id="se-track"
              class="se-select"
              value={smartEdit.trackId ?? ""}
              onchange={(e) => smartEdit.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if smartEdit.eligibleTracks.length === 0}
                <option value="" disabled>{t("silenceDetector.noTracks")}</option>
              {/if}
              {#each smartEdit.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="se-row">
            <label class="se-label" for="se-clip">{t("silenceDetector.clipLabel")}</label>
            <select
              id="se-clip"
              class="se-select"
              value={smartEdit.clipId ?? ""}
              disabled={smartEdit.clipsForSelectedTrack.length === 0}
              onchange={(e) => smartEdit.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if smartEdit.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("silenceDetector.noClips")}</option>
              {/if}
              {#each smartEdit.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          <div class="se-row">
            <span class="se-label">{t("silenceDetector.applyModeLabel")}</span>
            <div class="se-radio-group">
              <label class="se-radio">
                <input type="radio" name="se-apply-mode" value="clip" checked={smartEdit.applyMode === "clip"} onchange={() => (smartEdit.applyMode = "clip")} />
                {t("silenceDetector.applyModeClip")}
              </label>
              <label class="se-radio">
                <input type="radio" name="se-apply-mode" value="track" checked={smartEdit.applyMode === "track"} onchange={() => (smartEdit.applyMode = "track")} />
                {t("silenceDetector.applyModeTrack")}
              </label>
            </div>
          </div>
          {#if smartEdit.selectedMedia && smartEdit.transcriptEntries.length === 0}
            <p class="se-hint muted-2">{t("smartEdit.noTranscriptHint")}</p>
          {/if}
          {#if !smartEdit.aiConfigured}
            <p class="se-hint muted-2">
              {t("smartEdit.aiNotConfiguredHint")} ({aiSettingsStore.provider})
            </p>
          {/if}
        </section>

        <section class="se-section">
          <h3 class="se-section-title">{t("smartEdit.recommendationsSectionTitle")}</h3>
          {#if smartEdit.recommendations.length === 0}
            <p class="se-empty muted-2">{t("smartEdit.recommendationsEmpty")}</p>
          {:else}
            <ul class="se-rec-list">
              {#each smartEdit.recommendations as rec (rec.id)}
                {@const action = smartEdit.actionFor(rec)}
                <li class="se-rec-row">
                  <div class="se-rec-meta">
                    <span class="se-badge se-badge-category">{categoryLabel(rec)}</span>
                    <span class="se-badge se-badge-confidence">{t("smartEdit.confidenceLabel")}: {Math.round(rec.confidence * 100)}%</span>
                    <span class="se-time mono">{formatSec(rec.start_us)} – {formatSec(rec.end_us)}</span>
                    <button class="btn btn-ghost se-seek-btn" onclick={() => smartEdit.seekToRecommendation(rec)} title={t("smartEdit.seekButton")}>⏵</button>
                  </div>
                  <p class="se-rec-transcript">{rec.transcript}</p>
                  <p class="se-rec-reason muted-2">{rec.reason}</p>
                  <div class="se-action-group">
                    {#each ACTIONS as type (type)}
                      <button
                        class="se-action-btn se-action-{type}"
                        class:active={action.type === type}
                        onclick={() => onActionClick(rec, type)}
                      >
                        {actionLabel(type)}
                      </button>
                    {/each}
                    {#if action.type === "shorten"}
                      <label class="se-shorten-input">
                        {t("smartEdit.shortenTargetMsLabel")}
                        <input
                          type="number"
                          min="1"
                          max={Math.max(1, Math.round((rec.end_us - rec.start_us) / 1000) - 1)}
                          step="10"
                          value={Math.round(action.target_duration_us / 1000)}
                          onchange={(e) => smartEdit.setShortenTargetMs(rec, Number((e.target as HTMLInputElement).value))}
                        />
                      </label>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
          {#if smartEdit.seekMissed}
            <p class="se-hint muted-2">{t("smartEdit.seekMissed")}</p>
          {/if}
        </section>

        {#if smartEdit.recommendations.length > 0}
          <section class="se-section">
            <h3 class="se-section-title">{t("smartEdit.previewSectionTitle")}</h3>
            {#if smartEdit.previewCuts.length === 0}
              <p class="se-empty muted-2">{t("smartEdit.previewEmpty")}</p>
            {:else}
              <p class="se-hint muted-2">
                {t("smartEdit.previewCutCount", {
                  count: smartEdit.previewCuts.length,
                  seconds: (smartEdit.totalPreviewDurationUs / 1_000_000).toFixed(1),
                })}
              </p>
              <FillerCandidateStrip durationUs={mediaDurationUs} candidates={smartEdit.previewCuts} checked={previewChecked} />
            {/if}
          </section>
        {/if}

        {#if smartEdit.lastError}
          <div class="se-error">{smartEdit.lastError}</div>
        {/if}
      </div>

      <div class="se-footer">
        <button class="btn" disabled={!smartEdit.canAnalyze} onclick={() => void smartEdit.analyze()}>
          {smartEdit.analyzing ? t("smartEdit.analyzing") : t("smartEdit.analyzeButton")}
        </button>
        <button class="btn" disabled={!smartEdit.canApply} onclick={() => void smartEdit.apply()}>
          {smartEdit.applying ? t("smartEdit.applying") : t("smartEdit.applyButton")}
        </button>
        <button class="btn btn-ghost" onclick={() => void smartEdit.reset()}>{t("smartEdit.resetButton")}</button>
        <span class="se-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => smartEdit.close()}>{t("smartEdit.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .se-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .se-dialog {
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
  .se-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .se-title {
    font-size: 13px;
    font-weight: 600;
  }
  .se-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .se-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .se-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .se-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .se-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .se-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .se-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .se-radio-group {
    display: flex;
    gap: 14px;
  }
  .se-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .se-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .se-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .se-rec-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 320px;
    overflow-y: auto;
  }
  .se-rec-row {
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .se-rec-meta {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .se-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .se-badge-category {
    color: var(--accent);
    border-color: var(--accent);
  }
  .se-time {
    color: var(--muted);
    font-size: 11px;
    margin-left: auto;
  }
  .se-seek-btn {
    padding: 0 6px;
    font-size: 11px;
  }
  .se-rec-transcript {
    margin: 0;
    font-size: 12px;
  }
  .se-rec-reason {
    margin: 0;
    font-size: 10.5px;
  }
  .se-action-group {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    margin-top: 2px;
  }
  .se-action-btn {
    font-size: 10.5px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .se-action-btn.active.se-action-keep {
    background: hsl(210 20% 60% / 0.25);
    color: var(--foreground);
    border-color: var(--border-strong);
  }
  .se-action-btn.active.se-action-remove {
    background: hsl(0 84% 65% / 0.2);
    color: var(--neg);
    border-color: var(--neg);
  }
  .se-action-btn.active.se-action-shorten {
    background: hsl(38 92% 60% / 0.2);
    color: var(--warn);
    border-color: var(--warn);
  }
  .se-action-btn.active.se-action-highlight {
    background: hsl(213 94% 68% / 0.2);
    color: var(--accent);
    border-color: var(--accent);
  }
  .se-shorten-input {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 10.5px;
    color: var(--muted);
    margin-left: 4px;
  }
  .se-shorten-input input {
    width: 64px;
    height: 22px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
    padding: 0 4px;
  }
  .se-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .se-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .se-footer-spacer {
    flex: 1;
  }
</style>
