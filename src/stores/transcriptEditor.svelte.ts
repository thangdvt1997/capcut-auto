// Svelte 5 runes-based store for the Phase 7 Transcript Editor (master
// prompt §15). Structurally the same "own transient workflow state,
// composes with `stores/timeline.svelte.ts` rather than living inside it"
// pattern `stores/silenceDetector.svelte.ts` established in Phase 5 — see
// that file's own doc comment for the rationale.
//
// Scope decision (task brief): the transcript shown is always "the
// transcript for the currently-selected timeline clip's underlying media" —
// there is no independent "which media is the transcript panel showing"
// picker; it just follows `stores/timeline.svelte.ts`'s own
// `selectedClipIds`, the same real shared selection state the rest of the
// app uses. This is why every seek/select action below reads/writes
// `timeline.playheadUs`/`timeline.selectedClipIds` directly rather than
// keeping a second, panel-local notion of "current position"/"current
// selection" — master prompt §15's own requirement ("synchronized
// transcript + timeline", "click word -> seek video", "select sentence ->
// select timeline range") is specifically that these stay the *same* state
// the main timeline/preview already use, not a parallel copy.
//
// Two modes (master prompt §15, "Clearly distinguish them"):
//   - "text": Transcript Text Edit. Editing an entry's text is a pure
//     correction affordance — `timeline.updateTranscriptEntryText` never
//     touches clips/tracks/cuts. Defaulted to on load (the non-destructive
//     choice) so a user never lands in a mode where an accidental click
//     could propose a cut.
//   - "video": Video Edit Through Transcript. Deletion is staged (per-word
//     or per-entry) into `pendingTargets`, then requires an explicit
//     `confirmDeletion()` step (a real UI confirm dialog, never triggered by
//     a mere keystroke/checkbox toggle) before any `Cut` is actually applied
//     via the existing `apply_silence_cuts` command — see that method's doc
//     comment for the full "never silently delete video when user edits
//     text" chain.

import { listen } from "@tauri-apps/api/event";
import { commands } from "../types/bindings";
import type { Clip, Cut, InstalledModel, MediaItem, TranscriptEntry, Word } from "../types/bindings";
import { timeline } from "./timeline.svelte";
// The concurrently-built Model Manager pass (`stores/modelManager.svelte.ts`)
// landed its own documented integration point — `openModelManager()` — while
// this pass was in progress, exactly as its own doc comment anticipated
// ("the Transcript Editor... can call this from a 'no model installed'
// prompt"). Calling it directly here rather than through an extra
// placeholder indirection layer.
import { openModelManager as openModelManagerDialog } from "./modelManager.svelte";

export type TranscriptEditMode = "text" | "video";

/**
 * Payload of the `transcription:progress` event
 * (`src-tauri/src/commands/transcription.rs::TranscriptionProgressEvent`).
 * Hand-written, not specta-generated, matching `stores/render.svelte.ts`'s
 * `RenderProgressEvent` precedent exactly (`tauri-specta`'s `Builder` only
 * registers *commands*, not typed events) — keep in sync with the Rust
 * struct by hand.
 */
export interface TranscriptionProgressEvent {
  job_id: string;
  media_id: string;
  percent: number | null;
  done: boolean;
  entries: TranscriptEntry[] | null;
  error: string | null;
}

const TRANSCRIPTION_PROGRESS_EVENT = "transcription:progress";

/** One staged deletion target in "Video Edit Through Transcript" mode: either
 * a whole entry (sentence) or a single word within one — word-level when the
 * data is available (this phase's whole point), entry-level as the fallback
 * granularity (also the only option when `words` is empty — see
 * `TranscriptEditor.svelte`). Carries its own `startUs`/`endUs` so building
 * the proposed `Cut`s never needs to re-derive them later. */
interface PendingTarget {
  key: string;
  entryId: string;
  wordIndex: number | null;
  startUs: number;
  endUs: number;
  label: string;
}

function entryTargetKey(entryId: string): string {
  return `entry:${entryId}`;
}
function wordTargetKey(entryId: string, wordIndex: number): string {
  return `word:${entryId}:${wordIndex}`;
}

class TranscriptEditorStore {
  mode = $state<TranscriptEditMode>("text");

  /** Per-entry local text buffers for "Transcript Text Edit" mode, keyed by
   * entry id. Only committed to `timeline.updateTranscriptEntryText` on
   * blur/explicit save, not on every keystroke (avoids a project-replacing
   * IPC round trip per character). Known limitation: an edit isn't committed
   * until its `<textarea>` actually blurs — switching the anchor clip (or
   * closing the panel) while mid-edit and never returning to blur that field
   * loses the uncommitted change, same tradeoff every "save on blur" text
   * field makes. Not fixed here (e.g. via a debounced auto-commit) since that
   * would reintroduce the per-keystroke IPC cost this design deliberately
   * avoids. */
  private editBuffers = $state<Record<string, string>>({});

  selectedEntryId = $state<string | null>(null);

  /** Set by `seekToWord`/`selectSentence` when the target time isn't covered
   * by any clip currently on the timeline (e.g. it was trimmed away) — surfaced
   * as a small inline note rather than a silent no-op. Cleared on the next
   * successful seek/select. */
  seekMissed = $state(false);

  // ---- "Video Edit Through Transcript" staged deletions ------------------

  pendingTargets = $state<PendingTarget[]>([]);
  /** Non-null while the confirm step (master prompt §15: never an implicit
   * auto-apply) is open — holds the exact merged `Cut`s the user is being
   * asked to confirm, computed once when the dialog opens so the preview and
   * the eventual apply call see identical data. */
  confirmingCuts = $state<Cut[] | null>(null);
  applying = $state(false);
  applyError = $state<string | null>(null);

  // ---- Transcribe workflow -------------------------------------------------

  installedModels = $state<InstalledModel[]>([]);
  modelsLoading = $state(false);
  modelsError = $state<string | null>(null);
  selectedModelId = $state<string | null>(null);
  language = $state<string>("");

  /** Keyed by `job_id`, mirroring `stores/render.svelte.ts`'s
   * `progressByJob` precedent (never assume only one job can ever exist). */
  progressByJob = $state<Record<string, TranscriptionProgressEvent>>({});
  activeJobId = $state<string | null>(null);
  starting = $state(false);
  startError = $state<string | null>(null);

  constructor() {
    void listen<TranscriptionProgressEvent>(TRANSCRIPTION_PROGRESS_EVENT, (event) => {
      this.progressByJob[event.payload.job_id] = event.payload;
      if (event.payload.done && event.payload.entries) {
        void timeline.replaceTranscriptForMedia(event.payload.media_id, event.payload.entries);
      }
    });
  }

  // -------------------------------------------------------------------
  // Anchor: the transcript always follows the timeline's own selection
  // -------------------------------------------------------------------

  anchorClip = $derived<Clip | null>(timeline.selectedClips[0] ?? null);

  anchorMedia = $derived.by((): MediaItem | null => {
    const clip = this.anchorClip;
    if (!clip?.media_id) return null;
    return timeline.mediaById.get(clip.media_id) ?? null;
  });

  entries = $derived.by((): TranscriptEntry[] => {
    const media = this.anchorMedia;
    if (!media) return [];
    return (timeline.project?.transcript ?? [])
      .filter((e) => e.media_id === media.id)
      .sort((a, b) => a.start_us - b.start_us);
  });

  hasTranscript = $derived(this.entries.length > 0);

  progress = $derived(this.activeJobId ? (this.progressByJob[this.activeJobId] ?? null) : null);
  transcribing = $derived(this.activeJobId !== null && !(this.progress?.done ?? false));

  // -------------------------------------------------------------------
  // Mode
  // -------------------------------------------------------------------

  setMode(next: TranscriptEditMode): void {
    if (this.mode === next) return;
    this.mode = next;
    // Switching away from Video Edit discards any staged-but-unconfirmed
    // deletions rather than leaving them silently pending in a mode where
    // they're no longer visible.
    this.pendingTargets = [];
    this.confirmingCuts = null;
  }

  // -------------------------------------------------------------------
  // Timeline sync: click word -> seek, select sentence -> select range
  // -------------------------------------------------------------------

  /** Resolves a source-media timestamp to a real timeline position: prefers
   * the anchor clip itself when its trimmed source range covers `sourceUs`,
   * else the first other on-timeline clip using the same media that does.
   * Returns `null` when no clip currently covers that moment (e.g. it was
   * trimmed off every instance) — callers surface that via `seekMissed`
   * rather than silently doing nothing unexplained. */
  private resolveTimelinePosition(sourceUs: number): { clip: Clip; playheadUs: number } | null {
    const media = this.anchorMedia;
    if (!media) return null;
    const candidates = timeline.clips.filter((c) => c.media_id === media.id);
    const anchor = this.anchorClip;
    const ordered = anchor ? [anchor, ...candidates.filter((c) => c.id !== anchor.id)] : candidates;
    for (const clip of ordered) {
      if (sourceUs >= clip.source_in_us && sourceUs < clip.source_out_us) {
        const speed = clip.speed > 0 ? clip.speed : 1;
        const playheadUs = clip.position_us + Math.round((sourceUs - clip.source_in_us) / speed);
        return { clip, playheadUs };
      }
    }
    return null;
  }

  /** Master prompt §15: "Click word -> seek video". Drives the exact same
   * `timeline.playheadUs` that `VideoPlayer.svelte`'s `activeVideoTarget`
   * already follows — no separate seek path. */
  seekToWord(word: Word): void {
    const resolved = this.resolveTimelinePosition(word.start_us);
    this.seekMissed = resolved === null;
    if (resolved) timeline.setPlayhead(resolved.playheadUs);
  }

  /** Master prompt §15: "Select sentence -> select timeline range". This
   * app's selection model is clip-based (see `stores/timeline.svelte.ts`),
   * not a sub-clip time-range selection — there is no such concept anywhere
   * else in the app to hang a "range" off. The closest faithful mapping onto
   * *real* shared state: select the on-timeline clip the sentence actually
   * lives in (so `ClipView.svelte` highlights it exactly like any other
   * selection) and move the playhead to the sentence's start (so the preview
   * scrubs to the start of that "range"). Documented judgment call, not a
   * literal sub-clip range highlight the codebase has no primitive for. */
  selectSentence(entry: TranscriptEntry): void {
    const resolved = this.resolveTimelinePosition(entry.start_us);
    this.seekMissed = resolved === null;
    this.selectedEntryId = entry.id;
    if (resolved) {
      timeline.selectClip(resolved.clip.id);
      timeline.setPlayhead(resolved.playheadUs);
    }
  }

  // -------------------------------------------------------------------
  // Transcript Text Edit mode
  // -------------------------------------------------------------------

  textBufferFor(entry: TranscriptEntry): string {
    return this.editBuffers[entry.id] ?? entry.text;
  }

  setTextBuffer(entryId: string, value: string): void {
    this.editBuffers[entryId] = value;
  }

  /** Commits one entry's edited text (blur/explicit save) — pure text
   * correction, never a timeline mutation (see class doc comment). */
  async commitText(entry: TranscriptEntry): Promise<void> {
    const buffered = this.editBuffers[entry.id];
    if (buffered === undefined || buffered === entry.text) return;
    await timeline.updateTranscriptEntryText(entry.id, buffered);
  }

  // -------------------------------------------------------------------
  // Video Edit Through Transcript mode: stage -> confirm -> apply
  // -------------------------------------------------------------------

  isEntryStaged(entryId: string): boolean {
    return this.pendingTargets.some((t) => t.key === entryTargetKey(entryId));
  }

  isWordStaged(entryId: string, wordIndex: number): boolean {
    const key = wordTargetKey(entryId, wordIndex);
    return this.pendingTargets.some((t) => t.key === key || t.key === entryTargetKey(entryId));
  }

  toggleEntryStaged(entry: TranscriptEntry): void {
    const key = entryTargetKey(entry.id);
    if (this.pendingTargets.some((t) => t.key === key)) {
      this.pendingTargets = this.pendingTargets.filter((t) => t.key !== key);
      return;
    }
    // Staging the whole entry supersedes any individually-staged words
    // within it (avoids double-counting the same span twice).
    const withoutWords = this.pendingTargets.filter((t) => t.entryId !== entry.id);
    withoutWords.push({
      key,
      entryId: entry.id,
      wordIndex: null,
      startUs: entry.start_us,
      endUs: entry.end_us,
      label: entry.text,
    });
    this.pendingTargets = withoutWords;
  }

  toggleWordStaged(entry: TranscriptEntry, wordIndex: number): void {
    const word = entry.words[wordIndex];
    if (!word) return;
    // No-op while the whole entry is already staged — clear that first so
    // per-word intent is unambiguous.
    if (this.isEntryStaged(entry.id)) return;
    const key = wordTargetKey(entry.id, wordIndex);
    if (this.pendingTargets.some((t) => t.key === key)) {
      this.pendingTargets = this.pendingTargets.filter((t) => t.key !== key);
      return;
    }
    this.pendingTargets = [
      ...this.pendingTargets,
      { key, entryId: entry.id, wordIndex, startUs: word.start_us, endUs: word.end_us, label: word.text },
    ];
  }

  clearStaged(): void {
    this.pendingTargets = [];
  }

  /** Sorted, overlap/adjacency-merged `Cut`s built from every staged target —
   * shown in the confirm step and, unmodified, sent to `apply_silence_cuts`.
   * Merging avoids handing the backend two overlapping "remove" spans for
   * the common case of several staged words within one sentence. */
  buildProposedCuts(): Cut[] {
    const media = this.anchorMedia;
    if (!media || this.pendingTargets.length === 0) return [];
    const sorted = [...this.pendingTargets].sort((a, b) => a.startUs - b.startUs);
    const merged: { startUs: number; endUs: number }[] = [];
    for (const target of sorted) {
      const last = merged[merged.length - 1];
      if (last && target.startUs <= last.endUs) {
        last.endUs = Math.max(last.endUs, target.endUs);
      } else {
        merged.push({ startUs: target.startUs, endUs: target.endUs });
      }
    }
    return merged.map((span) => ({
      id: crypto.randomUUID(),
      kind: "remove" as const,
      source_media_id: media.id,
      start_us: span.startUs,
      end_us: span.endUs,
      // `CutReason` (`src-tauri/src/project/types.rs`) has exactly three
      // variants — silence / filler_word / ai_suggested — and none of them
      // literally means "a user directly proposed this via the transcript
      // editor". This is a frontend-only pass that cannot add a fourth
      // backend variant; `ai_suggested` is the closest existing bucket
      // (a suggestion the user is explicitly confirming, same as this
      // panel's own confirm step), not a claim this was AI-generated.
      // Documented here rather than silently mislabeled.
      reason: "ai_suggested" as const,
      applied: false,
    }));
  }

  /** Opens the confirm step (master prompt §15: "Never silently delete video
   * when user edits text") — never called implicitly from a checkbox toggle
   * or keystroke, only from an explicit "Delete Selected" button. */
  openDeleteConfirm(): void {
    const cuts = this.buildProposedCuts();
    if (cuts.length === 0) return;
    this.confirmingCuts = cuts;
  }

  cancelDeleteConfirm(): void {
    this.confirmingCuts = null;
  }

  /** The explicit confirm action itself: applies the previously-computed
   * `confirmingCuts` to the anchor clip via the existing (Phase 5)
   * `apply_silence_cuts` command — same atomic-undo-step guarantee as the
   * Silence Detector / Filler Word Detector both already rely on. */
  async confirmDeletion(): Promise<void> {
    const clip = this.anchorClip;
    const cuts = this.confirmingCuts;
    if (!clip || !cuts || cuts.length === 0 || this.applying) return;
    this.applying = true;
    this.applyError = null;
    try {
      const outcome = await timeline.applyExternalProjectResult(commands.applySilenceCuts(clip.id, cuts));
      if (outcome.ok) {
        this.pendingTargets = [];
        this.confirmingCuts = null;
      } else {
        this.applyError = outcome.error;
      }
    } finally {
      this.applying = false;
    }
  }

  // -------------------------------------------------------------------
  // Transcribe workflow (master prompt §14/§60 integration)
  // -------------------------------------------------------------------

  async ensureModelsLoaded(): Promise<void> {
    if (this.installedModels.length > 0 || this.modelsLoading) return;
    await this.refreshModels();
  }

  /** Unconditional (re)load — used by `ensureModelsLoaded`'s first call, and
   * by the panel's own "Refresh" affordance after the user installs a model
   * via the Model Manager dialog and returns here (that dialog is a separate
   * component/store, so there is no shared reactive link telling this store
   * a download just finished — a manual refresh is the honest option rather
   * than pretending to auto-sync with a store this pass doesn't own). */
  async refreshModels(): Promise<void> {
    if (this.modelsLoading) return;
    this.modelsLoading = true;
    this.modelsError = null;
    try {
      const result = await commands.listInstalledModels();
      if (result.status === "ok") {
        this.installedModels = result.data;
        if (!this.selectedModelId) this.selectedModelId = result.data[0]?.id ?? null;
      } else {
        this.modelsError = result.error.message;
      }
    } finally {
      this.modelsLoading = false;
    }
  }

  /** No model installed: hand off to the real Model Manager dialog
   * (`stores/modelManager.svelte.ts`). */
  openModelManager(): void {
    openModelManagerDialog();
  }

  async transcribeAnchorMedia(): Promise<void> {
    const media = this.anchorMedia;
    if (!media || !this.selectedModelId || this.starting || this.transcribing) return;
    this.starting = true;
    this.startError = null;
    try {
      const result = await commands.transcribeMedia(
        media.id,
        media.source_path,
        this.selectedModelId,
        this.language.trim() === "" ? null : this.language.trim(),
      );
      if (result.status === "ok") {
        this.activeJobId = result.data;
        this.progressByJob[result.data] = {
          job_id: result.data,
          media_id: media.id,
          percent: 0,
          done: false,
          entries: null,
          error: null,
        };
      } else {
        this.startError = result.error.message;
      }
    } finally {
      this.starting = false;
    }
  }

  async cancelTranscription(): Promise<void> {
    if (!this.activeJobId) return;
    const result = await commands.cancelTranscription(this.activeJobId);
    if (result.status === "error") this.startError = result.error.message;
  }

  /** Clears a finished job's progress so `TranscriptEditor.svelte` can offer
   * "Transcribe" again (e.g. after an error, or to re-transcribe). */
  dismissJob(): void {
    this.activeJobId = null;
    this.startError = null;
  }
}

export const transcriptEditor = new TranscriptEditorStore();
