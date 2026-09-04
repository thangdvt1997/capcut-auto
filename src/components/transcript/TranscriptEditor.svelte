<!--
  Transcript Editor panel (master prompt §15), mounted as `LeftPanel.svelte`'s
  real "Transcript" tab (replacing that tab's Phase-2 placeholder).

  Scope (task brief): shows the transcript for "the currently-selected
  timeline clip's underlying media" — `transcriptEditor.anchorClip`/
  `anchorMedia` follow `stores/timeline.svelte.ts`'s own `selectedClipIds`
  directly, so this panel has no independent media picker of its own.

  Layout, top to bottom:
    1. Mode toggle (Transcript Text Edit / Video Edit Through Transcript) —
       master prompt §15's single most important UX safety requirement:
       always visible, never ambiguous which mode is active.
    2. Empty states: no clip selected: no media; media with no transcript yet
       (Transcribe workflow, including the "no model installed -> Model
       Manager" hand-off).
    3. The transcript itself (`TranscriptEntryRow` per entry).
    4. In Video Edit mode only: the staged-deletions action bar and its
       explicit confirm step — never an implicit auto-apply.
-->
<script lang="ts">
  import { transcriptEditor } from "../../stores/transcriptEditor.svelte";
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import TranscriptEntryRow from "./TranscriptEntryRow.svelte";

  $effect(() => {
    if (transcriptEditor.anchorMedia && !transcriptEditor.hasTranscript) {
      void transcriptEditor.ensureModelsLoaded();
    }
  });

  let progressLabel = $derived.by(() => {
    const p = transcriptEditor.progress;
    if (!p) return "";
    if (p.error) return t("transcriptEditor.transcribeFailed", { error: p.error });
    if (p.done) return t("transcriptEditor.transcribeDone");
    return p.percent !== null ? t("transcriptEditor.transcribingPercent", { percent: p.percent }) : t("transcriptEditor.transcribing");
  });
</script>

<div class="panel">
  <div class="mode-toggle" role="radiogroup" aria-label={t("transcriptEditor.modeGroupLabel")}>
    <button
      class="mode-btn"
      class:active={transcriptEditor.mode === "text"}
      role="radio"
      aria-checked={transcriptEditor.mode === "text"}
      onclick={() => transcriptEditor.setMode("text")}
    >
      {t("transcriptEditor.modeText")}
    </button>
    <button
      class="mode-btn mode-btn-danger"
      class:active={transcriptEditor.mode === "video"}
      role="radio"
      aria-checked={transcriptEditor.mode === "video"}
      onclick={() => transcriptEditor.setMode("video")}
    >
      {t("transcriptEditor.modeVideo")}
    </button>
  </div>
  <p class="mode-explainer muted-2">
    {transcriptEditor.mode === "text" ? t("transcriptEditor.modeTextExplainer") : t("transcriptEditor.modeVideoExplainer")}
  </p>

  {#if transcriptEditor.seekMissed}
    <div class="notice">{t("transcriptEditor.seekMissed")}</div>
  {/if}

  <div class="body">
    {#if !transcriptEditor.anchorMedia}
      <p class="empty muted-2">{t("transcriptEditor.noClipSelected")}</p>
    {:else if !transcriptEditor.hasTranscript}
      <div class="transcribe-box">
        <p class="empty muted-2">{t("transcriptEditor.noTranscript")}</p>

        {#if transcriptEditor.transcribing}
          <p class="progress-line">{progressLabel}</p>
          <button class="btn btn-ghost" onclick={() => void transcriptEditor.cancelTranscription()}>
            {t("transcriptEditor.cancelButton")}
          </button>
        {:else if transcriptEditor.progress?.done}
          <p class="progress-line" class:error={!!transcriptEditor.progress.error}>{progressLabel}</p>
          <button class="btn btn-ghost" onclick={() => transcriptEditor.dismissJob()}>{t("transcriptEditor.tryAgainButton")}</button>
        {:else if transcriptEditor.modelsLoading}
          <p class="muted-2">{t("transcriptEditor.loadingModels")}</p>
        {:else if transcriptEditor.installedModels.length === 0}
          <p class="muted-2">{t("transcriptEditor.noModelsInstalled")}</p>
          <button class="btn" onclick={() => transcriptEditor.openModelManager()}>{t("transcriptEditor.openModelManagerButton")}</button>
          <button class="btn btn-ghost" onclick={() => void transcriptEditor.refreshModels()}>{t("transcriptEditor.refreshModelsButton")}</button>
        {:else}
          <div class="transcribe-form">
            <label class="field-label" for="te-model">{t("transcriptEditor.modelLabel")}</label>
            <select id="te-model" class="select" bind:value={transcriptEditor.selectedModelId}>
              {#each transcriptEditor.installedModels as model (model.id)}
                <option value={model.id}>{model.id}</option>
              {/each}
            </select>
            <label class="field-label" for="te-lang">{t("transcriptEditor.languageLabel")}</label>
            <input
              id="te-lang"
              class="select"
              type="text"
              placeholder={t("transcriptEditor.languagePlaceholder")}
              bind:value={transcriptEditor.language}
            />
            <button class="btn" onclick={() => void transcriptEditor.transcribeAnchorMedia()}>
              {t("transcriptEditor.transcribeButton")}
            </button>
          </div>
        {/if}
        {#if transcriptEditor.startError}
          <div class="sd-error">{transcriptEditor.startError}</div>
        {/if}
      </div>
    {:else}
      <div class="entries">
        {#each transcriptEditor.entries as entry (entry.id)}
          <TranscriptEntryRow {entry} />
        {/each}
      </div>

      {#if transcriptEditor.mode === "video"}
        <div class="video-edit-bar">
          <span class="staged-count muted-2">
            {t("transcriptEditor.stagedCount", { count: transcriptEditor.pendingTargets.length })}
          </span>
          <button
            class="btn btn-ghost"
            disabled={transcriptEditor.pendingTargets.length === 0}
            onclick={() => transcriptEditor.clearStaged()}
          >
            {t("transcriptEditor.clearStagedButton")}
          </button>
          <button
            class="btn btn-danger"
            disabled={transcriptEditor.pendingTargets.length === 0}
            onclick={() => transcriptEditor.openDeleteConfirm()}
          >
            {t("transcriptEditor.deleteSelectedButton")}
          </button>
        </div>

        {#if transcriptEditor.confirmingCuts}
          <div class="confirm-backdrop" role="presentation" onclick={() => transcriptEditor.cancelDeleteConfirm()}>
            <div
              class="confirm-dialog"
              role="dialog"
              aria-modal="true"
              aria-label={t("transcriptEditor.confirmTitle")}
              tabindex="-1"
              onclick={(e) => e.stopPropagation()}
              onkeydown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  transcriptEditor.cancelDeleteConfirm();
                }
              }}
            >
              <h3 class="confirm-title">{t("transcriptEditor.confirmTitle")}</h3>
              <p class="confirm-explainer muted-2">{t("transcriptEditor.confirmExplainer")}</p>
              <ul class="confirm-list">
                {#each transcriptEditor.confirmingCuts as cut (cut.id)}
                  <li>{formatTimecode(cut.start_us)} – {formatTimecode(cut.end_us)}</li>
                {/each}
              </ul>
              {#if transcriptEditor.applyError}
                <div class="sd-error">{transcriptEditor.applyError}</div>
              {/if}
              <div class="confirm-actions">
                <button class="btn btn-ghost" onclick={() => transcriptEditor.cancelDeleteConfirm()}>
                  {t("transcriptEditor.confirmCancelButton")}
                </button>
                <button class="btn btn-danger" disabled={transcriptEditor.applying} onclick={() => void transcriptEditor.confirmDeletion()}>
                  {transcriptEditor.applying ? t("transcriptEditor.confirmApplying") : t("transcriptEditor.confirmApplyButton")}
                </button>
              </div>
            </div>
          </div>
        {/if}
      {/if}
    {/if}
  </div>
</div>

<style>
  .panel {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 10px;
    gap: 6px;
    min-height: 0;
  }
  .mode-toggle {
    display: flex;
    gap: 4px;
  }
  .mode-btn {
    flex: 1;
    padding: 6px 8px;
    font-size: 11px;
    font-weight: 600;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--muted);
    cursor: pointer;
  }
  .mode-btn.active {
    background: hsl(210 90% 60% / 0.18);
    border-color: hsl(210 90% 60% / 0.5);
    color: var(--foreground);
  }
  .mode-btn-danger.active {
    background: hsl(0 84% 65% / 0.16);
    border-color: hsl(0 84% 65% / 0.5);
  }
  .mode-explainer {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.4;
  }
  .notice {
    font-size: 10.5px;
    color: var(--neg);
    padding: 4px 6px;
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
  }
  .empty {
    margin: 0;
    font-size: 11.5px;
  }
  .transcribe-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }
  .transcribe-form {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
  }
  .field-label {
    font-size: 10.5px;
    color: var(--muted);
  }
  .select {
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
    padding: 0 6px;
  }
  .progress-line {
    margin: 0;
    font-size: 11px;
  }
  .progress-line.error {
    color: var(--neg);
  }
  .entries {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .video-edit-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .staged-count {
    flex: 1;
    font-size: 10.5px;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.18);
    border: 1px solid hsl(0 84% 65% / 0.5);
    color: var(--foreground);
  }
  .sd-error {
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .confirm-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .confirm-dialog {
    width: min(420px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
  }
  .confirm-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .confirm-explainer {
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
  }
  .confirm-list {
    margin: 0;
    padding-left: 18px;
    font-size: 11.5px;
    max-height: 160px;
    overflow-y: auto;
  }
  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
