<!--
  Multi-track sync group creation (master prompt §39/§40). Placement
  decision (documented here and in `IMPLEMENTATION_PLAN.md`'s Phase 5
  notes): a small dialog opened from a toolbar button next to the rest of
  `Timeline.svelte`'s multi-select actions (Copy/Paste/Delete), rather than
  living inside the Silence Detector panel — grouping clips is a general
  timeline operation useful outside any one silence-removal pass, and it
  operates on whatever the user already has multi-selected there.

  Workflow: try `create_sync_group_by_timecode` first (the "smart" option,
  no user input needed); on `TIMELINE_TIMECODE_UNAVAILABLE` (or any other
  failure), fall back to a manual per-clip offset (ms) form backed by
  `create_sync_group_manual`. Once created, `timeline::ops`'s existing
  `SyncGroup` propagation (Phase 4) already makes every linked clip
  split/trim/delete together — no further UI is needed for that part.
-->
<script lang="ts">
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let offsetsMs = $state<Record<string, number>>({});
  let attemptedTimecode = $state(false);
  let loading = $state(false);
  let errorMessage = $state<string | null>(null);

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function clipLabel(clipId: string): string {
    const clip = timeline.clips.find((c) => c.id === clipId);
    if (!clip) return clipId;
    const media = clip.media_id ? timeline.mediaById.get(clip.media_id) : undefined;
    const name = media ? basename(media.source_path) : t("timelinePanel.clipEmptyLabel");
    return `${name} (${(clip.position_us / 1_000_000).toFixed(1)}s)`;
  }

  // Reset the form's local state fresh every time the dialog opens against
  // whatever's currently selected.
  $effect(() => {
    if (open) {
      offsetsMs = Object.fromEntries(Array.from(timeline.selectedClipIds, (id) => [id, 0]));
      attemptedTimecode = false;
      errorMessage = null;
    }
  });

  function close(): void {
    open = false;
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }

  async function tryTimecode(): Promise<void> {
    loading = true;
    errorMessage = null;
    try {
      const outcome = await timeline.createSyncGroupByTimecode(Array.from(timeline.selectedClipIds));
      attemptedTimecode = true;
      if (outcome.ok) {
        close();
      } else {
        errorMessage = outcome.error;
      }
    } finally {
      loading = false;
    }
  }

  async function createManual(): Promise<void> {
    loading = true;
    errorMessage = null;
    try {
      const offsetsUs: Record<string, number> = {};
      for (const [id, ms] of Object.entries(offsetsMs)) {
        offsetsUs[id] = Math.round(ms * 1000);
      }
      const outcome = await timeline.createSyncGroupManual(Array.from(timeline.selectedClipIds), offsetsUs);
      if (outcome.ok) {
        close();
      } else {
        errorMessage = outcome.error;
      }
    } finally {
      loading = false;
    }
  }
</script>

{#if open}
  <div class="sg-backdrop" role="presentation" onclick={close}>
    <div
      class="sg-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("syncGroup.dialogTitle")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="sg-header">
        <span class="sg-title">{t("syncGroup.dialogTitle")}</span>
        <button class="btn btn-ghost" onclick={close}>×</button>
      </div>
      <p class="sg-explainer muted-2">{t("syncGroup.explainer")}</p>

      <div class="sg-body">
        <div class="sg-selected">
          <span class="sg-selected-label muted-2">{t("syncGroup.selectedClipsLabel")}</span>
          <ul class="sg-clip-list">
            {#each Array.from(timeline.selectedClipIds) as clipId (clipId)}
              <li>{clipLabel(clipId)}</li>
            {/each}
          </ul>
        </div>

        <button class="btn" disabled={loading} onclick={() => void tryTimecode()}>
          {t("syncGroup.tryTimecodeButton")}
        </button>

        {#if attemptedTimecode && errorMessage}
          <p class="sg-hint muted-2">{t("syncGroup.timecodeUnavailableHint")}</p>
        {/if}

        {#if errorMessage}
          <div class="sg-error">{errorMessage}</div>
        {/if}

        <div class="sg-manual">
          <h3 class="sg-manual-title">{t("syncGroup.manualSectionTitle")}</h3>
          <p class="sg-hint muted-2">{t("syncGroup.manualHint")}</p>
          {#each Array.from(timeline.selectedClipIds) as clipId (clipId)}
            <div class="sg-offset-row">
              <span class="sg-offset-label">{clipLabel(clipId)}</span>
              <input
                type="number"
                class="sg-offset-input"
                step="1"
                value={offsetsMs[clipId] ?? 0}
                oninput={(e) => (offsetsMs = { ...offsetsMs, [clipId]: Number((e.target as HTMLInputElement).value) })}
              />
              <span class="muted-2">ms</span>
            </div>
          {/each}
          <button class="btn" disabled={loading} onclick={() => void createManual()}>
            {t("syncGroup.createManualButton")}
          </button>
        </div>
      </div>
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
    width: min(460px, 92vw);
    max-height: 84vh;
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
  }
  .sg-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .sg-selected-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .sg-clip-list {
    margin: 4px 0 0;
    padding-left: 18px;
    font-size: 11.5px;
  }
  .sg-hint {
    margin: 0;
    font-size: 11px;
  }
  .sg-error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .sg-manual {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .sg-manual-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sg-offset-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sg-offset-label {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sg-offset-input {
    width: 90px;
    height: 24px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11px;
    padding: 0 6px;
  }
</style>
