<!--
  One transcript sentence/segment row, rendered by `TranscriptEditor.svelte`.
  Behavior forks on `transcriptEditor.mode` (master prompt §15's two
  "clearly distinguished" modes):

    - "text" (Transcript Text Edit): the entry's text is a plain editable
      textarea — a correction affordance only, never a timeline mutation
      (`transcriptEditor.commitText`).
    - "video" (Video Edit Through Transcript): text is read-only, rendered as
      individually clickable word spans (word-level when `entry.words` is
      populated — this phase's own addition — falling back to the whole
      entry as one span when it isn't). Each word/entry also gets its own
      stage-for-deletion control; nothing here ever applies a cut by itself —
      that only happens through `TranscriptEditor.svelte`'s explicit confirm
      step.

  The leading timestamp button is the "select sentence -> select timeline
  range" affordance (master prompt §15) in both modes — clicking *a word*
  seeks, clicking *the timestamp* selects the whole sentence/range.
-->
<script lang="ts">
  import { transcriptEditor } from "../../stores/transcriptEditor.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import type { TranscriptEntry } from "../../types/bindings";

  let { entry }: { entry: TranscriptEntry } = $props();

  let mode = $derived(transcriptEditor.mode);
  let staged = $derived(transcriptEditor.isEntryStaged(entry.id));
  let selected = $derived(transcriptEditor.selectedEntryId === entry.id);

  function onTextInput(e: Event): void {
    transcriptEditor.setTextBuffer(entry.id, (e.currentTarget as HTMLTextAreaElement).value);
  }
</script>

<div class="row" class:selected>
  <div class="row-gutter">
    {#if mode === "video"}
      <input
        type="checkbox"
        class="entry-checkbox"
        checked={staged}
        title={t("transcriptEditor.stageEntryTooltip")}
        onchange={() => transcriptEditor.toggleEntryStaged(entry)}
      />
    {/if}
    <button
      class="timestamp"
      onclick={() => transcriptEditor.selectSentence(entry)}
      title={t("transcriptEditor.selectSentenceTooltip")}
    >
      {formatTimecode(entry.start_us)}
    </button>
  </div>

  <div class="row-body" class:staged>
    {#if mode === "text"}
      <textarea
        class="text-edit"
        rows="1"
        value={transcriptEditor.textBufferFor(entry)}
        oninput={onTextInput}
        onblur={() => void transcriptEditor.commitText(entry)}
      ></textarea>
    {:else if entry.words.length > 0}
      <p class="words">
        {#each entry.words as word, i (i)}
          {@const wordStaged = transcriptEditor.isWordStaged(entry.id, i)}
          <span class="word-pill" class:staged={wordStaged}>
            <button
              class="word"
              class:low-confidence={word.confidence < 0.5}
              onclick={() => transcriptEditor.seekToWord(word)}
              title={t("transcriptEditor.seekWordTooltip")}
            >{word.text}</button
            ><button
              class="word-del"
              disabled={staged}
              onclick={() => transcriptEditor.toggleWordStaged(entry, i)}
              title={t("transcriptEditor.stageWordTooltip")}
            >×</button>
          </span>
        {/each}
      </p>
    {:else}
      <button class="fallback-text" onclick={() => transcriptEditor.selectSentence(entry)} title={t("transcriptEditor.noWordDataTooltip")}>
        {entry.text}
      </button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
  }
  .row.selected {
    background: hsl(210 90% 60% / 0.08);
    border-color: hsl(210 90% 60% / 0.35);
  }
  .row-gutter {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    padding-top: 2px;
  }
  .timestamp {
    background: none;
    border: none;
    color: var(--muted);
    font-size: 10.5px;
    font-family: var(--font-mono, monospace);
    cursor: pointer;
    padding: 1px 4px;
    border-radius: var(--radius-sm);
  }
  .timestamp:hover {
    background: var(--surface-2);
    color: var(--foreground);
  }
  .row-body {
    flex: 1;
    min-width: 0;
  }
  .row-body.staged {
    opacity: 0.55;
  }
  .text-edit {
    width: 100%;
    resize: vertical;
    min-height: 30px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 12.5px;
    line-height: 1.5;
    padding: 4px 6px;
  }
  .words {
    margin: 0;
    line-height: 1.9;
    font-size: 12.5px;
  }
  .word-pill {
    display: inline-flex;
    align-items: center;
    border-radius: var(--radius-sm);
  }
  .word-pill.staged .word {
    text-decoration: line-through;
    color: var(--neg);
  }
  .word {
    background: none;
    border: none;
    color: var(--foreground);
    cursor: pointer;
    padding: 0 1px;
    font-size: 12.5px;
  }
  .word:hover {
    background: hsl(210 90% 60% / 0.15);
    border-radius: 2px;
  }
  .word.low-confidence {
    border-bottom: 1px dotted var(--neg);
  }
  .word-del {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 10px;
    padding: 0 2px;
    visibility: hidden;
  }
  .word-del:disabled {
    cursor: not-allowed;
  }
  .word-pill:hover .word-del {
    visibility: visible;
  }
  .word-pill.staged .word-del {
    visibility: visible;
    color: var(--neg);
  }
  .fallback-text {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    color: var(--foreground);
    font-size: 12.5px;
    line-height: 1.6;
    cursor: pointer;
    padding: 0;
  }
  .entry-checkbox {
    accent-color: var(--neg);
  }
</style>
