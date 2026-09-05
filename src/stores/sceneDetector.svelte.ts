// Svelte 5 runes-based store for the Phase 11 Scene Detector panel (master
// prompt §25). Same "own transient workflow state, composes with
// `stores/timeline.svelte.ts` rather than living inside it" shape
// `stores/silenceDetector.svelte.ts`/`stores/fillerWordDetector.svelte.ts`
// already establish — see either file's own doc comment for the rationale.
//
// Detection source: `detect_media_scenes` runs real ffmpeg scene-cut
// detection directly against the selected clip's underlying media file
// (unlike Filler Words, this needs no transcript). Each scene's thumbnail is
// written into a shared `$APPLOCALDATA/scene_thumbnails` directory, resolved
// client-side via `@tauri-apps/api/path` (mirroring the backend's own
// `templates_dir`/`models_dir` "one shared directory under app-local-data"
// convention — there is no dedicated Tauri command for this one, so it's
// resolved the same way `CapCutSettingsDialog.svelte`'s browse button
// resolves paths: directly from the frontend).
//
// Selection: one shared `checked: Record<Scene.id, boolean>` set (all-true
// right after Detect, same as `fillerWordDetector.checked`), reused for BOTH
// "Split at Selected" (their `start_us` boundaries) and "Remove Selected"
// (their full spans) — two different actions sharing one selection, rather
// than two independent checkbox sets.
//
// "Generate Highlights from Scenes": bridges into the existing Highlight
// Detection dialog (`stores/highlightDetection.svelte.ts`'s
// `showExternalHighlights`) so results render through the *same*
// `HighlightCard.svelte` list with its full Preview/Add to timeline/Create
// project/Export clip action set, rather than a second, poorer results UI
// (task brief: "reuse HighlightCard for consistent rendering").

import { appLocalDataDir, join } from "@tauri-apps/api/path";
import { commands } from "../types/bindings";
import type { Clip, MediaItem, Scene, Track } from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { highlightDetection } from "./highlightDetection.svelte";

export type ApplyMode = "clip" | "track";

/** Shared `$APPLOCALDATA/scene_thumbnails` resolution (see class doc
 * comment above) — also reused by `stores/autoZoom.svelte.ts`'s own
 * "Detect Scenes for this Clip" step (the long-static-scene zoom trigger
 * needs the same real `Scene` data this dialog produces), rather than
 * duplicating the two-line join in a second place. */
export async function sceneThumbnailDir(): Promise<string> {
  return join(await appLocalDataDir(), "scene_thumbnails");
}

class SceneDetectorStore {
  open = $state(false);

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);
  /** Only meaningful for "Remove Selected" — "Split at Selected" always
   * targets `clipId` (a split has no meaningful "whole track" variant, per
   * `timeline::scenes::split_clip_at_scenes`'s own single-clip signature). */
  applyMode = $state<ApplyMode>("clip");

  /** 0-100, maps to `detect_media_scenes`'s `threshold` (0.0-1.0) — same
   * ms-vs-fraction unit convention as `silenceDetector.thresholdPct`. */
  thresholdPct = $state(40);

  scenes = $state<Scene[]>([]);
  checked = $state<Record<string, boolean>>({});

  detecting = $state(false);
  splitting = $state(false);
  removing = $state(false);
  generatingHighlights = $state(false);
  lastError = $state<string | null>(null);

  // ---- Derived selection context (same shape as sibling detectors) ------

  eligibleTracks = $derived.by((): Track[] => timeline.tracks.filter((t) => t.kind === "audio" || t.kind === "video"));

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

  checkedScenes = $derived<Scene[]>(this.scenes.filter((s) => this.checked[s.id] ?? false));

  canDetect = $derived(this.selectedMedia !== null && !this.detecting);
  canSplit = $derived(this.checkedScenes.length > 0 && !this.splitting && this.clipId !== null);
  canRemove = $derived(
    this.checkedScenes.length > 0 &&
      !this.removing &&
      (this.applyMode === "clip" ? this.clipId !== null : this.trackId !== null),
  );
  canGenerateHighlights = $derived(this.checkedScenes.length > 0 && !this.generatingHighlights);

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
    this.scenes = [];
    this.checked = {};
    this.lastError = null;
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

  toggleScene(sceneId: string): void {
    this.checked = { ...this.checked, [sceneId]: !(this.checked[sceneId] ?? false) };
  }

  selectAll(): void {
    const next: Record<string, boolean> = {};
    for (const scene of this.scenes) next[scene.id] = true;
    this.checked = next;
  }

  deselectAll(): void {
    const next: Record<string, boolean> = {};
    for (const scene of this.scenes) next[scene.id] = false;
    this.checked = next;
  }

  // -------------------------------------------------------------------
  // Detect
  // -------------------------------------------------------------------

  async detect(): Promise<void> {
    const media = this.selectedMedia;
    if (!media || this.detecting) return;
    this.detecting = true;
    this.lastError = null;
    try {
      const thumbnailDir = await sceneThumbnailDir();
      const result = await commands.detectMediaScenes(
        media.source_path,
        media.duration_us,
        thumbnailDir,
        this.thresholdPct / 100,
      );
      if (result.status === "ok") {
        this.scenes = result.data;
        const nextChecked: Record<string, boolean> = {};
        for (const scene of result.data) nextChecked[scene.id] = true;
        this.checked = nextChecked;
      } else {
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.detecting = false;
    }
  }

  // -------------------------------------------------------------------
  // Split / Remove / Generate highlights
  // -------------------------------------------------------------------

  /** "Split at scenes" (master prompt §25): every checked scene's
   * `start_us`. A boundary that isn't strictly inside the clip's own span
   * (e.g. the very first scene's `0`) is harmless — `split_clip_at_scenes`
   * already only splits at boundaries strictly inside the clip. */
  async splitAtSelected(): Promise<void> {
    if (!this.canSplit || !this.clipId) return;
    this.splitting = true;
    this.lastError = null;
    try {
      const boundaries = this.checkedScenes.map((s) => s.start_us);
      const outcome = await timeline.applyExternalProjectResult(commands.splitClipAtScenes(this.clipId, boundaries));
      if (!outcome.ok) this.lastError = outcome.error;
    } finally {
      this.splitting = false;
    }
  }

  /** "Remove scenes" (master prompt §25): cuts every checked scene's own
   * span out of the clip (or every clip on the track sharing this media,
   * per `applyMode`) — structurally identical to a silence/filler-word
   * removal (`timeline::scenes` module doc comment). */
  async removeSelected(): Promise<void> {
    if (!this.canRemove) return;
    const media = this.selectedMedia;
    if (!media) return;
    this.removing = true;
    this.lastError = null;
    try {
      const scenes = this.checkedScenes;
      const outcome =
        this.applyMode === "clip"
          ? await timeline.applyExternalProjectResult(commands.removeScenesFromClip(this.clipId as string, scenes, media.id))
          : await timeline.applyExternalProjectResult(
              commands.removeScenesFromTrack(this.trackId as string, scenes, media.id),
            );
      if (!outcome.ok) this.lastError = outcome.error;
    } finally {
      this.removing = false;
    }
  }

  /** "Generate highlights from scenes" (master prompt §25) — pure, no
   * session needed; results are handed to the existing Highlight Detection
   * dialog for display/actions (see class doc comment). */
  async generateHighlights(): Promise<void> {
    if (!this.canGenerateHighlights) return;
    this.generatingHighlights = true;
    this.lastError = null;
    try {
      const highlights = await commands.generateHighlightsFromScenes(this.checkedScenes, null);
      highlightDetection.showExternalHighlights(highlights, { trackId: this.trackId, clipId: this.clipId });
      this.close();
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.generatingHighlights = false;
    }
  }
}

export const sceneDetector = new SceneDetectorStore();
