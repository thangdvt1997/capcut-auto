<!--
  AI Command Box dialog (Phase 10, master prompt §20): "Natural language ->
  AI Provider -> EditPlan -> Schema validation -> Preview -> Apply". Opened
  from `Timeline.svelte`'s toolbar next to Silence Detector/Filler Words
  (same "detect/generate on whatever's already on the timeline" entry-point
  precedent those two dialogs already establish — see their own doc
  comments) rather than a persistent always-visible bar: this workflow needs
  a track/clip picker and an apply-mode choice exactly like those two
  dialogs, so a lightweight toolbar-triggered dialog reuses that established
  shape instead of inventing a new "floating input bar" pattern for what is,
  structurally, the same propose -> preview -> explicit-apply workflow those
  dialogs already use.

  Pure UI over `stores/aiNlCommand.svelte.ts`. Never applies a generated plan
  without this dialog's own explicit "Apply" click (master prompt §18's
  "User Approves" pipeline stage, a named mandatory stage, not optional UI
  polish) — "Generate Plan" only ever populates a preview. `Zoom` operations
  are listed with an honest "not applied yet" badge (see the store's own doc
  comment for why) rather than being hidden or implied to take effect.
-->
<script lang="ts">
  import { aiNlCommandStore } from "../../stores/aiNlCommand.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import type { Clip, EditOperation } from "../../types/bindings";

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

  function opLabel(op: EditOperation): string {
    return op.type === "remove" ? t("aiCommandBox.opRemoveLabel") : t("aiCommandBox.opZoomLabel");
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      aiNlCommandStore.close();
    }
  }
</script>

{#if aiNlCommandStore.open}
  <div class="ac-backdrop" role="presentation" onclick={() => aiNlCommandStore.close()}>
    <div
      class="ac-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("aiCommandBox.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="ac-header">
        <span class="ac-title">{t("aiCommandBox.title")}</span>
        <button class="btn btn-ghost" onclick={() => aiNlCommandStore.close()} title={t("aiCommandBox.close")}>×</button>
      </div>

      <p class="ac-explainer muted-2">{t("aiCommandBox.explainer")}</p>

      <div class="ac-body">
        <section class="ac-section">
          <h3 class="ac-section-title">{t("aiCommandBox.sourceSectionTitle")}</h3>
          <div class="ac-row">
            <label class="ac-label" for="ac-track">{t("aiCommandBox.trackLabel")}</label>
            <select
              id="ac-track"
              class="ac-select"
              value={aiNlCommandStore.trackId ?? ""}
              onchange={(e) => aiNlCommandStore.setTrack((e.target as HTMLSelectElement).value)}
            >
              {#if aiNlCommandStore.eligibleTracks.length === 0}
                <option value="" disabled>{t("aiCommandBox.noTracks")}</option>
              {/if}
              {#each aiNlCommandStore.eligibleTracks as track (track.id)}
                <option value={track.id}>{track.name} ({track.kind})</option>
              {/each}
            </select>
          </div>
          <div class="ac-row">
            <label class="ac-label" for="ac-clip">{t("aiCommandBox.clipLabel")}</label>
            <select
              id="ac-clip"
              class="ac-select"
              value={aiNlCommandStore.clipId ?? ""}
              disabled={aiNlCommandStore.clipsForSelectedTrack.length === 0}
              onchange={(e) => aiNlCommandStore.setClip((e.target as HTMLSelectElement).value)}
            >
              {#if aiNlCommandStore.clipsForSelectedTrack.length === 0}
                <option value="" disabled>{t("aiCommandBox.noClips")}</option>
              {/if}
              {#each aiNlCommandStore.clipsForSelectedTrack as clip (clip.id)}
                <option value={clip.id}>{clipLabel(clip)}</option>
              {/each}
            </select>
          </div>
          <div class="ac-row">
            <span class="ac-label">{t("aiCommandBox.applyModeLabel")}</span>
            <div class="ac-radio-group">
              <label class="ac-radio">
                <input
                  type="radio"
                  name="ac-apply-mode"
                  value="clip"
                  checked={aiNlCommandStore.applyMode === "clip"}
                  onchange={() => (aiNlCommandStore.applyMode = "clip")}
                />
                {t("aiCommandBox.applyModeClip")}
              </label>
              <label class="ac-radio">
                <input
                  type="radio"
                  name="ac-apply-mode"
                  value="track"
                  checked={aiNlCommandStore.applyMode === "track"}
                  onchange={() => (aiNlCommandStore.applyMode = "track")}
                />
                {t("aiCommandBox.applyModeTrack")}
              </label>
            </div>
          </div>
          {#if aiNlCommandStore.transcriptEntries.length === 0}
            <p class="ac-hint muted-2">{t("aiCommandBox.noTranscriptHint")}</p>
          {/if}
        </section>

        <section class="ac-section">
          <h3 class="ac-section-title">{t("aiCommandBox.commandSectionTitle")}</h3>
          <textarea
            class="ac-textarea"
            rows="2"
            placeholder={t("aiCommandBox.commandPlaceholder")}
            bind:value={aiNlCommandStore.nlCommand}
          ></textarea>
        </section>

        <section class="ac-section">
          <h3 class="ac-section-title">{t("aiCommandBox.planSectionTitle")}</h3>
          {#if !aiNlCommandStore.plan}
            <p class="ac-empty muted-2">{t("aiCommandBox.planEmpty")}</p>
          {:else}
            <p class="ac-summary muted-2">
              {t("aiCommandBox.summaryLine", {
                count: aiNlCommandStore.plan.operations.length,
                removable: aiNlCommandStore.removeOperationsCount,
                zoom: aiNlCommandStore.zoomOperationsCount,
              })}
            </p>
            {#if aiNlCommandStore.previewCuts.length > 0}
              <p class="ac-summary muted-2">
                {t("aiCommandBox.willCutLabel", {
                  count: aiNlCommandStore.previewCuts.length,
                  duration: formatSec(aiNlCommandStore.previewTotalCutUs),
                })}
              </p>
            {/if}
            <ul class="ac-op-list">
              {#each aiNlCommandStore.plan.operations as op, i (i)}
                <li class="ac-op-card">
                  <div class="ac-op-header">
                    <span class="ac-op-type" class:ac-op-type-zoom={op.type === "zoom"}>{opLabel(op)}</span>
                    <span class="ac-op-range mono">{formatTimecode(op.start_us)} – {formatTimecode(op.end_us)}</span>
                    {#if op.type === "zoom"}
                      <span class="ac-op-badge">{t("aiCommandBox.opZoomNotAppliedBadge")}</span>
                    {/if}
                  </div>
                  <p class="ac-op-reason">{op.reason}</p>
                  {#if op.type === "remove" && op.confidence !== null}
                    <span class="ac-op-confidence muted-2">
                      {t("aiCommandBox.opConfidenceLabel")}: {(op.confidence * 100).toFixed(0)}%
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        {#if aiNlCommandStore.lastError}
          <div class="ac-error">{aiNlCommandStore.lastError}</div>
        {/if}
      </div>

      <div class="ac-footer">
        <button class="btn" disabled={!aiNlCommandStore.canGenerate} onclick={() => void aiNlCommandStore.generate()}>
          {aiNlCommandStore.generating ? t("aiCommandBox.generating") : t("aiCommandBox.generateButton")}
        </button>
        <button
          class="btn"
          disabled={!aiNlCommandStore.canApply}
          title={aiNlCommandStore.plan && aiNlCommandStore.removeOperationsCount === 0 ? t("aiCommandBox.noOperationsApplicable") : undefined}
          onclick={() => void aiNlCommandStore.apply()}
        >
          {aiNlCommandStore.applying ? t("aiCommandBox.applying") : t("aiCommandBox.applyButton")}
        </button>
        <button class="btn btn-ghost" onclick={() => void aiNlCommandStore.reset()}>
          {t("aiCommandBox.resetButton")}
        </button>
        <span class="ac-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => aiNlCommandStore.close()}>{t("aiCommandBox.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ac-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .ac-dialog {
    width: min(720px, 94vw);
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
  .ac-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ac-title {
    font-size: 13px;
    font-weight: 600;
  }
  .ac-explainer {
    margin: 0;
    padding: 8px 14px 0;
    font-size: 11px;
    line-height: 1.5;
    flex-shrink: 0;
  }
  .ac-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .ac-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .ac-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .ac-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .ac-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .ac-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .ac-select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .ac-radio-group {
    display: flex;
    gap: 14px;
  }
  .ac-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .ac-hint {
    margin: 0;
    font-size: 10.5px;
  }
  .ac-textarea {
    width: 100%;
    min-height: 48px;
    padding: 8px 10px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    resize: vertical;
  }
  .ac-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .ac-summary {
    margin: 0;
    font-size: 11px;
  }
  .ac-op-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ac-op-card {
    padding: 10px 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ac-op-header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .ac-op-type {
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--accent);
  }
  .ac-op-type-zoom {
    color: var(--muted);
  }
  .ac-op-range {
    font-size: 11px;
    color: var(--muted);
  }
  .ac-op-badge {
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 999px;
    background: hsl(38 92% 60% / 0.15);
    color: hsl(38 92% 55%);
  }
  .ac-op-reason {
    margin: 0;
    font-size: 11.5px;
  }
  .ac-op-confidence {
    font-size: 10.5px;
  }
  .ac-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .ac-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ac-footer-spacer {
    flex: 1;
  }
</style>
