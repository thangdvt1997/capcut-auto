<!--
  Start Batch dialog (master prompt §42's own worked example: "100 videos ->
  Remove silence -> Generate captions -> Apply template -> Export"). Nested
  on top of `BatchJobsDialog.svelte` (opened from its own "Start New Batch…"
  button) rather than a separate TopBar entry point, since a batch is only
  ever meaningful in the context of the Jobs table it lands in.

  Multi-file picker mirrors `MediaLibrary.svelte`'s established
  `open({multiple: true, filters: [...]})` pattern exactly (same
  `@tauri-apps/plugin-dialog` native picker, video/audio extensions only —
  batch stages are transcription/silence-removal/render, none of which
  operate on a still image). The pipeline-config form's stage toggles/params
  mirror `stores/silenceDetector.svelte.ts` (ms-in-UI, `CutParams` is
  µs-native) and `stores/captions.svelte.ts` (max words/chars per line +
  grouping mode) exactly, so the same settings read the same way whether
  reached from Batch or from their own dedicated panels. Template/export
  preset pickers are sourced directly from the real `list_templates`/
  `list_render_presets` commands (no separate Templates-browser UI exists
  yet to defer to, per this pass's own task brief).
-->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { batchStore } from "../../stores/batch.svelte";
  import { commands } from "../../types/bindings";
  import { t } from "../../lib/i18n.svelte";
  import type {
    AvailableModel,
    BatchPipelineConfig,
    CaptionGroupingMode,
    RenderPreset,
    Template,
  } from "../../types/bindings";

  const MEDIA_EXTENSIONS = ["mp4", "mov", "mkv", "avi", "webm", "m4v", "mp3", "wav", "aac", "m4a", "flac"];

  let selectedPaths = $state<string[]>([]);

  let removeSilenceEnabled = $state(false);
  let paddingBeforeMs = $state(0);
  let paddingAfterMs = $state(0);
  let mergeGapMs = $state(0);

  let captionsEnabled = $state(false);
  let transcriptionModelId = $state<string | null>(null);
  let transcriptionLanguage = $state("");
  let maxWordsPerLine = $state(6);
  let maxCharsPerLine = $state(30);
  let grouping = $state<CaptionGroupingMode>("sentence");

  let templateId = $state<string | null>(null);
  let exportPresetId = $state<string | null>(null);

  let installedModels = $state<AvailableModel[]>([]);
  let templates = $state<Template[]>([]);
  let presets = $state<RenderPreset[]>([]);
  let catalogsLoading = $state(false);
  let catalogsError = $state<string | null>(null);
  let catalogsLoaded = false;

  /** Loaded lazily on first dialog open, matching `renderStore.openDialog()`'s
   * precedent — cheap catalogs, no need to reload on every reopen. */
  async function ensureCatalogsLoaded(): Promise<void> {
    if (catalogsLoaded || catalogsLoading) return;
    catalogsLoading = true;
    catalogsError = null;
    try {
      const [modelsResult, templatesResult, presetList] = await Promise.all([
        commands.listAvailableModels(),
        commands.listTemplates(),
        commands.listRenderPresets(),
      ]);
      if (modelsResult.status === "ok") {
        installedModels = modelsResult.data.filter((m) => m.installed);
      }
      if (templatesResult.status === "ok") {
        templates = [...templatesResult.data.built_in, ...templatesResult.data.custom];
      } else {
        catalogsError = templatesResult.error.message;
      }
      presets = presetList;
      const [firstPreset] = presets;
      if (firstPreset && exportPresetId === null) {
        const default1080 = presets.find((p) => p.id === "p1080");
        exportPresetId = (default1080 ?? firstPreset).id;
      }
      catalogsLoaded = true;
    } finally {
      catalogsLoading = false;
    }
  }

  $effect(() => {
    if (batchStore.startDialogOpen) void ensureCatalogsLoaded();
  });

  async function pickFiles(): Promise<void> {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Media", extensions: MEDIA_EXTENSIONS }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    // Dedupe — picking the same folder's files twice shouldn't queue two
    // identical jobs.
    const merged = [...selectedPaths];
    for (const p of paths) if (!merged.includes(p)) merged.push(p);
    selectedPaths = merged;
  }

  function removePath(path: string): void {
    selectedPaths = selectedPaths.filter((p) => p !== path);
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  const canStart = $derived(
    selectedPaths.length > 0 &&
      !batchStore.starting &&
      (templateId !== null || exportPresetId !== null) &&
      (!captionsEnabled || transcriptionModelId !== null),
  );

  function buildConfig(): BatchPipelineConfig {
    return {
      remove_silence: removeSilenceEnabled
        ? {
            padding_before_us: Math.round(paddingBeforeMs * 1000),
            padding_after_us: Math.round(paddingAfterMs * 1000),
            merge_gap_us: Math.round(mergeGapMs * 1000),
          }
        : null,
      captions: captionsEnabled
        ? {
            max_words_per_line: Math.max(1, Math.round(maxWordsPerLine)),
            max_chars_per_line: Math.max(1, Math.round(maxCharsPerLine)),
            grouping,
          }
        : null,
      transcription_model_id: captionsEnabled ? transcriptionModelId : null,
      transcription_language: captionsEnabled && transcriptionLanguage.trim() ? transcriptionLanguage.trim() : null,
      template_id: templateId,
      export_preset_id: exportPresetId,
    };
  }

  async function onStart(): Promise<void> {
    if (!canStart) return;
    await batchStore.startBatch(selectedPaths, buildConfig());
    // Only clear the picked files on success — `startBatch` leaves
    // `startDialogOpen` open (and its own `startError` set) on failure so
    // the user doesn't have to re-pick everything after a transient error.
    if (!batchStore.startDialogOpen) {
      selectedPaths = [];
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      batchStore.closeStartDialog();
    }
  }
</script>

{#if batchStore.startDialogOpen}
  <div class="sb-backdrop" role="presentation" onclick={() => batchStore.closeStartDialog()}>
    <div
      class="sb-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("startBatchDialog.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="sb-header">
        <span class="sb-title">{t("startBatchDialog.title")}</span>
        <button class="btn btn-ghost" onclick={() => batchStore.closeStartDialog()} title={t("startBatchDialog.close")}>×</button>
      </div>

      <div class="sb-body">
        <section class="sb-section">
          <h3 class="sb-section-title">{t("startBatchDialog.filesSectionTitle")}</h3>
          <div class="sb-row">
            <button class="btn" onclick={() => void pickFiles()}>{t("startBatchDialog.pickFilesButton")}</button>
            <span class="muted-2">{t("startBatchDialog.fileCount", { count: selectedPaths.length })}</span>
          </div>
          {#if selectedPaths.length > 0}
            <ul class="sb-file-list">
              {#each selectedPaths as path (path)}
                <li class="sb-file-item">
                  <span class="sb-file-name" title={path}>{basename(path)}</span>
                  <button class="btn btn-ghost sb-file-remove" onclick={() => removePath(path)} title={t("startBatchDialog.removeFile")}>×</button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="sb-empty muted-2">{t("startBatchDialog.noFiles")}</p>
          {/if}
        </section>

        <section class="sb-section">
          <h3 class="sb-section-title">{t("startBatchDialog.silenceSectionTitle")}</h3>
          <label class="sb-checkbox">
            <input type="checkbox" bind:checked={removeSilenceEnabled} />
            {t("startBatchDialog.silenceEnable")}
          </label>
          {#if removeSilenceEnabled}
            <div class="sb-row">
              <label class="sb-label" for="sb-pad-before">{t("startBatchDialog.paddingBefore")}</label>
              <input id="sb-pad-before" class="sb-number" type="number" min="0" bind:value={paddingBeforeMs} /> ms
            </div>
            <div class="sb-row">
              <label class="sb-label" for="sb-pad-after">{t("startBatchDialog.paddingAfter")}</label>
              <input id="sb-pad-after" class="sb-number" type="number" min="0" bind:value={paddingAfterMs} /> ms
            </div>
            <div class="sb-row">
              <label class="sb-label" for="sb-merge-gap">{t("startBatchDialog.mergeGap")}</label>
              <input id="sb-merge-gap" class="sb-number" type="number" min="0" bind:value={mergeGapMs} /> ms
            </div>
          {/if}
        </section>

        <section class="sb-section">
          <h3 class="sb-section-title">{t("startBatchDialog.captionsSectionTitle")}</h3>
          <label class="sb-checkbox">
            <input type="checkbox" bind:checked={captionsEnabled} />
            {t("startBatchDialog.captionsEnable")}
          </label>
          {#if captionsEnabled}
            <div class="sb-row">
              <label class="sb-label" for="sb-model">{t("startBatchDialog.transcriptionModel")}</label>
              <select
                id="sb-model"
                class="sb-select"
                value={transcriptionModelId ?? ""}
                onchange={(e) => (transcriptionModelId = (e.target as HTMLSelectElement).value || null)}
              >
                <option value="" disabled>{t("startBatchDialog.selectModel")}</option>
                {#each installedModels as m (m.entry.id)}
                  <option value={m.entry.id}>{m.entry.display_name}</option>
                {/each}
              </select>
            </div>
            {#if installedModels.length === 0 && !catalogsLoading}
              <p class="sb-hint muted-2">{t("startBatchDialog.noModelsInstalled")}</p>
            {/if}
            <div class="sb-row">
              <label class="sb-label" for="sb-language">{t("startBatchDialog.language")}</label>
              <input id="sb-language" class="sb-select" type="text" placeholder={t("startBatchDialog.languageAuto")} bind:value={transcriptionLanguage} />
            </div>
            <div class="sb-row">
              <label class="sb-label" for="sb-max-words">{t("startBatchDialog.maxWordsPerLine")}</label>
              <input id="sb-max-words" class="sb-number" type="number" min="1" bind:value={maxWordsPerLine} />
            </div>
            <div class="sb-row">
              <label class="sb-label" for="sb-max-chars">{t("startBatchDialog.maxCharsPerLine")}</label>
              <input id="sb-max-chars" class="sb-number" type="number" min="1" bind:value={maxCharsPerLine} />
            </div>
            <div class="sb-row">
              <span class="sb-label">{t("startBatchDialog.grouping")}</span>
              <div class="sb-radio-group">
                <label class="sb-radio">
                  <input type="radio" name="sb-grouping" checked={grouping === "sentence"} onchange={() => (grouping = "sentence")} />
                  {t("startBatchDialog.groupingSentence")}
                </label>
                <label class="sb-radio">
                  <input type="radio" name="sb-grouping" checked={grouping === "word"} onchange={() => (grouping = "word")} />
                  {t("startBatchDialog.groupingWord")}
                </label>
              </div>
            </div>
          {/if}
        </section>

        <section class="sb-section">
          <h3 class="sb-section-title">{t("startBatchDialog.templateSectionTitle")}</h3>
          <div class="sb-row">
            <label class="sb-label" for="sb-template">{t("startBatchDialog.template")}</label>
            <select
              id="sb-template"
              class="sb-select"
              value={templateId ?? ""}
              onchange={(e) => (templateId = (e.target as HTMLSelectElement).value || null)}
            >
              <option value="">{t("startBatchDialog.noTemplate")}</option>
              {#each templates as tpl (tpl.id)}
                <option value={tpl.id}>{tpl.name}</option>
              {/each}
            </select>
          </div>
        </section>

        <section class="sb-section">
          <h3 class="sb-section-title">{t("startBatchDialog.exportSectionTitle")}</h3>
          <div class="sb-row">
            <label class="sb-label" for="sb-preset">{t("startBatchDialog.exportPreset")}</label>
            <select
              id="sb-preset"
              class="sb-select"
              value={exportPresetId ?? ""}
              onchange={(e) => (exportPresetId = (e.target as HTMLSelectElement).value || null)}
            >
              {#if templateId !== null}
                <option value="">{t("startBatchDialog.useTemplateDefaultPreset")}</option>
              {/if}
              {#each presets as p (p.id)}
                <option value={p.id}>{p.name}</option>
              {/each}
            </select>
          </div>
        </section>

        {#if catalogsError}
          <div class="sb-error">{catalogsError}</div>
        {/if}
        {#if batchStore.startError}
          <div class="sb-error">{t("startBatchDialog.startFailed", { error: batchStore.startError })}</div>
        {/if}
      </div>

      <div class="sb-footer">
        <button class="btn" disabled={!canStart} onclick={() => void onStart()}>
          {batchStore.starting ? t("startBatchDialog.starting") : t("startBatchDialog.startButton")}
        </button>
        <span class="sb-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => batchStore.closeStartDialog()}>{t("startBatchDialog.cancelButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .sb-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 110;
  }
  .sb-dialog {
    width: min(640px, 94vw);
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
  .sb-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sb-title {
    font-size: 13px;
    font-weight: 600;
  }
  .sb-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .sb-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .sb-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sb-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .sb-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
    min-width: 140px;
  }
  .sb-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
    padding: 0 6px;
  }
  .sb-number {
    width: 90px;
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .sb-checkbox {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    cursor: pointer;
    width: fit-content;
  }
  .sb-radio-group { display: flex; gap: 14px; }
  .sb-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .sb-file-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 140px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .sb-file-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 8px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .sb-file-name {
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .sb-file-remove {
    flex-shrink: 0;
    padding: 0 6px;
    line-height: 1;
  }
  .sb-empty { margin: 0; font-size: 11.5px; }
  .sb-hint { margin: 0; font-size: 10.5px; }
  .sb-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .sb-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .sb-footer-spacer { flex: 1; }
</style>
