<!--
  Auto-Zoom panel (master prompt §24). Mounted as `RightPanel.svelte`'s real
  "Properties" tab content (replacing that tab's placeholder) — see
  `stores/autoZoom.svelte.ts`'s class doc comment for the full placement
  rationale (auto-zoom is a per-clip property, so this always follows the
  live timeline selection rather than having its own picker).

  Workflow: pick Intensity -> optionally check session markers as manual
  triggers -> optionally "Detect Scenes" (for the long-static-scene trigger)
  -> "Generate Triggers" (previews what will be keyframed) -> "Apply
  Auto-Zoom" (writes real keyframes onto the selected clip, standard
  undo/redo).
-->
<script lang="ts">
  import { autoZoom } from "../../stores/autoZoom.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { ZoomIntensity } from "../../types/bindings";

  const INTENSITIES: ZoomIntensity[] = ["off", "low", "medium", "high"];

  function intensityLabel(i: ZoomIntensity): string {
    return t(`autoZoom.intensity.${i}`);
  }

  function formatSec(us: number): string {
    return `${(us / 1_000_000).toFixed(2)}s`;
  }

  let lastClipId = $state<string | null>(null);
  $effect(() => {
    const id = autoZoom.selectedClip?.id ?? null;
    if (id !== lastClipId) {
      lastClipId = id;
      autoZoom.resetForNewClip();
    }
  });
</script>

<div class="az-panel">
  {#if !autoZoom.selectedClip}
    <p class="az-empty muted-2">{t("autoZoom.noClipSelected")}</p>
  {:else}
    <section class="az-section">
      <h3 class="az-section-title">{t("autoZoom.intensitySectionTitle")}</h3>
      <div class="az-intensity-group" role="radiogroup" aria-label={t("autoZoom.intensitySectionTitle")}>
        {#each INTENSITIES as i (i)}
          <button
            class="az-intensity-btn"
            class:active={autoZoom.intensity === i}
            role="radio"
            aria-checked={autoZoom.intensity === i}
            onclick={() => (autoZoom.intensity = i)}
          >
            {intensityLabel(i)}
          </button>
        {/each}
      </div>
    </section>

    <section class="az-section">
      <h3 class="az-section-title">{t("autoZoom.markersSectionTitle")}</h3>
      <p class="az-hint muted-2">{t("autoZoom.markersHint")}</p>
      {#if timeline.markers.length === 0}
        <p class="az-empty muted-2">{t("autoZoom.noMarkers")}</p>
      {:else}
        <div class="az-marker-list">
          {#each timeline.markers as marker (marker.id)}
            <label class="az-marker-row">
              <input
                type="checkbox"
                checked={autoZoom.manualMarkerIds.has(marker.id)}
                onchange={() => autoZoom.toggleMarker(marker.id)}
              />
              <span class="mono">{formatSec(marker.time_us)}</span>
              <span class="az-marker-label muted-2">{marker.label || t("autoZoom.unlabeledMarker")}</span>
            </label>
          {/each}
        </div>
      {/if}
      <button class="btn btn-ghost btn-sm" onclick={() => timeline.addMarker(timeline.playheadUs)}>
        {t("autoZoom.addMarkerAtPlayheadButton")}
      </button>
    </section>

    <section class="az-section">
      <h3 class="az-section-title">{t("autoZoom.scenesSectionTitle")}</h3>
      <p class="az-hint muted-2">{t("autoZoom.scenesHint")}</p>
      <button class="btn btn-ghost btn-sm" disabled={!autoZoom.canDetectScenes} onclick={() => void autoZoom.detectScenesForClip()}>
        {autoZoom.detectingScenes ? t("autoZoom.detectingScenes") : t("autoZoom.detectScenesButton")}
      </button>
      {#if autoZoom.scenes.length > 0}
        <p class="az-note muted-2">{t("autoZoom.scenesFound", { count: autoZoom.scenes.length })}</p>
      {/if}
      {#if autoZoom.scenesError}
        <div class="az-error">{autoZoom.scenesError}</div>
      {/if}
      <p class="az-hint muted-2">{t("autoZoom.emphasisNotWiredHint")}</p>
    </section>

    <section class="az-section">
      <h3 class="az-section-title">{t("autoZoom.triggersSectionTitle")}</h3>
      <button class="btn btn-ghost" disabled={!autoZoom.canGenerateTriggers} onclick={() => void autoZoom.generateTriggers()}>
        {autoZoom.generatingTriggers ? t("autoZoom.generatingTriggers") : t("autoZoom.generateTriggersButton")}
      </button>
      {#if autoZoom.triggersError}
        <div class="az-error">{autoZoom.triggersError}</div>
      {/if}
      {#if autoZoom.triggers.length === 0}
        <p class="az-empty muted-2">{t("autoZoom.triggersEmpty")}</p>
      {:else}
        <ul class="az-trigger-list">
          {#each autoZoom.triggers as trigger, index (index)}
            <li class="az-trigger-row">
              <span class="mono">{formatSec(trigger.start_us)} → {formatSec(trigger.end_us)}</span>
              <span class="az-trigger-reason muted-2">{trigger.reason}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="az-section az-apply-section">
      <button class="btn" disabled={!autoZoom.canApply} onclick={() => void autoZoom.apply()}>
        {autoZoom.applying ? t("autoZoom.applying") : t("autoZoom.applyButton")}
      </button>
      {#if autoZoom.appliedThisSession}
        <span class="az-note az-note-ok">{t("autoZoom.appliedNote")}</span>
      {/if}
      {#if autoZoom.applyError}
        <div class="az-error">{autoZoom.applyError}</div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .az-panel {
    height: 100%;
    overflow-y: auto;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .az-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .az-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .az-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .az-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .az-intensity-group {
    display: flex;
    gap: 4px;
  }
  .az-intensity-btn {
    flex: 1;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
    cursor: pointer;
  }
  .az-intensity-btn.active {
    border-color: var(--accent);
    background: hsl(213 94% 68% / 0.15);
    color: var(--accent);
  }
  .az-marker-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 120px;
    overflow-y: auto;
  }
  .az-marker-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    cursor: pointer;
  }
  .az-marker-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn-sm {
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
    align-self: flex-start;
  }
  .az-note {
    margin: 0;
    font-size: 10.5px;
  }
  .az-note-ok {
    color: var(--pos);
  }
  .az-trigger-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 160px;
    overflow-y: auto;
  }
  .az-trigger-row {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 4px 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: 11px;
  }
  .az-trigger-reason {
    font-size: 10.5px;
  }
  .az-apply-section {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .az-error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
</style>
