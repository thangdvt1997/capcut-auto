// Svelte 5 runes-based store for the Phase 5 Silence Detector panel (master
// prompt §12/§13). Composes with `stores/timeline.svelte.ts` rather than
// living inside it (see `IMPLEMENTATION_PLAN.md` Phase 5 notes) — this is a
// distinct, self-contained workflow (Analyze -> Preview Cuts -> Apply Cuts ->
// Reset) with its own transient state (VAD scores, proposed cuts, loading
// flags per step), not part of the core timeline model.
//
// Unit convention: every slider lives in this store as milliseconds (or a
// 0-100 percent for the threshold) — i64 microseconds never leaks into a
// user-facing label (task brief). Conversion to/from `VadParams`/`CutParams`
// (both µs-native, per `docs/project-format.md`'s mandated timebase) happens
// only in the `vadParams`/`cutParams` derived getters below and nowhere else.

import { commands } from "../types/bindings";
import type { Clip, Cut, CutParams, MediaItem, SpeechSegment, Track, VadParams, VadScoreSummary } from "../types/bindings";
import { timeline } from "./timeline.svelte";

export type ApplyMode = "clip" | "track";

class SilenceDetectorStore {
  open = $state(false);

  /** The audio-bearing track the user is working from. Populated from the
   * project's tracks when the panel opens; see `eligibleTracks` below for
   * which track kinds are offered. */
  trackId = $state<string | null>(null);

  /** The specific clip whose underlying media gets analyzed (`Analyze`
   * scores one `MediaItem`, keyed by its id — a track can hold clips from
   * several different source files, so the clip picker is how the user
   * disambiguates which one to run VAD against). */
  clipId = $state<string | null>(null);

  /** Whether `Apply Cuts` targets just `clipId` or every clip currently on
   * `trackId` (master prompt §12's "analysis track selection" applied at
   * Apply time — `commands::timeline::apply_silence_cuts_to_track`). */
  applyMode = $state<ApplyMode>("clip");

  // ---- Parameters (master prompt §12), UI units -----------------------

  /** 0-100, maps to `VadParams.threshold` (0-1). */
  thresholdPct = $state(50);
  minSilenceMs = $state(100);
  minSpeechMs = $state(150);
  paddingBeforeMs = $state(0);
  paddingAfterMs = $state(0);
  mergeGapMs = $state(0);

  /**
   * "Audio channel selection" (master prompt §12). `audio::pcm::extract_pcm`
   * (the PCM extraction `score_media_silence` calls) hardcodes `-ac 1`
   * (downmix to mono) with no channel argument at all — verified by reading
   * `src-tauri/src/audio/pcm.rs` and `src-tauri/src/commands/vad.rs` for
   * this pass. Per this task's brief, that's real DSP-adjacent backend work
   * out of scope for a frontend pass, so this control is intentionally
   * inert: always "mono (downmix)", disabled, with a tooltip explaining why
   * — never a fake per-channel picker that silently does nothing.
   */
  readonly channelSelectionSupported = false;

  vadParams = $derived<VadParams>({
    threshold: this.thresholdPct / 100,
    min_silence_us: Math.round(this.minSilenceMs * 1000),
    min_speech_us: Math.round(this.minSpeechMs * 1000),
  });

  cutParams = $derived<CutParams>({
    padding_before_us: Math.round(this.paddingBeforeMs * 1000),
    padding_after_us: Math.round(this.paddingAfterMs * 1000),
    merge_gap_us: Math.round(this.mergeGapMs * 1000),
  });

  // ---- Workflow results -------------------------------------------------

  scoreSummary = $state<VadScoreSummary | null>(null);
  segments = $state<SpeechSegment[]>([]);
  cuts = $state<Cut[]>([]);

  analyzing = $state(false);
  previewLoading = $state(false);
  applying = $state(false);
  lastError = $state<string | null>(null);

  /** Set once `Apply Cuts` has succeeded this session, so `Reset` knows to
   * also call `timeline.undo()` (master prompt §12's Reset behavior — one
   * atomic undo step, no second undo mechanism, per
   * `src-tauri/src/timeline/silence.rs`'s module doc comment). */
  appliedThisSession = $state(false);

  // ---- Derived selection context ----------------------------------------

  /**
   * Tracks offered in the "analysis track selection" dropdown. Master
   * prompt §12 literally says "Audio tracks", but this app's video clips
   * routinely carry the only dialogue track (a talking-head recording), and
   * PCM extraction doesn't care about `TrackKind` — it just needs a media
   * file with an audio stream. Restricting to `TrackKind === "audio"` alone
   * would make the common single-video-file case unreachable, so both
   * `audio` and `video` tracks are offered here (documented judgment call,
   * not a literal-spec deviation in behavior — silence detection still only
   * ever reads/writes timeline clips, never source media).
   */
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

  selectedTrack = $derived.by((): Track | null => {
    if (!this.trackId) return null;
    return timeline.tracks.find((t) => t.id === this.trackId) ?? null;
  });

  canAnalyze = $derived(this.selectedMedia !== null && !this.analyzing);
  canPreview = $derived(this.scoreSummary !== null && !this.previewLoading && !this.analyzing);
  canApply = $derived(this.cuts.length > 0 && !this.applying && (this.applyMode === "clip" ? this.clipId !== null : this.trackId !== null));

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  /** Opens the panel, optionally preselecting a track/clip (e.g. a "Detect
   * Silence" action on a specific `MediaLibrary`/`Timeline` clip). Falls
   * back to the first eligible track/clip when nothing is preselected. */
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
    this.scoreSummary = null;
    this.segments = [];
    this.cuts = [];
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

  // -------------------------------------------------------------------
  // Workflow: Analyze -> Preview Cuts -> Apply Cuts -> Reset
  // -------------------------------------------------------------------

  /** **Analyze**: scores the selected clip's media once (expensive, the
   * only step that runs the VAD model — see `commands::vad::score_media_silence`
   * doc comment), then immediately runs an initial Preview with the current
   * slider defaults so the region strip isn't left empty. */
  async analyze(): Promise<void> {
    const media = this.selectedMedia;
    if (!media) return;
    this.analyzing = true;
    this.lastError = null;
    try {
      const result = await commands.scoreMediaSilence(media.id, media.source_path);
      if (result.status === "ok") {
        this.scoreSummary = result.data;
        await this.previewCuts();
      } else {
        this.lastError = result.error.message;
      }
    } finally {
      this.analyzing = false;
    }
  }

  /** **Preview Cuts**: re-segments the already-cached scores under the
   * current sliders, then rebuilds the padded/merged cutlist — never
   * re-runs the model (`segment_media_silence`/`build_silence_cutlist` are
   * both cheap, pure post-processing per their own doc comments). */
  async previewCuts(): Promise<void> {
    const media = this.selectedMedia;
    if (!media || !this.scoreSummary) return;
    this.previewLoading = true;
    this.lastError = null;
    try {
      const segResult = await commands.segmentMediaSilence(media.id, this.vadParams);
      if (segResult.status !== "ok") {
        this.lastError = segResult.error.message;
        return;
      }
      this.segments = segResult.data;
      this.cuts = await commands.buildSilenceCutlist(media.id, media.duration_us, this.segments, this.cutParams);
    } finally {
      this.previewLoading = false;
    }
  }

  /** **Apply Cuts**: sends the previewed cuts to the real timeline as one
   * atomic undo step (clip-level or track-level, per `applyMode`), then
   * folds the resulting `ProjectV1` back into `stores/timeline.svelte.ts` so
   * the main timeline immediately shows real trimmed/split clips. Does not
   * touch source media — only ever produces timeline edits. */
  async applyCuts(): Promise<void> {
    if (!this.canApply) return;
    this.applying = true;
    this.lastError = null;
    try {
      const outcome =
        this.applyMode === "clip"
          ? await timeline.applyExternalProjectResult(commands.applySilenceCuts(this.clipId as string, this.cuts))
          : await timeline.applyExternalProjectResult(
              commands.applySilenceCutsToTrack(this.trackId as string, this.cuts),
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

  /** **Reset**: discards the current preview (scores/segments/cuts) and, if
   * `Apply Cuts` already ran this session, undoes that one atomic apply via
   * the existing `timeline.undo()` — deliberately not a second undo
   * mechanism (`src-tauri/src/timeline/silence.rs` module doc comment). */
  async reset(): Promise<void> {
    if (this.appliedThisSession) {
      await timeline.undo();
    }
    this.resetResults();
  }
}

export const silenceDetector = new SilenceDetectorStore();
