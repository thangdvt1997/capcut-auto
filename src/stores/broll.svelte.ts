// Svelte 5 runes-based store for the Phase 11 B-Roll panel (master prompt
// §34, `src-tauri/src/broll/` / `src-tauri/src/commands/broll.rs`).
// Anchored to the timeline's own selection exactly like
// `stores/transcriptEditor.svelte.ts` (mounted as a section inside
// `TranscriptEditor.svelte` itself, since suggestions are transcript-driven
// and that panel already owns "the selected clip's transcript" — see
// `IMPLEMENTATION_PLAN.md` Phase 11 notes for the placement rationale).
//
// Workflow: "Suggest B-Roll" runs the combined `suggest_and_search_broll`
// pipeline (AI suggestion -> real local-library keyword search) against the
// anchor media's own transcript; each result pairs a suggestion
// (keyword/reason/time range) with whatever real local candidates were
// found for it — possibly none, an honest "no local B-roll found" outcome
// per that command's own doc comment, never hidden by this store.
//
// "Add to timeline": genuinely achievable by reusing `MediaLibrary.svelte`'s
// exact "re-probe by path, then `addMediaAsClip`" bridge — a `BRollCandidate`
// doesn't carry every field `MediaItem` needs (fps/codec/bitrate/etc), so
// this re-probes the file via `probe_media_file` rather than fabricating
// them. Appended onto a dedicated `overlay`-kind track (kept separate from
// the main video track), trimmed to whichever is shorter of the candidate's
// own duration or the suggestion's requested duration — there is no signal
// for *where inside* a longer candidate file the relevant footage actually
// is, so this always starts at the candidate's own `0`, documented as an
// honest limitation.

import { commands } from "../types/bindings";
import type { BRollCandidate, BRollSuggestionWithCandidatesPayload, Clip, MediaItem, TranscriptEntry } from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { aiSettingsStore, currentAiProviderSettings, keyRequirementFor } from "./aiSettings.svelte";

class BRollStore {
  suggesting = $state(false);
  lastError = $state<string | null>(null);
  results = $state<BRollSuggestionWithCandidatesPayload[]>([]);

  addingKey = $state<string | null>(null);
  addedKeys = $state<Set<string>>(new Set());
  addError = $state<string | null>(null);

  // ---- Anchor: follows the timeline's own selection, same shape as
  // `stores/transcriptEditor.svelte.ts` -----------------------------------

  anchorClip = $derived<Clip | null>(timeline.selectedClips[0] ?? null);

  anchorMedia = $derived.by((): MediaItem | null => {
    const clip = this.anchorClip;
    if (!clip?.media_id) return null;
    return timeline.mediaById.get(clip.media_id) ?? null;
  });

  transcriptEntries = $derived.by((): TranscriptEntry[] => {
    const media = this.anchorMedia;
    if (!media) return [];
    return (timeline.project?.transcript ?? [])
      .filter((e) => e.media_id === media.id)
      .sort((a, b) => a.start_us - b.start_us);
  });

  /** Reactive snapshot of the AI Settings dialog's current provider
   * settings — same `stores/aiSettings.svelte.ts` dependency
   * `stores/highlightDetection.svelte.ts`/`stores/smartEdit.svelte.ts`
   * already use. */
  aiSettings = $derived(currentAiProviderSettings());
  aiConfigured = $derived(
    aiSettingsStore.model.trim().length > 0 &&
      (keyRequirementFor(aiSettingsStore.provider) !== "required" || aiSettingsStore.hasKeyConfigured),
  );

  canSuggest = $derived(this.transcriptEntries.length > 0 && this.aiConfigured && !this.suggesting);

  reset(): void {
    this.results = [];
    this.lastError = null;
    this.addingKey = null;
    this.addedKeys = new Set();
    this.addError = null;
  }

  // -------------------------------------------------------------------
  // Suggest (+ search)
  // -------------------------------------------------------------------

  async suggest(): Promise<void> {
    const media = this.anchorMedia;
    if (!media || !this.canSuggest) return;
    this.suggesting = true;
    this.lastError = null;
    try {
      const result = await commands.suggestAndSearchBroll(
        this.aiSettings,
        this.transcriptEntries,
        media.duration_us,
        null,
      );
      if (result.status === "ok") {
        this.results = result.data;
      } else {
        this.results = [];
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.suggesting = false;
    }
  }

  // -------------------------------------------------------------------
  // Add to timeline (see class doc comment)
  // -------------------------------------------------------------------

  private keyFor(suggestionId: string, candidate: BRollCandidate): string {
    return `${suggestionId}:${candidate.media_id}`;
  }

  isAdded(suggestionId: string, candidate: BRollCandidate): boolean {
    return this.addedKeys.has(this.keyFor(suggestionId, candidate));
  }

  isAdding(suggestionId: string, candidate: BRollCandidate): boolean {
    return this.addingKey === this.keyFor(suggestionId, candidate);
  }

  async addToTimeline(suggestionId: string, requestedDurationUs: number, candidate: BRollCandidate): Promise<void> {
    const key = this.keyFor(suggestionId, candidate);
    if (this.addingKey !== null) return;
    this.addingKey = key;
    this.addError = null;
    try {
      const probed = await commands.probeMediaFile(candidate.path);
      if (probed.status !== "ok") {
        this.addError = probed.error.message;
        return;
      }
      const sourceOutUs = Math.min(candidate.duration_us, requestedDurationUs > 0 ? requestedDurationUs : candidate.duration_us);
      await timeline.addMediaAsClip(probed.data, { trackKind: "overlay", sourceInUs: 0, sourceOutUs });
      this.addedKeys = new Set(this.addedKeys).add(key);
    } catch (err) {
      this.addError = String(err);
    } finally {
      this.addingKey = null;
    }
  }
}

export const brollStore = new BRollStore();
