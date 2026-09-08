<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): Tab 1 ("Workspace")'s Simple-mode script/
  subtitle editor — promt.md §3.2/§28's own concept.

  **Phase D18 rewrite** (`STUDIO_PLAN.md` Phase D18, `promt.md` §3.2's full
  table): this used to be a read-mostly `# | Time | Original | Translation`
  view. It is now the real, richer per-row editor §3.2 actually asks for —
  multi-select, delete, split/merge/duplicate a row, per-row re-translate,
  per-row regenerate/preview voice, and an auto-fit-voice-to-duration
  action — built directly on real backend primitives added this same phase
  (`timeline::captions::duplicate_caption`/`delete_captions`) plus real,
  already-existing ones (`split_caption`/`merge_captions`/
  `translate_captions`/`synthesize_speech`). Still reads
  `stores/captions.svelte.ts`'s real `captions` array directly — one real
  source of truth, shared with `CaptionsPanel.svelte`/`CaptionRow.svelte` in
  Advanced mode, which stays the place for full retime-by-drag/style/find-
  replace editing.

  ## Undo/redo

  Every mutating action here (`split`/`merge`/`duplicate`/`delete`/
  translation-apply) goes through `captionsStore`/`translationReviewStore`
  methods that all end in `timeline.applyExternalProjectResult(...)` — the
  exact same real project-mutation + undo-history path `TimelineSession`
  already provides project-wide (`Ctrl+Z`/`Ctrl+Shift+Z`, wired in
  `stores/timeline.svelte.ts`, work here with zero new machinery: this view
  never invents a second undo stack for Simple mode).

  ## Three real, honestly-scoped gaps carried forward from this same phase
  (documented in full in `STUDIO_PLAN.md`'s own Phase D18 section, repeated
  here briefly since they directly shape this file):

  1. **No per-caption `speaker`/`role` field exists** (`project::Caption`
     has no such field — confirmed by reading the struct before assuming
     one). The "Speaker" column below is therefore a *session-only* Voice-
     Mapping-role picker (`voiceRoleByCaption`, keyed by caption id, never
     persisted, never sent to the backend as part of the caption) — not a
     fabricated caption property.
  2. **Auto-fit voice is a real, working speed-multiplier retry**, not just
     a mismatch indicator: after a row's voice finishes generating, its real
     `duration_us` (`VoiceSynthesisOutput`) is compared against the
     caption's own `end_us - start_us`; outside a 5% tolerance, "Auto-fit"
     computes `speed *= actual/expected` (clamped to `[0.5, 2.0]`, since a
     TTS engine's own real speed range is finite) and re-synthesizes with
     that adjusted `VoiceSynthesisSettings.speed` — a real correction, not
     merely a printed warning.
  3. **Re-translate/regenerate-voice are per-row, not per-project.** Re-
     translate reuses `translationReviewStore.retranslateOne()` (new this
     phase — a one-caption `translate_captions` call, landing in the exact
     same `proposals` map the big `TranslationReviewDialog` uses, so this
     row's Translation cell and that dialog never disagree). Voice
     generation reuses the existing singleton `stores/voiceSynthesis.svelte.ts`
     job store (Phase D16) — only one row's voice can be generating at a
     time, the same real "one job at a time" limit that store already
     documents; a second row's Generate click while another is in flight is
     a no-op until the first finishes (buttons disable accordingly).

  **Import SRT — a real, separate, out-of-scope gap, not fabricated here**:
  confirmed via grep (`srt`/`SubRip`) that no SRT-parsing code exists
  anywhere in this codebase, frontend or backend. Building a parser is a
  meaningfully separate, sizeable feature (file format parsing + a real
  import command), not something to bolt onto this same pass — matching
  Phase D2+D3's own precedent of leaving an unbacked action out entirely
  rather than adding a decorative disabled button for it.
-->
<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { captionsStore } from "../../stores/captions.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { translationReviewStore } from "../../stores/translationReview.svelte";
  import { recentProjectsStore } from "../../stores/recentProjects.svelte";
  import { voiceSettingsStore } from "../../stores/voiceSettings.svelte";
  import { voiceSynthesisStore } from "../../stores/voiceSynthesis.svelte";
  import type { Caption } from "../../types/bindings";
  import { formatTimecode, usToSec } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import Panel from "../ui/Panel.svelte";
  import Button from "../ui/Button.svelte";
  import Tooltip from "../ui/Tooltip.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Card from "../ui/Card.svelte";
  import Checkbox from "../ui/Checkbox.svelte";
  import Select from "../ui/Select.svelte";
  import Badge from "../ui/Badge.svelte";

  const AUTO_FIT_TOLERANCE = 0.05;
  const MIN_SPEED = 0.5;
  const MAX_SPEED = 2.0;

  let sortedCaptions = $derived([...captionsStore.captions].sort((a, b) => a.start_us - b.start_us));
  let allSelected = $derived(
    sortedCaptions.length > 0 && sortedCaptions.every((c) => captionsStore.selectedCaptionIds.has(c.id)),
  );

  // ---- Per-row voice generation (session-only — see doc comment #1/#2) ----
  let voiceRoleByCaption = $state<Record<string, string>>({});
  let voiceSpeedByCaption = $state<Record<string, number>>({});
  let voiceOutputByCaption = $state<Record<string, { path: string; durationUs: number | null }>>({});
  let activeVoiceCaptionId = $state<string | null>(null);
  let previewCaptionId = $state<string | null>(null);
  let previewAudioEl: HTMLAudioElement | undefined = $state();

  // ---- Two-step delete confirm (matches `AssetLibraryDialog.svelte`'s
  // arm/cancel/confirm precedent for destructive actions) ----
  let armedDeleteId = $state<string | null>(null);
  let bulkDeleteArmed = $state(false);

  const roleOptions = $derived(
    voiceSettingsStore.roleMappings.map((m) => ({ value: m.role, label: `${m.role} (${m.voiceName})` })),
  );

  function roleFor(captionId: string): string {
    return voiceRoleByCaption[captionId] ?? voiceSettingsStore.roleMappings[0]?.role ?? "";
  }

  function speedFor(captionId: string): number {
    return voiceSpeedByCaption[captionId] ?? 1;
  }

  function durationSec(row: Caption): number {
    return usToSec(Math.max(0, row.end_us - row.start_us));
  }

  /** Real audio duration ÷ real caption duration — `null` until a row has
   * actually generated voice with a provider-reported duration. */
  function mismatchRatio(row: Caption): number | null {
    const output = voiceOutputByCaption[row.id];
    if (!output || output.durationUs == null) return null;
    const captionDur = durationSec(row);
    if (captionDur <= 0) return null;
    return usToSec(output.durationUs) / captionDur;
  }

  function isMismatched(row: Caption): boolean {
    const ratio = mismatchRatio(row);
    return ratio != null && Math.abs(ratio - 1) > AUTO_FIT_TOLERANCE;
  }

  function nextCaption(row: Caption): Caption | null {
    const idx = sortedCaptions.indexOf(row);
    return idx >= 0 ? (sortedCaptions[idx + 1] ?? null) : null;
  }

  function canMergeWithNext(row: Caption): boolean {
    const next = nextCaption(row);
    return next !== null && next.track_id === row.track_id;
  }

  function playheadInside(row: Caption): boolean {
    return timeline.playheadUs >= row.start_us && timeline.playheadUs < row.end_us;
  }

  function canSplit(row: Caption): boolean {
    return playheadInside(row) && row.words.length > 0 && captionsStore.busyCaptionId !== row.id;
  }

  async function generateVoiceForRow(row: Caption): Promise<void> {
    if (voiceSynthesisStore.starting || voiceSynthesisStore.isRunning) return;
    const mapping = voiceSettingsStore.roleMappings.find((m) => m.role === roleFor(row.id));
    if (!mapping) return;
    activeVoiceCaptionId = row.id;
    voiceSynthesisStore.reset();
    await voiceSynthesisStore.generate(row.text, mapping.voiceId, voiceSettingsStore.settingsSnapshot(), {
      speed: speedFor(row.id),
      pitch: 1,
      volume: 1,
      emotion: null,
      language: null,
    });
  }

  /** Real speed-multiplier correction (doc comment #2) — computes a new
   * `speed` from the last generation's actual/expected duration ratio and
   * re-synthesizes with it. */
  async function autoFitVoiceForRow(row: Caption): Promise<void> {
    const ratio = mismatchRatio(row);
    if (ratio == null) return;
    const next = Math.min(MAX_SPEED, Math.max(MIN_SPEED, speedFor(row.id) * ratio));
    voiceSpeedByCaption = { ...voiceSpeedByCaption, [row.id]: next };
    await generateVoiceForRow(row);
  }

  function previewVoiceForRow(row: Caption): void {
    const output = voiceOutputByCaption[row.id];
    if (!output || !previewAudioEl) return;
    previewCaptionId = row.id;
    previewAudioEl.src = convertFileSrc(output.path);
    void previewAudioEl.play();
  }

  // Captures the singleton voice job's terminal outcome into whichever row
  // actually started it (`activeVoiceCaptionId`) — the same "job_id known,
  // no terminal event yet = running" honesty model
  // `stores/voiceSynthesis.svelte.ts` already documents; this effect never
  // fabricates a percentage, it only records a real, already-arrived result.
  $effect(() => {
    const progress = voiceSynthesisStore.progress;
    const target = activeVoiceCaptionId;
    if (!progress || !progress.done || !target || progress.cancelled || !progress.output_path) return;
    const existing = voiceOutputByCaption[target];
    if (existing?.path === progress.output_path) return;
    voiceOutputByCaption = {
      ...voiceOutputByCaption,
      [target]: { path: progress.output_path, durationUs: progress.duration_us },
    };
  });

  function statusFor(row: Caption): { text: string; variant: "neutral" | "pos" | "neg" | "warn" | "accent" } {
    if (translationReviewStore.rowTranslating.has(row.id)) {
      return { text: t("workspaceTab.script.statusTranslating"), variant: "accent" };
    }
    if (translationReviewStore.rowTranslateError[row.id]) {
      return { text: t("workspaceTab.script.statusError"), variant: "neg" };
    }
    if (translationReviewStore.proposals[row.id] !== undefined) {
      return { text: t("workspaceTab.script.statusTranslationPending"), variant: "warn" };
    }
    if (activeVoiceCaptionId === row.id && voiceSynthesisStore.isRunning) {
      return { text: t("workspaceTab.script.statusGeneratingVoice"), variant: "accent" };
    }
    if (voiceOutputByCaption[row.id]) {
      return isMismatched(row)
        ? { text: t("workspaceTab.script.statusVoiceMismatch"), variant: "warn" }
        : { text: t("workspaceTab.script.statusVoiceReady"), variant: "pos" };
    }
    return { text: t("workspaceTab.script.statusIdle"), variant: "neutral" };
  }

  function toggleSelectAll(): void {
    if (allSelected) {
      captionsStore.clearCaptionSelection();
    } else {
      captionsStore.selectedCaptionIds = new Set(sortedCaptions.map((c) => c.id));
    }
  }
</script>

{#snippet noProjectAction()}
  <div class="no-project-action">
    <Button variant="primary" size="sm" onclick={() => void recentProjectsStore.browseAndOpen()}>
      {t("workspaceTab.script.openProjectButton")}
    </Button>
    {#if recentProjectsStore.entries.length > 0}
      <div class="recent-projects">
        <p class="recent-projects-label muted-2">{t("topBar.recentProjectsLabel")}</p>
        {#each recentProjectsStore.entries as entry (entry.path)}
          <Card interactive padding="sm" onclick={() => void recentProjectsStore.open(entry.path)}>
            <div class="recent-row">
              <span class="recent-name">{entry.name}</span>
              <span class="recent-path muted-2" title={entry.path}>{entry.path}</span>
            </div>
          </Card>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet selectCell(row: Caption)}
  <Checkbox
    checked={captionsStore.selectedCaptionIds.has(row.id)}
    onchange={() => captionsStore.toggleCaptionSelected(row.id, true)}
  />
{/snippet}
{#snippet indexCell(row: Caption)}
  <span class="mono idx-cell" class:idx-cell-active={captionsStore.activeCaptionId === row.id}>
    {sortedCaptions.indexOf(row) + 1}
  </span>
{/snippet}
{#snippet startCell(row: Caption)}
  <button
    class="link-btn mono"
    title={t("captionsPanel.jumpToCaption")}
    onclick={() => captionsStore.seekToCaption(row)}
  >
    {formatTimecode(row.start_us)}
  </button>
{/snippet}
{#snippet endCell(row: Caption)}
  <span class="mono">{formatTimecode(row.end_us)}</span>
{/snippet}
{#snippet durationCell(row: Caption)}
  <span class="mono">{durationSec(row).toFixed(2)}s</span>
{/snippet}
{#snippet speakerCell(row: Caption)}
  {#if roleOptions.length === 0}
    <span class="muted-2 hint-inline">{t("workspaceTab.script.noRoleMappingsPlaceholder")}</span>
  {:else}
    <Select
      value={roleFor(row.id)}
      options={roleOptions}
      onchange={(v) => (voiceRoleByCaption = { ...voiceRoleByCaption, [row.id]: v })}
    />
  {/if}
{/snippet}
{#snippet originalCell(row: Caption)}
  <span class="cell-text" title={row.text}>{row.text}</span>
{/snippet}
{#snippet translationCell(row: Caption)}
  {#if translationReviewStore.proposals[row.id] !== undefined}
    <span class="cell-text" title={translationReviewStore.proposals[row.id]}>{translationReviewStore.proposals[row.id]}</span>
  {:else if translationReviewStore.rowTranslating.has(row.id) || translationReviewStore.translating}
    <span class="muted-2">{t("workspaceTab.script.translating")}</span>
  {:else if translationReviewStore.rowTranslateError[row.id]}
    <span class="cell-text err-text" title={translationReviewStore.rowTranslateError[row.id]}>{translationReviewStore.rowTranslateError[row.id]}</span>
  {:else}
    <span class="muted-2">—</span>
  {/if}
{/snippet}
{#snippet voiceCell(row: Caption)}
  {#if activeVoiceCaptionId === row.id && voiceSynthesisStore.isRunning}
    <span class="muted-2">{t("workspaceTab.script.statusGeneratingVoice")}</span>
  {:else if activeVoiceCaptionId === row.id && voiceSynthesisStore.startError}
    <span class="cell-text err-text" title={voiceSynthesisStore.startError}>{voiceSynthesisStore.startError}</span>
  {:else if voiceOutputByCaption[row.id]}
    {@const output = voiceOutputByCaption[row.id]!}
    <span class="mono">
      {output.durationUs != null ? `${usToSec(output.durationUs).toFixed(2)}s` : t("workspaceTab.script.voiceDurationUnknown")}
    </span>
    {#if isMismatched(row)}
      <Badge variant="warn">{t("workspaceTab.script.statusVoiceMismatch")}</Badge>
    {/if}
  {:else}
    <span class="muted-2">—</span>
  {/if}
{/snippet}
{#snippet speedCell(row: Caption)}
  <span class="mono">{speedFor(row.id).toFixed(2)}x</span>
{/snippet}
{#snippet statusCell(row: Caption)}
  {@const status = statusFor(row)}
  <Badge variant={status.variant}>{status.text}</Badge>
{/snippet}
{#snippet actionsCell(row: Caption)}
  <div class="row-actions">
    <Button
      size="sm"
      variant="ghost"
      disabled={!canSplit(row)}
      title={t("workspaceTab.script.splitTooltip")}
      onclick={() => void captionsStore.splitAtPlayhead(row)}
    >
      {t("workspaceTab.script.splitButton")}
    </Button>
    <Button
      size="sm"
      variant="ghost"
      disabled={!canMergeWithNext(row)}
      title={t("workspaceTab.script.mergeNextTooltip")}
      onclick={() => {
        const next = nextCaption(row);
        if (next) void captionsStore.mergeCaptionsByIds([row.id, next.id]);
      }}
    >
      {t("workspaceTab.script.mergeNextButton")}
    </Button>
    <Button
      size="sm"
      variant="ghost"
      disabled={captionsStore.busyCaptionId === row.id}
      onclick={() => void captionsStore.duplicateCaption(row)}
    >
      {t("workspaceTab.script.duplicateButton")}
    </Button>

    {#if armedDeleteId === row.id}
      <Button
        size="sm"
        variant="danger"
        onclick={() => {
          armedDeleteId = null;
          void captionsStore.deleteCaption(row.id);
        }}
      >
        {t("workspaceTab.script.deleteConfirmButton")}
      </Button>
      <Button size="sm" variant="ghost" onclick={() => (armedDeleteId = null)}>
        {t("workspaceTab.script.deleteCancelButton")}
      </Button>
    {:else}
      <Button size="sm" variant="danger" onclick={() => (armedDeleteId = row.id)}>
        {t("workspaceTab.script.deleteButton")}
      </Button>
    {/if}

    {#if translationReviewStore.proposals[row.id] !== undefined}
      <Button size="sm" variant="primary" onclick={() => void translationReviewStore.applyOne(row.id)}>
        {t("workspaceTab.script.acceptButton")}
      </Button>
      <Button size="sm" variant="ghost" onclick={() => translationReviewStore.discardOne(row.id)}>
        {t("workspaceTab.script.rejectButton")}
      </Button>
    {:else}
      <Button
        size="sm"
        variant="ghost"
        disabled={translationReviewStore.rowTranslating.has(row.id) || !translationReviewStore.aiConfigured || translationReviewStore.targetLanguage.trim() === ""}
        title={t("workspaceTab.script.retranslateTooltip")}
        onclick={() => void translationReviewStore.retranslateOne(row)}
      >
        {t("workspaceTab.script.retranslateButton")}
      </Button>
    {/if}

    <Button
      size="sm"
      variant="ghost"
      disabled={roleOptions.length === 0 || voiceSynthesisStore.starting || (voiceSynthesisStore.isRunning && activeVoiceCaptionId !== row.id)}
      title={roleOptions.length === 0 ? t("workspaceTab.script.noRoleMappingsPlaceholder") : t("workspaceTab.script.generateVoiceRowTooltip")}
      onclick={() => void generateVoiceForRow(row)}
    >
      {t("workspaceTab.script.generateVoiceRowButton")}
    </Button>
    <Button size="sm" variant="ghost" disabled={!voiceOutputByCaption[row.id]} onclick={() => previewVoiceForRow(row)}>
      {t("workspaceTab.script.previewVoiceButton")}
    </Button>
    {#if isMismatched(row)}
      <Button size="sm" variant="ghost" onclick={() => void autoFitVoiceForRow(row)}>
        {t("workspaceTab.script.autoFitButton")}
      </Button>
    {/if}
  </div>
{/snippet}

<div class="script-editor">
  <Panel title={t("workspaceTab.script.title")}>
    {#if !timeline.project}
      <EmptyState
        title={t("workspaceTab.script.noProjectTitle")}
        description={t("workspaceTab.script.noProjectDesc")}
        action={noProjectAction}
      />
    {:else}
      {#if captionsStore.selectedCaptionIds.size > 0}
        <div class="bulk-toolbar">
          <span class="muted-2">{t("workspaceTab.script.selectedCount", { count: captionsStore.selectedCaptionIds.size })}</span>
          <Button
            size="sm"
            variant="ghost"
            disabled={captionsStore.selectedCaptionIds.size < 2}
            onclick={() => void captionsStore.mergeSelected()}
          >
            {t("workspaceTab.script.mergeSelectedButton")}
          </Button>
          {#if bulkDeleteArmed}
            <Button
              size="sm"
              variant="danger"
              onclick={() => {
                bulkDeleteArmed = false;
                void captionsStore.deleteSelected();
              }}
            >
              {t("workspaceTab.script.deleteConfirmButton")}
            </Button>
            <Button size="sm" variant="ghost" onclick={() => (bulkDeleteArmed = false)}>
              {t("workspaceTab.script.deleteCancelButton")}
            </Button>
          {:else}
            <Button size="sm" variant="danger" onclick={() => (bulkDeleteArmed = true)}>
              {t("workspaceTab.script.bulkDeleteButton")}
            </Button>
          {/if}
          <Button size="sm" variant="ghost" onclick={() => captionsStore.clearCaptionSelection()}>
            {t("workspaceTab.script.clearSelectionButton")}
          </Button>
        </div>
      {/if}

      <DataTable
        columns={[
          { key: "select", label: "", cell: selectCell },
          { key: "idx", label: "#", cell: indexCell },
          { key: "start", label: t("workspaceTab.script.colStart"), cell: startCell },
          { key: "end", label: t("workspaceTab.script.colEnd"), cell: endCell },
          { key: "duration", label: t("workspaceTab.script.colDuration"), cell: durationCell },
          { key: "speaker", label: t("workspaceTab.script.colSpeaker"), cell: speakerCell },
          { key: "original", label: t("workspaceTab.script.colOriginal"), cell: originalCell },
          { key: "translation", label: t("workspaceTab.script.colTranslation"), cell: translationCell },
          { key: "voice", label: t("workspaceTab.script.colVoice"), cell: voiceCell },
          { key: "speed", label: t("workspaceTab.script.colSpeed"), cell: speedCell },
          { key: "status", label: t("workspaceTab.script.colStatus"), cell: statusCell },
          { key: "actions", label: t("workspaceTab.script.colActions"), cell: actionsCell },
        ]}
        rows={sortedCaptions}
        rowKey={(c) => c.id}
        emptyMessage={t("workspaceTab.script.empty")}
      />
      <p class="select-all-hint muted-2">
        <button class="link-btn" onclick={() => toggleSelectAll()}>
          {allSelected ? t("workspaceTab.script.deselectAllButton") : t("workspaceTab.script.selectAllButton")}
        </button>
      </p>

      {#if captionsStore.generateError}
        <ErrorState message={captionsStore.generateError} />
      {/if}
      {#if captionsStore.correctionError}
        <ErrorState message={captionsStore.correctionError} />
      {/if}
      {#if translationReviewStore.applyError}
        <ErrorState message={translationReviewStore.applyError} />
      {/if}

      <div class="toolbar">
        <Button
          size="sm"
          disabled={captionsStore.generating || !captionsStore.hasTranscript || !captionsStore.effectiveGenTrackId}
          onclick={() => void captionsStore.generate()}
        >
          {captionsStore.generating ? t("workspaceTab.script.generatingButton") : t("workspaceTab.script.generateButton")}
        </Button>

        <Button size="sm" disabled={sortedCaptions.length === 0} onclick={() => translationReviewStore.openDialog()}>
          {t("workspaceTab.script.translateButton")}
        </Button>

        <Tooltip text={t("workspaceTab.script.voiceNotWiredTooltip")}>
          <Button size="sm" disabled>{t("workspaceTab.script.generateVoiceButton")}</Button>
        </Tooltip>

        <Tooltip text={t("workspaceTab.script.syncNotWiredTooltip")}>
          <Button size="sm" disabled>{t("workspaceTab.script.syncTimelineButton")}</Button>
        </Tooltip>
      </div>

      {#if !captionsStore.hasTranscript}
        <p class="hint muted-2">{t("workspaceTab.script.noTranscriptHint")}</p>
      {/if}
      {#if roleOptions.length === 0}
        <p class="hint muted-2">{t("workspaceTab.script.noRoleMappingsHint")}</p>
      {/if}
    {/if}
  </Panel>
</div>

<!-- One shared, hidden-by-default audio element for "Preview Voice"
     (`STUDIO_PLAN.md` Phase D18) — a plain HTML5 `<audio>` element served
     through Tauri's `asset:` protocol via `convertFileSrc`, the same
     real-file-playback mechanism `VideoPlayer.svelte` already uses for
     video. `controls` so the user can pause/scrub/replay the currently
     previewed line without re-clicking "Preview". -->

<div class="preview-player" class:preview-player-hidden={!previewCaptionId}>
  <span class="muted-2">
    {t("workspaceTab.script.nowPreviewing")}
    {#if previewCaptionId}
      — “{sortedCaptions.find((c) => c.id === previewCaptionId)?.text ?? ""}”
    {/if}
  </span>
  <audio bind:this={previewAudioEl} controls></audio>
</div>

<style>
  .script-editor {
    height: 100%;
    min-height: 0;
    overflow-y: auto;
    padding: 0 0 0 var(--space-3);
  }
  .cell-text {
    display: block;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err-text {
    color: var(--neg);
  }
  .idx-cell {
    display: inline-block;
    min-width: 1.4em;
    color: var(--muted);
  }
  .idx-cell-active {
    color: var(--accent);
    font-weight: 700;
  }
  .hint-inline {
    font-size: 10.5px;
    white-space: nowrap;
  }
  .row-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    min-width: 260px;
  }
  .bulk-toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) 0;
  }
  .select-all-hint {
    margin: var(--space-1) 0 0;
  }
  .link-btn {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font-size: 10.5px;
    cursor: pointer;
    text-decoration: underline;
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }
  .hint {
    margin: 0;
    font-size: 10.5px;
  }
  .preview-player {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--border);
  }
  .preview-player-hidden {
    display: none;
  }
  .no-project-action {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
  }
  .recent-projects {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    width: 100%;
    max-width: 420px;
  }
  .recent-projects-label {
    margin: 0;
    font-size: 10.5px;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    text-align: left;
  }
  .recent-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    text-align: left;
    min-width: 0;
  }
  .recent-name {
    font-size: 12px;
    font-weight: 600;
  }
  .recent-path {
    font-size: 10.5px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
