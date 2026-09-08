<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): Tab 1 ("Workspace")'s Simple-mode script/
  subtitle editor — promt.md §3.2/§28's "Original / Translation" two-column
  concept.

  **Placement decision (documented per the task brief's "your call, document
  it either way"):** this is a NEW, lighter view purpose-built for Tab 1,
  reading `stores/captions.svelte.ts`'s real `captions` array directly (the
  same single source of truth `CaptionsPanel.svelte`/`CaptionRow.svelte`
  already read from) rather than modifying either of those existing,
  tested components — full caption editing (split/merge/retime/style/find-
  replace) stays exactly where it already lived, in the Right Panel's
  Captions tab, reachable via Advanced mode. Tab 1's own version is
  intentionally read-mostly (matching promt.md §28's own simpler ASCII
  concept, which shows only `# | time | Original | Translation`, not the
  fuller §3.2 table's Speaker/Voice/Speed/Status/Actions columns — no real
  per-caption data source exists for any of those yet, so they are not
  fabricated here).

  **Translation column: wired for real**, backed by a real review/apply
  dialog (`STUDIO_PLAN.md` Phase D12) — not a session-only preview shown
  inline with no way to keep it. The toolbar's "Translate…" button opens
  `TranslationReviewDialog.svelte` (mounted once in `WorkspaceSimple.svelte`,
  the parent), which owns the real `translate_captions` call (Phase S6),
  the per-caption Accept/Reject review, and the real
  `apply_caption_translations` write-back (Phase D12's new backend command
  — the only place a translation proposal ever becomes real project text).
  This column shows `stores/translationReview.svelte.ts`'s own live
  `proposals` map directly (not a copy passed down as a prop) so it reflects
  the dialog's current state — including going back to "—" once a
  translation is actually applied, since at that point `Original` (below)
  already shows the exact same text for real.

  No new text-editing capability is added: `Original` is read-only, same as
  `CaptionRow.svelte`'s own `<p class="text">` — this codebase has no
  `set_caption_text` backend command for freehand text edits (only the
  translation-specific `apply_caption_translations` write path above).

  Phase D14 pass (`STUDIO_PLAN.md` "Recent projects"): the "no project open"
  `EmptyState` below gained a real action — an "Open Project…" button plus a
  Recent Projects list (`stores/recentProjects.svelte.ts`), reusing the exact
  same `recentProjectsStore.browseAndOpen`/`.open` methods `TopBar.svelte`'s
  File-menu "Open Project…"/"Recent" entries call. Chosen as this feature's
  second real home (alongside the File menu) because this is the actual
  screen a user with no project open sees first in Simple mode — the task
  brief's own "often most useful exactly where a user currently sees 'no
  project open'" reasoning.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { translationReviewStore } from "../../stores/translationReview.svelte";
  import { recentProjectsStore } from "../../stores/recentProjects.svelte";
  import type { Caption } from "../../types/bindings";
  import { formatTimecode } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import Panel from "../ui/Panel.svelte";
  import Button from "../ui/Button.svelte";
  import Tooltip from "../ui/Tooltip.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import DataTable from "../ui/DataTable.svelte";
  import Card from "../ui/Card.svelte";

  let sortedCaptions = $derived([...captionsStore.captions].sort((a, b) => a.start_us - b.start_us));
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

{#snippet indexCell(row: Caption)}
  <span class="mono muted-2">{sortedCaptions.indexOf(row) + 1}</span>
{/snippet}
{#snippet timeCell(row: Caption)}
  <span class="mono">{formatTimecode(row.start_us)} – {formatTimecode(row.end_us)}</span>
{/snippet}
{#snippet originalCell(row: Caption)}
  <span class="cell-text" title={row.text}>{row.text}</span>
{/snippet}
{#snippet translationCell(row: Caption)}
  {#if translationReviewStore.proposals[row.id]}
    <span class="cell-text" title={translationReviewStore.proposals[row.id]}>{translationReviewStore.proposals[row.id]}</span>
  {:else if translationReviewStore.translating}
    <span class="muted-2">{t("workspaceTab.script.translating")}</span>
  {:else}
    <span class="muted-2">—</span>
  {/if}
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
      <DataTable
        columns={[
          { key: "idx", label: "#", cell: indexCell },
          { key: "time", label: t("workspaceTab.script.colTime"), cell: timeCell },
          { key: "original", label: t("workspaceTab.script.colOriginal"), cell: originalCell },
          { key: "translation", label: t("workspaceTab.script.colTranslation"), cell: translationCell },
        ]}
        rows={sortedCaptions}
        rowKey={(c) => c.id}
        emptyMessage={t("workspaceTab.script.empty")}
      />

      {#if captionsStore.generateError}
        <ErrorState message={captionsStore.generateError} />
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
    {/if}
  </Panel>
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
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .hint {
    margin: 0;
    font-size: 10.5px;
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
