<!--
  B-Roll panel (master prompt §34). Mounted inside `TranscriptEditor.svelte`
  (below the transcript itself) since suggestions are transcript-driven and
  that panel already owns "the selected clip's own transcript" — see
  `stores/broll.svelte.ts`'s class doc comment for the full placement
  rationale and the "Add to timeline" bridge it reuses from
  `MediaLibrary.svelte`.
-->
<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { brollStore } from "../../stores/broll.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { t } from "../../lib/i18n.svelte";

  function formatSec(us: number): string {
    return `${(us / 1_000_000).toFixed(2)}s`;
  }
</script>

<div class="br-panel">
  <div class="br-header">
    <h3 class="br-title">{t("broll.title")}</h3>
    <button class="btn btn-ghost btn-sm" disabled={!brollStore.canSuggest} onclick={() => void brollStore.suggest()}>
      {brollStore.suggesting ? t("broll.suggesting") : t("broll.suggestButton")}
    </button>
  </div>
  <p class="br-explainer muted-2">{t("broll.explainer")}</p>

  {#if !brollStore.aiConfigured}
    <p class="br-hint muted-2">{t("broll.aiNotConfiguredHint")} ({aiSettingsStore.provider})</p>
  {/if}

  {#if brollStore.lastError}
    <div class="br-error">{brollStore.lastError}</div>
  {/if}
  {#if brollStore.addError}
    <div class="br-error">{brollStore.addError}</div>
  {/if}

  {#if brollStore.results.length > 0}
    <div class="br-list">
      {#each brollStore.results as item (item.suggestion.id)}
        <div class="br-suggestion-card">
          <div class="br-suggestion-header">
            <span class="br-keyword">{item.suggestion.keyword}</span>
            <span class="br-time mono">
              {formatSec(item.suggestion.insertion_time_us)} · {formatSec(item.suggestion.duration_us)}
            </span>
          </div>
          <p class="br-reason muted-2">{item.suggestion.reason}</p>

          {#if item.candidates.length === 0}
            <p class="br-empty-candidates muted-2">{t("broll.noLocalCandidates")}</p>
          {:else}
            <div class="br-candidate-grid">
              {#each item.candidates as candidate (candidate.media_id)}
                <div class="br-candidate-card">
                  <div class="br-candidate-thumb">
                    {#if candidate.thumbnail_path}
                      <img src={convertFileSrc(candidate.thumbnail_path)} alt="" loading="lazy" />
                    {:else}
                      <span class="muted-2">{candidate.kind}</span>
                    {/if}
                  </div>
                  <span class="br-candidate-filename" title={candidate.path}>{candidate.filename}</span>
                  <button
                    class="btn btn-ghost btn-sm"
                    disabled={brollStore.isAdding(item.suggestion.id, candidate)}
                    onclick={() => void brollStore.addToTimeline(item.suggestion.id, item.suggestion.duration_us, candidate)}
                  >
                    {brollStore.isAdding(item.suggestion.id, candidate)
                      ? t("broll.adding")
                      : brollStore.isAdded(item.suggestion.id, candidate)
                        ? t("broll.added")
                        : t("broll.addToTimelineButton")}
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {:else if !brollStore.suggesting}
    <p class="br-empty muted-2">{t("broll.resultsEmpty")}</p>
  {/if}
</div>

<style>
  .br-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0 0;
    border-top: 1px solid var(--border);
    margin-top: 10px;
  }
  .br-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .br-title {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .btn-sm {
    height: 22px;
    padding: 0 8px;
    font-size: 10.5px;
  }
  .br-explainer {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .br-hint {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .br-empty {
    margin: 0;
    font-size: 11px;
  }
  .br-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 320px;
    overflow-y: auto;
  }
  .br-suggestion-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .br-suggestion-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .br-keyword {
    font-size: 11.5px;
    font-weight: 600;
  }
  .br-time {
    font-size: 10.5px;
    color: var(--muted);
    flex-shrink: 0;
  }
  .br-reason {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .br-empty-candidates {
    margin: 0;
    font-size: 10.5px;
    font-style: italic;
  }
  .br-candidate-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(90px, 1fr));
    gap: 6px;
  }
  .br-candidate-card {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .br-candidate-thumb {
    aspect-ratio: 16 / 9;
    background: var(--elevated);
    border-radius: var(--radius-sm);
    overflow: hidden;
    display: grid;
    place-items: center;
    font-size: 9.5px;
  }
  .br-candidate-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .br-candidate-filename {
    font-size: 9.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .br-error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
</style>
