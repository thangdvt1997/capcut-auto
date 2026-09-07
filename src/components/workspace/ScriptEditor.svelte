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

  **Translation column: wired for real**, not deferred — `ai::
  translate_captions` (Phase S6, already real on the backend) is a natural
  fit here: `WorkspaceSimple.svelte` (the parent) calls it with this
  store's real `captions` + `aiSettingsStore`'s real provider settings, and
  hands the resulting `Record<caption_id, translated_text>` map down as
  `translations` — a session-only proposal (translate_captions "never
  mutates project.captions itself", so nothing here claims to persist a
  translation, only to show one). If this hadn't fit cleanly in the time
  available, a single-column Original-only editor would have been an
  equally honest first version; it did fit, so it's built.

  No new text-editing capability is added: `Original` is read-only, same as
  `CaptionRow.svelte`'s own `<p class="text">` — this codebase has no
  `set_caption_text` backend command for either component to call.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import type { Caption } from "../../types/bindings";
  import { formatTimecode } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import Panel from "../ui/Panel.svelte";
  import Button from "../ui/Button.svelte";
  import Input from "../ui/Input.svelte";
  import Tooltip from "../ui/Tooltip.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import DataTable from "../ui/DataTable.svelte";

  let {
    targetLanguage = $bindable("vi"),
    translating,
    translateError,
    translations,
    onTranslate,
  }: {
    targetLanguage?: string;
    translating: boolean;
    translateError: string | null;
    translations: Record<string, string>;
    onTranslate: () => void;
  } = $props();

  let sortedCaptions = $derived([...captionsStore.captions].sort((a, b) => a.start_us - b.start_us));
</script>

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
  {#if translations[row.id]}
    <span class="cell-text" title={translations[row.id]}>{translations[row.id]}</span>
  {:else if translating}
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
      {#if translateError}
        <ErrorState message={translateError} />
      {/if}

      <div class="toolbar">
        <Button
          size="sm"
          disabled={captionsStore.generating || !captionsStore.hasTranscript || !captionsStore.effectiveGenTrackId}
          onclick={() => void captionsStore.generate()}
        >
          {captionsStore.generating ? t("workspaceTab.script.generatingButton") : t("workspaceTab.script.generateButton")}
        </Button>

        <div class="lang-field">
          <Input bind:value={targetLanguage} placeholder={t("workspaceTab.script.targetLanguagePlaceholder")} />
        </div>

        <Button size="sm" disabled={translating || sortedCaptions.length === 0} onclick={onTranslate}>
          {translating ? t("workspaceTab.script.translatingButton") : t("workspaceTab.script.translateButton")}
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
  .lang-field {
    width: 150px;
  }
  .hint {
    margin: 0;
    font-size: 10.5px;
  }
</style>
