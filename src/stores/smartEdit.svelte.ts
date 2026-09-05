// Svelte 5 runes-based store for the Phase 10 follow-up Smart Edit dialog
// (master prompt §19, `src-tauri/src/ai/smart_edit.rs`). Same overall shape
// as `stores/fillerWordDetector.svelte.ts` (Detect -> candidates-first
// review -> Apply -> Reset, against the same source-track/clip picker and
// the same `apply_silence_cuts`-derived backend contract) — see that file's
// doc comment for the shared rationale. One real difference the task brief
// calls out: each Smart Edit recommendation already carries its own
// AI-suggested action (`SmartEditAction`: Keep/Remove/Shorten/Highlight),
// which the user can override per-row — this is not a simple
// checked/unchecked candidate list, it's "review and optionally downgrade/
// upgrade each recommendation's action, then apply whatever the resulting
// actions actually cut".
//
// `analyze()` needs an `AiProviderSettings` to call `analyze_smart_edit`
// with. The concurrent AI Settings UI pass (Phase 10, a different work
// stream) owns configuring that — `stores/aiSettings.svelte.ts`, which
// landed in this same working tree while this pass was in progress and
// explicitly anticipates this exact call site (`currentAiProviderSettings`'s
// own doc comment: "for other stores (the NL command box, and a future
// Smart Edit UI) to read the currently configured provider settings").
// This store reads it reactively (`aiSettingsStore.model`/`.provider`/
// `.hasKeyConfigured`), never owns or duplicates it.

import { commands } from "../types/bindings";
import type {
  Clip,
  Cut,
  MediaItem,
  SmartEditAction,
  SmartEditRecommendation,
  Track,
  TranscriptEntry,
} from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { aiSettingsStore, currentAiProviderSettings, keyRequirementFor } from "./aiSettings.svelte";
import { resolveTimelinePositionForMedia } from "../lib/timelineSeek";

export type ApplyMode = "clip" | "track";

function defaultShortenTargetUs(startUs: number, endUs: number): number {
  // No AI-suggested `Shorten` recommendation needs a client-picked default
  // (its own `target_duration_us` is used as-is) — this only applies when
  // the user *overrides* a Keep/Remove/Highlight recommendation into
  // Shorten, which has no target duration of its own yet. Half the span,
  // clamped to the schema's own valid range (`0 < target < span`,
  // `ai::smart_edit::validate_recommendation`), is a reasonable starting
  // point for the user to then drag/type away from.
  const span = Math.max(1, endUs - startUs);
  return Math.min(span - 1, Math.max(1, Math.round(span / 2)));
}

class SmartEditStore {
  open = $state(false);

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);
  applyMode = $state<ApplyMode>("clip");

  recommendations = $state<SmartEditRecommendation[]>([]);
  /** Per-recommendation user override, keyed by `SmartEditRecommendation.id`
   * — absent means "use the AI's own `suggested_action` unchanged". */
  actionOverrides = $state<Record<string, SmartEditAction>>({});

  previewCuts = $state<Cut[]>([]);
  previewLoading = $state(false);

  analyzing = $state(false);
  applying = $state(false);
  lastError = $state<string | null>(null);
  appliedThisSession = $state(false);
  seekMissed = $state(false);

  // ---- Derived selection context (same shape as `fillerWordDetector`'s) ----

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

  /** Reactive snapshot of the AI Settings dialog's current provider
   * settings (`stores/aiSettings.svelte.ts`) — recomputed whenever the user
   * changes provider/model/etc. there, since this reads that store's own
   * `$state` fields. */
  aiSettings = $derived(currentAiProviderSettings());

  /** `model` non-empty, and (only when this provider kind actually requires
   * one — `keyRequirementFor`) a key has actually been saved for it. */
  aiConfigured = $derived(
    aiSettingsStore.model.trim().length > 0 &&
      (keyRequirementFor(aiSettingsStore.provider) !== "required" || aiSettingsStore.hasKeyConfigured),
  );

  /** Each recommendation with its current effective action (the user's
   * override, if any, else the AI's own suggestion) — what's actually sent
   * to `build_cuts_from_smart_edit_recommendations`/apply. */
  effectiveRecommendations = $derived.by((): SmartEditRecommendation[] => {
    return this.recommendations.map((r) => ({
      ...r,
      suggested_action: this.actionOverrides[r.id] ?? r.suggested_action,
    }));
  });

  canAnalyze = $derived(this.transcriptEntries.length > 0 && this.aiConfigured && !this.analyzing);
  canApply = $derived(
    this.previewCuts.length > 0 &&
      !this.applying &&
      (this.applyMode === "clip" ? this.clipId !== null : this.trackId !== null),
  );

  totalPreviewDurationUs = $derived(
    this.previewCuts.reduce((sum, c) => sum + Math.max(0, c.end_us - c.start_us), 0),
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
    this.recommendations = [];
    this.actionOverrides = {};
    this.previewCuts = [];
    this.lastError = null;
    this.appliedThisSession = false;
    this.seekMissed = false;
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
  // Workflow: Analyze -> per-row action override -> live Preview -> Apply
  // -------------------------------------------------------------------

  async analyze(): Promise<void> {
    if (!this.canAnalyze) return;
    this.analyzing = true;
    this.lastError = null;
    try {
      const result = await commands.analyzeSmartEdit(this.aiSettings, this.transcriptEntries);
      if (result.status === "ok") {
        this.recommendations = result.data;
        this.actionOverrides = {};
        await this.refreshPreview();
      } else {
        this.recommendations = [];
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.analyzing = false;
    }
  }

  /** Current effective action for one recommendation (override, else the
   * AI's own suggestion) — what the row's action selector should show as
   * active. */
  actionFor(rec: SmartEditRecommendation): SmartEditAction {
    return this.actionOverrides[rec.id] ?? rec.suggested_action;
  }

  setAction(rec: SmartEditRecommendation, action: SmartEditAction): void {
    this.actionOverrides = { ...this.actionOverrides, [rec.id]: action };
    void this.refreshPreview();
  }

  /** Convenience for the row UI switching *into* Shorten from another
   * action — picks a starting `target_duration_us` since neither Keep,
   * Remove, nor Highlight carry one. */
  setActionToShorten(rec: SmartEditRecommendation): void {
    const current = this.actionFor(rec);
    const targetUs =
      current.type === "shorten" ? current.target_duration_us : defaultShortenTargetUs(rec.start_us, rec.end_us);
    this.setAction(rec, { type: "shorten", target_duration_us: targetUs });
  }

  setShortenTargetMs(rec: SmartEditRecommendation, ms: number): void {
    const span = rec.end_us - rec.start_us;
    const targetUs = Math.min(Math.max(1, Math.round(ms * 1000)), Math.max(1, span - 1));
    this.setAction(rec, { type: "shorten", target_duration_us: targetUs });
  }

  /** Live preview of what will actually be cut (task brief) — a pure
   * conversion, so this is safe to re-run after every action override, not
   * just once after Analyze. */
  async refreshPreview(): Promise<void> {
    const media = this.selectedMedia;
    if (!media || this.recommendations.length === 0) {
      this.previewCuts = [];
      return;
    }
    this.previewLoading = true;
    try {
      this.previewCuts = await commands.buildCutsFromSmartEditRecommendations(media.id, this.effectiveRecommendations);
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.previewLoading = false;
    }
  }

  /** Master prompt §19-adjacent convenience: jump the shared timeline
   * playhead to a recommendation's start, same mechanism
   * `stores/transcriptEditor.svelte.ts`'s "click word -> seek video"
   * established (`lib/timelineSeek.ts`). */
  seekToRecommendation(rec: SmartEditRecommendation): void {
    const media = this.selectedMedia;
    if (!media) return;
    const resolved = resolveTimelinePositionForMedia(media, rec.start_us, this.clipId);
    this.seekMissed = resolved === null;
    if (resolved) {
      timeline.selectClip(resolved.clip.id);
      timeline.setPlayhead(resolved.playheadUs);
    }
  }

  /** **Apply**: sends the effective (possibly user-overridden) subset of
   * recommendations to the existing `apply_smart_edit_recommendations_to_clip`/
   * `_to_track` commands — one atomic undo step, same contract as
   * `fillerWordDetector.applyCuts`. `Keep`/`Highlight` recommendations
   * simply produce no `Cut` server-side. */
  async apply(): Promise<void> {
    if (!this.canApply) return;
    const media = this.selectedMedia;
    if (!media) return;
    this.applying = true;
    this.lastError = null;
    try {
      const recs = this.effectiveRecommendations;
      const outcome =
        this.applyMode === "clip"
          ? await timeline.applyExternalProjectResult(
              commands.applySmartEditRecommendationsToClip(this.clipId as string, media.id, recs),
            )
          : await timeline.applyExternalProjectResult(
              commands.applySmartEditRecommendationsToTrack(this.trackId as string, media.id, recs),
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

export const smartEdit = new SmartEditStore();
