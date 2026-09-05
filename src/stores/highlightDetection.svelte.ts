// Svelte 5 runes-based store for the Phase 10 follow-up Highlight Detection
// dialog (master prompt §21, `src-tauri/src/highlights/`). Same source
// track/clip picker shape as `stores/fillerWordDetector.svelte.ts`/
// `stores/smartEdit.svelte.ts` (the panel always analyzes "the currently
// selected timeline clip's underlying media"), calling `detect_highlights`
// against that media's real file path + its transcript (if any) + an
// optional `AiProviderSettings` read reactively from `stores/aiSettings
// .svelte.ts` (the concurrent AI Settings UI pass, Phase 10's other work
// stream, which landed in this same working tree while this pass was in
// progress) via its exported `currentAiProviderSettings()`/`aiSettingsStore`.
//
// Per-highlight actions (master prompt §21's exact UI: Preview / Add to
// timeline / Create new project / Export clip):
//
//   - Preview: seeks the shared timeline playhead via
//     `lib/timelineSeek.ts` (the same mechanism Smart Edit/Transcript
//     Editor use) — no second preview mechanism.
//   - Add to timeline: `timeline.addMediaAsClip(media, {sourceInUs,
//     sourceOutUs})` — the *existing* client-side "add media as clip"
//     bridge (`stores/timeline.svelte.ts` doc comment: there is no
//     `insert_clip` backend command, per Phase 4's documented gap), now
//     extended to accept a sub-range so a highlight's own span is what
//     actually lands on the timeline, not the whole source file.
//   - Create new project: genuinely achievable with existing commands
//     (`new_project` + the same clip-append bridge, `timeline.loadProject`)
//     without inventing new backend surface — but it silently discards
//     whatever is currently open in memory (there is no Project Manager/
//     unsaved-changes guard yet), so this is gated behind the same
//     "click once to arm, click again to confirm" in-dialog pattern
//     `stores/modelManager.svelte.ts` established for its own destructive
//     delete action, rather than a native `confirm()` popup (that file's
//     own doc comment explains why: "keeps it in-dialog and themeable").
//   - Export clip: also genuinely achievable by reusing the *existing*
//     render pipeline (`start_render_job`) against a synthetic,
//     throwaway single-clip `ProjectV1` built the same way (`new_project`
//     + the clip-append bridge) — never touching the live session project,
//     never a second render code path. Progress is read straight out of
//     `stores/render.svelte.ts`'s own `progressByJob` (its constructor
//     already listens on the `render:progress` event for *every* job id,
//     not just ones it started itself), so this doesn't need a second
//     event listener.

import { save } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type { Clip, Highlight, HighlightDetectionResult, MediaItem, Track, TranscriptEntry } from "../types/bindings";
import { timeline, projectWithMediaClip } from "./timeline.svelte";
import { renderStore } from "./render.svelte";
import { aiSettingsStore, currentAiProviderSettings, keyRequirementFor } from "./aiSettings.svelte";
import { resolveTimelinePositionForMedia } from "../lib/timelineSeek";

function sanitizeFilename(name: string): string {
  const cleaned = name.replace(/[\\/:*?"<>|]+/g, " ").trim();
  return cleaned.length > 0 ? cleaned : "highlight";
}

class HighlightDetectionStore {
  open = $state(false);

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);

  maxHighlights = $state(5);
  /** User's own toggle for whether to attempt the AI semantic signal at all
   * (master prompt §21 lists it as one of several signals, not mandatory) —
   * independent of whether AI settings are actually configured; both must
   * be true for `aiSettings` to actually be passed to `detect_highlights`. */
  useAi = $state(true);

  result = $state<HighlightDetectionResult | null>(null);
  detecting = $state(false);
  lastError = $state<string | null>(null);
  seekMissed = $state(false);

  // ---- Per-highlight action state, keyed by `Highlight.id` --------------

  addingId = $state<string | null>(null);
  addedIds = $state<Set<string>>(new Set());
  addError = $state<string | null>(null);

  /** Two-step "Create new project" confirmation, same in-dialog
   * arm/confirm pattern as `stores/modelManager.svelte.ts`'s delete
   * confirmation (see class doc comment) — only one highlight can be armed
   * at a time. */
  pendingCreateProjectId = $state<string | null>(null);
  creatingProjectId = $state<string | null>(null);
  createProjectError = $state<string | null>(null);

  exportingId = $state<string | null>(null);
  /** Keyed by `Highlight.id` -> the render job id `start_render_job`
   * returned, so `exportProgressFor` can read live progress straight out of
   * `renderStore.progressByJob` (see class doc comment). */
  exportJobByHighlight = $state<Record<string, string>>({});
  exportError = $state<string | null>(null);
  exportedPathByHighlight = $state<Record<string, string>>({});

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

  /** Reactive snapshot of the AI Settings dialog's current provider
   * settings — see `stores/smartEdit.svelte.ts`'s identical field for the
   * full rationale (same `stores/aiSettings.svelte.ts` dependency). */
  aiSettings = $derived(currentAiProviderSettings());
  aiConfigured = $derived(
    aiSettingsStore.model.trim().length > 0 &&
      (keyRequirementFor(aiSettingsStore.provider) !== "required" || aiSettingsStore.hasKeyConfigured),
  );
  highlights = $derived<Highlight[]>(this.result?.highlights ?? []);
  usedAiSemanticSignal = $derived(this.result?.used_ai_semantic_signal ?? false);

  canDetect = $derived(this.selectedMedia !== null && !this.detecting);

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

  /** Phase 11 follow-up bridge: `stores/sceneDetector.svelte.ts`'s "Generate
   * Highlights from Scenes" (master prompt §25) hands its externally
   * computed `Highlight[]` here so they render through this exact dialog /
   * `HighlightCard.svelte` list, with the same full Preview/Add to
   * timeline/Create project/Export clip action set, instead of a second,
   * poorer results UI. `used_ai_semantic_signal` is always `false` here —
   * scene-derived highlights carry no AI semantic signal at all. */
  showExternalHighlights(highlights: Highlight[], opts: { trackId?: string | null; clipId?: string | null } = {}): void {
    this.resetResults();
    this.open = true;
    this.trackId = opts.trackId ?? this.eligibleTracks[0]?.id ?? null;
    this.clipId = opts.clipId ?? null;
    this.result = { highlights, used_ai_semantic_signal: false };
  }

  close(): void {
    this.open = false;
    this.resetResults();
  }

  private resetResults(): void {
    this.result = null;
    this.lastError = null;
    this.seekMissed = false;
    this.addingId = null;
    this.addedIds = new Set();
    this.addError = null;
    this.pendingCreateProjectId = null;
    this.creatingProjectId = null;
    this.createProjectError = null;
    this.exportingId = null;
    this.exportJobByHighlight = {};
    this.exportError = null;
    this.exportedPathByHighlight = {};
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
  // Detect
  // -------------------------------------------------------------------

  async detect(): Promise<void> {
    if (!this.canDetect) return;
    const media = this.selectedMedia;
    if (!media) return;
    this.detecting = true;
    this.lastError = null;
    try {
      const aiSettings = this.useAi && this.aiConfigured ? this.aiSettings : null;
      const result = await commands.detectHighlights(
        media.source_path,
        this.transcriptEntries,
        media.duration_us,
        aiSettings,
        this.maxHighlights > 0 ? this.maxHighlights : null,
      );
      if (result.status === "ok") {
        this.result = result.data;
      } else {
        this.result = null;
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.detecting = false;
    }
  }

  // -------------------------------------------------------------------
  // Preview: seek the shared playhead (no second preview mechanism)
  // -------------------------------------------------------------------

  preview(highlight: Highlight): void {
    const media = this.selectedMedia;
    if (!media) return;
    const resolved = resolveTimelinePositionForMedia(media, highlight.start_us, this.clipId);
    this.seekMissed = resolved === null;
    if (resolved) {
      timeline.selectClip(resolved.clip.id);
      timeline.setPlayhead(resolved.playheadUs);
    }
  }

  // -------------------------------------------------------------------
  // Add to timeline (existing client-side bridge, sub-range)
  // -------------------------------------------------------------------

  async addToTimeline(highlight: Highlight): Promise<void> {
    const media = this.selectedMedia;
    if (!media || this.addingId !== null) return;
    this.addingId = highlight.id;
    this.addError = null;
    try {
      await timeline.addMediaAsClip(media, { sourceInUs: highlight.start_us, sourceOutUs: highlight.end_us });
      this.addedIds = new Set(this.addedIds).add(highlight.id);
    } catch (err) {
      this.addError = String(err);
    } finally {
      this.addingId = null;
    }
  }

  // -------------------------------------------------------------------
  // Create new project (arm/confirm — see class doc comment)
  // -------------------------------------------------------------------

  armCreateProject(highlight: Highlight): void {
    this.pendingCreateProjectId = highlight.id;
  }

  cancelCreateProject(): void {
    this.pendingCreateProjectId = null;
  }

  async confirmCreateProject(highlight: Highlight): Promise<void> {
    const media = this.selectedMedia;
    if (!media || this.pendingCreateProjectId !== highlight.id || this.creatingProjectId !== null) return;
    this.creatingProjectId = highlight.id;
    this.createProjectError = null;
    try {
      const base = await commands.newProject(highlight.title || "Highlight Project");
      const { project } = projectWithMediaClip(base, media, {
        sourceInUs: highlight.start_us,
        sourceOutUs: highlight.end_us,
      });
      await timeline.loadProject(project);
      this.pendingCreateProjectId = null;
    } catch (err) {
      this.createProjectError = String(err);
    } finally {
      this.creatingProjectId = null;
    }
  }

  // -------------------------------------------------------------------
  // Export clip (synthetic single-clip project through the existing
  // render pipeline — see class doc comment)
  // -------------------------------------------------------------------

  exportProgressFor(highlightId: string): { fraction: number | null; done: boolean; error: string | null } | null {
    const jobId = this.exportJobByHighlight[highlightId];
    if (!jobId) return null;
    const event = renderStore.progressByJob[jobId];
    if (!event) return null;
    return { fraction: event.fraction, done: event.done, error: event.error };
  }

  async exportClip(highlight: Highlight): Promise<void> {
    const media = this.selectedMedia;
    if (!media || this.exportingId !== null) return;
    const chosen = await save({
      filters: [{ name: "MP4", extensions: ["mp4"] }],
      defaultPath: `${sanitizeFilename(highlight.title)}.mp4`,
    });
    if (!chosen) return;
    this.exportingId = highlight.id;
    this.exportError = null;
    try {
      const base = await commands.newProject(`Highlight export — ${highlight.title}`);
      const { project } = projectWithMediaClip(base, media, {
        sourceInUs: highlight.start_us,
        sourceOutUs: highlight.end_us,
      });
      const settings = renderStore.buildSettingsInput();
      const result = await commands.startRenderJob(project, settings, chosen);
      if (result.status === "ok") {
        this.exportJobByHighlight = { ...this.exportJobByHighlight, [highlight.id]: result.data };
        this.exportedPathByHighlight = { ...this.exportedPathByHighlight, [highlight.id]: chosen };
      } else {
        this.exportError = result.error.message;
      }
    } catch (err) {
      this.exportError = String(err);
    } finally {
      this.exportingId = null;
    }
  }
}

export const highlightDetection = new HighlightDetectionStore();
