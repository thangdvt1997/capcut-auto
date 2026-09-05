// Svelte 5 runes-based store for the Phase 11 Short Video Generator wizard
// (master prompt §22, `src-tauri/src/shorts/`, `src-tauri/src/commands/
// shorts.rs::generate_shorts`). This is this app's flagship "Long Video ->
// Shorts" feature: settings (duration/aspect/clip count/auto-zoom/optional
// AI) -> `generate_shorts` -> a results list of `ShortCandidate` (a real,
// immediately-editable `ProjectV1` per candidate, paired with the
// `Highlight` metadata that produced it).
//
// Same source track/clip picker shape as `stores/highlightDetection
// .svelte.ts`/`stores/smartEdit.svelte.ts` (the panel always analyzes "the
// currently selected timeline clip's underlying media") — deliberately, not
// a fresh file picker, since `generate_shorts` needs both a resolvable
// source media *path* and an already-produced transcript, and the timeline
// selection is this app's one existing place that ties a `MediaItem` to its
// own `TranscriptEntry[]` (`timeline.project.transcript`, filtered by
// `media_id`, exactly like every other detector in this codebase).
//
// ## "No transcript yet" handling (required by this pass's brief)
//
// `generate_shorts` hard-requires a non-empty transcript (`ShortsError
// ::TranscriptRequired` — see that command's own module doc comment: it is
// a deliberately *not* self-transcribing command, transcription is a slow,
// async, job-based operation that lives in `stores/transcriptEditor
// .svelte.ts`/the Transcript tab instead). Unlike Model Manager's
// `stores/modelManager.svelte.ts::openModelManager()` — a real, callable,
// global "open this modal from anywhere" entry point this store could
// reuse directly (the pattern `stores/transcriptEditor.svelte.ts` itself
// mirrors for its own "no model installed" prompt) — there is no
// equivalent global entry point for "switch to the Transcript tab":
// `LeftPanel.svelte`'s tab selection is genuinely local component state,
// not a store, and both this task's own scope (frontend-only, minimal
// footprint on files a concurrently-running sibling agent may also be
// touching) and this codebase's existing precedent (small, additive,
// honestly-scoped changes over inventing new shared plumbing for one
// caller) argue against adding a new cross-cutting "active left panel tab"
// store just for this one CTA. So this store instead surfaces a clear,
// honest inline message telling the user to open the Transcript tab and
// transcribe the selected media first (`shortsGenerator.noTranscriptNote`)
// — informational, not a broken/fake button — and disables Generate until
// a non-empty transcript exists for the selected media, exactly the same
// "disable + explain why" treatment `highlightDetection`'s own AI-not-
// configured hint uses.
//
// ## Load into editor (arm/confirm — mirrors `stores/highlightDetection
// .svelte.ts`'s "Create new project" exactly)
//
// Each `ShortCandidate` already carries a fully-built, ready-to-load
// `ProjectV1` (correct canvas, one reframed/captioned/optionally-zoomed
// clip) — "load into editor" is simply `timeline.loadProject(candidate
// .project)`, the same real session-replacing mechanism a genuine "open
// project" flow would use. That discards whatever is currently open in
// memory (no Project Manager/unsaved-changes guard exists yet, the exact
// same gap `stores/modelManager.svelte.ts`'s own destructive delete and
// `stores/highlightDetection.svelte.ts`'s "Create new project" already
// document), so it is gated behind the identical in-dialog "click once to
// arm, click again to confirm" pattern those two already established,
// rather than a native `confirm()` popup or a silent overwrite.
//
// ## Preview — deliberately not built as a separate action (documented,
// not silently dropped)
//
// `components/preview/VideoPlayer.svelte`'s own doc comment: the preview
// panel follows *the loaded timeline session's* playhead/selected clip
// (single-clip scrubbing over `timeline.project`), it does not know how to
// render an arbitrary, not-yet-loaded `ProjectV1` passed to it from
// elsewhere. A `ShortCandidate.project` is furthermore not "the same clip,
// just trimmed" — it's a *new* single-clip project with its own reframed
// canvas/aspect, baked-in reframe + optional-zoom keyframes, and
// regenerated captions, so seeking the *original* source clip already on
// the timeline to the highlight's own `start_us` (the way `highlightDetection
// .preview()` seeks for a plain time-range highlight) would show the raw,
// un-reframed/un-captioned/un-zoomed source video — actively misleading,
// not a lighter-weight preview of what this candidate actually produced.
// The only way to see the real candidate is to load it as the active
// session project, which is exactly what "Load into editor" already does —
// so this store offers no separate, lighter-weight "Preview" action.
import { commands } from "../types/bindings";
import type { Clip, MediaItem, ShortCandidate, ShortsAspect, ShortsSettings, Track, TranscriptEntry } from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { aiSettingsStore, currentAiProviderSettings, keyRequirementFor } from "./aiSettings.svelte";

export type DurationPresetKind = "fixed_15" | "fixed_30" | "fixed_60" | "fixed_90" | "custom";

export const CLIP_COUNT_PRESETS: readonly number[] = [1, 3, 5, 10];

export const ASPECT_OPTIONS: readonly ShortsAspect[] = ["vertical_9x_16", "square_1x_1", "portrait_4x_5"];

type WizardStep = "settings" | "results";

class ShortsGeneratorStore {
  open = $state(false);
  step = $state<WizardStep>("settings");

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);

  // ---- Settings (master prompt §22's exact three settings groups, plus
  // this pipeline's own `apply_zoom`/optional AI parameters) --------------

  durationPreset = $state<DurationPresetKind>("fixed_30");
  /** Only consulted when `durationPreset === "custom"`. */
  customSeconds = $state(45);
  aspect = $state<ShortsAspect>("vertical_9x_16");
  clipCount = $state(3);
  applyZoom = $state(true);
  /** Same independent-of-configuration toggle `highlightDetection.useAi`
   * already establishes: both this and `aiConfigured` must hold for
   * `aiSettings` to actually be passed to `generate_shorts`. */
  useAi = $state(true);

  generating = $state(false);
  lastError = $state<string | null>(null);
  candidates = $state<ShortCandidate[]>([]);

  // ---- Per-candidate "load into editor" action state, keyed by
  // `ShortCandidate.highlight.id` (unique per candidate) -------------------

  pendingLoadId = $state<string | null>(null);
  loadingId = $state<string | null>(null);
  loadError = $state<string | null>(null);
  loadedIds = $state<Set<string>>(new Set());

  // ---- Derived selection context (same shape as sibling detectors) ------

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

  transcriptEntries = $derived.by((): TranscriptEntry[] => {
    const media = this.selectedMedia;
    if (!media) return [];
    return (timeline.project?.transcript ?? [])
      .filter((e) => e.media_id === media.id)
      .sort((a, b) => a.start_us - b.start_us);
  });

  hasTranscript = $derived(this.transcriptEntries.length > 0);

  aiSettings = $derived(currentAiProviderSettings());
  aiConfigured = $derived(
    aiSettingsStore.model.trim().length > 0 &&
      (keyRequirementFor(aiSettingsStore.provider) !== "required" || aiSettingsStore.hasKeyConfigured),
  );

  canGenerate = $derived(this.selectedMedia !== null && this.hasTranscript && !this.generating);

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openFor(opts: { trackId?: string; clipId?: string } = {}): void {
    this.resetResults();
    this.step = "settings";
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
    this.lastError = null;
    this.candidates = [];
    this.pendingLoadId = null;
    this.loadingId = null;
    this.loadError = null;
    this.loadedIds = new Set();
  }

  setTrack(trackId: string): void {
    if (trackId === this.trackId) return;
    this.trackId = trackId;
    const clips = (timeline.clipsByTrack.get(trackId) ?? []).filter((c) => c.media_id !== null);
    this.clipId = clips[0]?.id ?? null;
  }

  setClip(clipId: string): void {
    if (clipId === this.clipId) return;
    this.clipId = clipId;
  }

  setDurationPreset(preset: DurationPresetKind): void {
    this.durationPreset = preset;
  }

  setCustomSeconds(seconds: number): void {
    this.customSeconds = Math.max(1, Math.round(seconds) || 1);
  }

  setAspect(aspect: ShortsAspect): void {
    this.aspect = aspect;
  }

  setClipCount(count: number): void {
    this.clipCount = count;
  }

  backToSettings(): void {
    this.step = "settings";
  }

  // -------------------------------------------------------------------
  // Generate
  // -------------------------------------------------------------------

  private settingsSnapshot(): ShortsSettings {
    const duration =
      this.durationPreset === "custom" ? { kind: "custom" as const, seconds: this.customSeconds } : { kind: this.durationPreset };
    return { duration, aspect: this.aspect, clip_count: this.clipCount };
  }

  async generate(): Promise<void> {
    if (!this.canGenerate) return;
    const media = this.selectedMedia;
    if (!media) return;
    this.generating = true;
    this.lastError = null;
    try {
      const aiSettings = this.useAi && this.aiConfigured ? this.aiSettings : null;
      const result = await commands.generateShorts(
        media.source_path,
        this.transcriptEntries,
        this.settingsSnapshot(),
        this.applyZoom,
        aiSettings,
      );
      if (result.status === "ok") {
        this.candidates = result.data;
        this.pendingLoadId = null;
        this.loadError = null;
        this.loadedIds = new Set();
        this.step = "results";
      } else {
        this.candidates = [];
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.generating = false;
    }
  }

  // -------------------------------------------------------------------
  // Load into editor (arm/confirm — see class doc comment)
  // -------------------------------------------------------------------

  armLoad(candidate: ShortCandidate): void {
    this.pendingLoadId = candidate.highlight.id;
  }

  cancelLoad(): void {
    this.pendingLoadId = null;
  }

  async confirmLoad(candidate: ShortCandidate): Promise<void> {
    if (this.pendingLoadId !== candidate.highlight.id || this.loadingId !== null) return;
    this.loadingId = candidate.highlight.id;
    this.loadError = null;
    try {
      await timeline.loadProject(candidate.project);
      this.loadedIds = new Set(this.loadedIds).add(candidate.highlight.id);
      this.pendingLoadId = null;
    } catch (err) {
      this.loadError = String(err);
    } finally {
      this.loadingId = null;
    }
  }
}

export const shortsGenerator = new ShortsGeneratorStore();
