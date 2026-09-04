// Svelte 5 runes-based store for the Phase 7 Filler Word Detector panel
// (master prompt §16). Deliberately the same shape as
// `stores/silenceDetector.svelte.ts` (Analyze-equivalent -> Preview
// candidates -> Apply Cuts -> Reset against the same `Cut`-producing/
// `apply_silence_cuts`-consuming backend contract — see that file's own doc
// comment for the full rationale this mirrors) with one real difference the
// task brief calls out: candidates here are individually selectable
// (per-candidate checkbox + Select all/Deselect), since "filler word"
// candidates are named/textual in a way generic silence regions aren't —
// a user might reasonably want to keep one instance of "like" and remove
// another.
//
// Detection source: unlike Silence Detector (which runs a VAD model over raw
// media), `detect_filler_words` is pure/stateless over an existing
// `TranscriptEntry[]` (master prompt §16: "Detection must use transcript
// timestamps") — so this store's "Detect" step requires the selected clip's
// media to already have a transcript (produced by the Transcript Editor's
// Transcribe workflow, `stores/transcriptEditor.svelte.ts`), not a fresh
// model run of its own.

import { commands } from "../types/bindings";
import type { Clip, Cut, CutParams, FillerDictionary, MediaItem, Track, TranscriptEntry } from "../types/bindings";
import { timeline } from "./timeline.svelte";

export type ApplyMode = "clip" | "track";

/** Master prompt §16's exact defaults, mirrored client-side only for display
 * (the "what's included" reference list under the toggle) — matching rules
 * and actual detection stay entirely server-side in
 * `transcription::filler::{DEFAULT_EN_FILLERS, DEFAULT_VI_FILLERS}`; this
 * app has no direct binding to those constants, so this list is duplicated
 * by hand and must be kept in sync with that module if it ever changes. */
export const DEFAULT_EN_FILLERS = ["uh", "um", "erm", "you know", "like"];
export const DEFAULT_VI_FILLERS = ["ờ", "ừ", "ừm", "à", "ờm", "kiểu như"];

class FillerWordDetectorStore {
  open = $state(false);

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);
  applyMode = $state<ApplyMode>("clip");

  // ---- Dictionary (master prompt §16) --------------------------------------

  /** Single toggle, not a separate EN/VI pair: `FillerDictionary::use_defaults`
   * (`src-tauri/src/transcription/filler.rs`) is one bool that includes both
   * language lists together — there is no backend support for enabling only
   * one of the two defaults. Documented judgment call (the task brief's "EN/VI
   * default toggle" wording is satisfied here by showing both default lists
   * as a reference next to the one real toggle, rather than fabricating a
   * client-side-only per-language filter the backend can't actually honor). */
  useDefaults = $state(true);
  /** Newline/comma-separated (the panel's own input UX choice) — split into
   * `FillerDictionary.custom_dictionary` only at request time. */
  customDictionaryText = $state("");

  // ---- Padding (µs at the boundary, ms in this store — same convention
  // `silenceDetector.svelte.ts` already establishes) -------------------------

  paddingBeforeMs = $state(0);
  paddingAfterMs = $state(0);
  mergeGapMs = $state(0);

  // ---- Workflow results -----------------------------------------------------

  candidates = $state<Cut[]>([]);
  /** Per-candidate inclusion, keyed by `Cut.id`. Populated all-`true` right
   * after a successful Detect (task brief: candidates-first, user then
   * narrows down) — see `detect()`. */
  checked = $state<Record<string, boolean>>({});

  detecting = $state(false);
  applying = $state(false);
  lastError = $state<string | null>(null);
  appliedThisSession = $state(false);

  // ---- Derived selection context (same shape as `silenceDetector`'s) -------

  eligibleTracks = $derived.by((): Track[] => {
    return timeline.tracks.filter((t) => t.kind === "audio" || t.kind === "video");
  });

  clipsForSelectedTrack = $derived.by((): Clip[] => {
    if (!this.trackId) return [];
    return (timeline.clipsByTrack.get(this.trackId) ?? []).filter((c) => c.media_id !== null);
  });

  selectedClip = $derived.by((): Clip | null => {
    if (!this.clipId) return null;
    return this.clipsForSelectedTrack.find((c) => c.id === this.clipId) ?? null;
  });

  selectedMedia = $derived.by((): MediaItem | null => {
    const clip = this.selectedClip;
    if (!clip?.media_id) return null;
    return timeline.mediaById.get(clip.media_id) ?? null;
  });

  /** The transcript entries Detect actually runs against — empty (with a
   * "transcribe first" hint in the UI) when the selected media has none. */
  transcriptEntries = $derived.by((): TranscriptEntry[] => {
    const media = this.selectedMedia;
    if (!media) return [];
    return (timeline.project?.transcript ?? []).filter((e) => e.media_id === media.id);
  });

  dictionary = $derived<FillerDictionary>({
    use_defaults: this.useDefaults,
    custom_dictionary: this.customDictionaryText
      .split(/[,\n]/)
      .map((w) => w.trim())
      .filter((w) => w.length > 0),
  });

  cutParams = $derived<CutParams>({
    padding_before_us: Math.round(this.paddingBeforeMs * 1000),
    padding_after_us: Math.round(this.paddingAfterMs * 1000),
    merge_gap_us: Math.round(this.mergeGapMs * 1000),
  });

  checkedCuts = $derived<Cut[]>(this.candidates.filter((c) => this.checked[c.id] ?? false));

  canDetect = $derived(this.transcriptEntries.length > 0 && !this.detecting);
  canApply = $derived(
    this.checkedCuts.length > 0 &&
      !this.applying &&
      (this.applyMode === "clip" ? this.clipId !== null : this.trackId !== null),
  );

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openFor(opts: { trackId?: string; clipId?: string } = {}): void {
    this.resetResults();
    this.open = true;
    const tracks = this.eligibleTracks;
    this.trackId = opts.trackId ?? tracks[0]?.id ?? null;
    const clips = this.trackId ? (timeline.clipsByTrack.get(this.trackId) ?? []).filter((c) => c.media_id !== null) : [];
    this.clipId = opts.clipId ?? clips[0]?.id ?? null;
  }

  close(): void {
    this.open = false;
    this.resetResults();
  }

  private resetResults(): void {
    this.candidates = [];
    this.checked = {};
    this.lastError = null;
    this.appliedThisSession = false;
  }

  setTrack(trackId: string): void {
    if (trackId === this.trackId) return;
    this.trackId = trackId;
    const clips = (timeline.clipsByTrack.get(trackId) ?? []).filter((c) => c.media_id !== null);
    this.clipId = clips[0]?.id ?? null;
    this.resetResults();
  }

  setClip(clipId: string): void {
    if (clipId === this.clipId) return;
    this.clipId = clipId;
    this.resetResults();
  }

  /** The transcript entry a candidate `Cut` corresponds to (detection is
   * entry-level granularity server-side, per
   * `transcription::filler`'s module doc comment) — used by the panel to
   * show the actual matched sentence text next to each candidate, since
   * `Cut` itself carries only the time range, not any text. */
  entryForCut(cut: Cut): TranscriptEntry | null {
    const entries = this.transcriptEntries;
    return (
      entries.find((e) => cut.start_us >= e.start_us && cut.end_us <= e.end_us) ??
      entries.find((e) => cut.start_us < e.end_us && cut.end_us > e.start_us) ??
      null
    );
  }

  // -------------------------------------------------------------------
  // Workflow: Detect -> Select all/Deselect -> Preview (checked list itself
  // *is* the preview) -> Apply -> Reset
  // -------------------------------------------------------------------

  async detect(): Promise<void> {
    if (!this.canDetect) return;
    this.detecting = true;
    this.lastError = null;
    try {
      const cuts = await commands.detectFillerWords(this.transcriptEntries, this.dictionary, this.cutParams);
      this.candidates = cuts;
      const nextChecked: Record<string, boolean> = {};
      for (const cut of cuts) nextChecked[cut.id] = true;
      this.checked = nextChecked;
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.detecting = false;
    }
  }

  toggleCandidate(cutId: string): void {
    this.checked = { ...this.checked, [cutId]: !(this.checked[cutId] ?? false) };
  }

  selectAll(): void {
    const next: Record<string, boolean> = {};
    for (const cut of this.candidates) next[cut.id] = true;
    this.checked = next;
  }

  deselectAll(): void {
    const next: Record<string, boolean> = {};
    for (const cut of this.candidates) next[cut.id] = false;
    this.checked = next;
  }

  /** **Apply**: sends only the checked subset of candidates to the existing
   * (Phase 5) `apply_silence_cuts`/`apply_silence_cuts_to_track` commands —
   * one atomic undo step, identical contract to `silenceDetector.applyCuts`. */
  async applyCuts(): Promise<void> {
    if (!this.canApply) return;
    this.applying = true;
    this.lastError = null;
    try {
      const cuts = this.checkedCuts;
      const outcome =
        this.applyMode === "clip"
          ? await timeline.applyExternalProjectResult(commands.applySilenceCuts(this.clipId as string, cuts))
          : await timeline.applyExternalProjectResult(
              commands.applySilenceCutsToTrack(this.trackId as string, cuts),
            );
      if (outcome.ok) {
        this.appliedThisSession = true;
      } else {
        this.lastError = outcome.error;
      }
    } finally {
      this.applying = false;
    }
  }

  async reset(): Promise<void> {
    if (this.appliedThisSession) {
      await timeline.undo();
    }
    this.resetResults();
  }
}

export const fillerWordDetector = new FillerWordDetectorStore();
