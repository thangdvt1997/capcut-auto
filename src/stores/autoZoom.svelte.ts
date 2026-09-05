// Svelte 5 runes-based store for the Phase 11 Auto-Zoom panel (master
// prompt §24, `src-tauri/src/zoom/` / `src-tauri/src/commands/zoom.rs`).
// Placement decision (documented here and in `IMPLEMENTATION_PLAN.md`):
// unlike the Silence/Filler/Scene detectors (each a toolbar-triggered
// dialog with its own track/clip picker), auto-zoom is a genuinely
// per-clip *property* — so this store has no independent clip picker at
// all, it always follows `stores/timeline.svelte.ts`'s own selection
// (`timeline.selectedClipIds`), the same way `RightPanel.svelte`'s
// Properties tab is meant to work. Rendered inline there
// (`components/zoom/AutoZoomPanel.svelte`), not as a modal dialog.
//
// Trigger sources actually wired (master prompt §24's four):
//   - "long static scene": real, via `detect_media_scenes` (this store's
//     own `detectScenesForClip`, reusing `stores/sceneDetector.svelte.ts`'s
//     `sceneThumbnailDir` resolution) feeding `Scene[]` into
//     `generate_zoom_triggers`.
//   - "manual markers": real, sourced from the session's own
//     `stores/timeline.svelte.ts` `TimelineMarker`s (task brief: "reuse
//     TimelineMarker`s ... if that's a natural source" — it is, the
//     toolbar's "◆+" button already creates them at the playhead) — the
//     user checks which markers should count as zoom triggers.
//   - "important sentence" / "speaker emphasis": **honestly not wired in
//     this pass**. `generate_zoom_triggers`'s `emphasis_windows` parameter
//     needs real `EmphasisWindow[]` (`highlights::signals::windowed_rms_energy`
//     scores over PCM), and there is no Tauri command exposing that
//     computation to the frontend as of this pass (only
//     `commands::render::compute_voice_speech_segments`, a *different*
//     VAD-based signal, and `detect_highlights`'s internal use of it,
//     neither of which returns raw `EmphasisWindow`s) — always passed as
//     `[]`, never fabricated client-side. Documented here rather than
//     silently omitted; a future pass adding a real
//     `compute_emphasis_windows`-shaped command would slot in here with no
//     other change needed.

import { commands } from "../types/bindings";
import type { Clip, Keyframe, MediaItem, Scene, ZoomIntensity, ZoomTrigger } from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { sceneThumbnailDir } from "./sceneDetector.svelte";

class AutoZoomStore {
  intensity = $state<ZoomIntensity>("medium");

  /** Which of the session's `TimelineMarker`s currently count as manual
   * zoom triggers — a subset, not "all markers always count", since
   * markers are also used for plain navigation (task brief: manual-marker
   * *input*, not "every marker is a zoom point"). */
  manualMarkerIds = $state<Set<string>>(new Set());

  scenes = $state<Scene[]>([]);
  detectingScenes = $state(false);
  scenesError = $state<string | null>(null);

  triggers = $state<ZoomTrigger[]>([]);
  generatingTriggers = $state(false);
  triggersError = $state<string | null>(null);

  previewKeyframes = $state<Keyframe[]>([]);

  applying = $state(false);
  applyError = $state<string | null>(null);
  appliedThisSession = $state(false);

  // -------------------------------------------------------------------
  // Derived: always follows the live timeline selection (see class doc
  // comment) — no independent picker.
  // -------------------------------------------------------------------

  selectedClip = $derived.by((): Clip | null => {
    const [firstId] = timeline.selectedClipIds;
    if (!firstId) return null;
    return timeline.clips.find((c) => c.id === firstId) ?? null;
  });

  selectedMedia = $derived.by((): MediaItem | null => {
    const clip = this.selectedClip;
    if (!clip?.media_id) return null;
    return timeline.mediaById.get(clip.media_id) ?? null;
  });

  manualMarkerTimestampsUs = $derived.by((): number[] =>
    timeline.markers.filter((m) => this.manualMarkerIds.has(m.id)).map((m) => m.time_us),
  );

  canDetectScenes = $derived(this.selectedMedia !== null && !this.detectingScenes);
  canGenerateTriggers = $derived(
    this.selectedClip !== null &&
      !this.generatingTriggers &&
      (this.scenes.length > 0 || this.manualMarkerTimestampsUs.length > 0),
  );
  canApply = $derived(this.triggers.length > 0 && this.selectedClip !== null && !this.applying);

  // -------------------------------------------------------------------
  // Reset when the selected clip changes (called from the panel's own
  // `$effect`, since a plain store class has no lifecycle of its own to
  // hook into `timeline.selectedClipIds` changing).
  // -------------------------------------------------------------------

  resetForNewClip(): void {
    this.scenes = [];
    this.scenesError = null;
    this.triggers = [];
    this.triggersError = null;
    this.previewKeyframes = [];
    this.applyError = null;
    this.appliedThisSession = false;
    this.manualMarkerIds = new Set();
  }

  toggleMarker(markerId: string): void {
    const next = new Set(this.manualMarkerIds);
    if (next.has(markerId)) next.delete(markerId);
    else next.add(markerId);
    this.manualMarkerIds = next;
  }

  // -------------------------------------------------------------------
  // Detect scenes -> Generate triggers -> Apply
  // -------------------------------------------------------------------

  async detectScenesForClip(): Promise<void> {
    const media = this.selectedMedia;
    if (!media || this.detectingScenes) return;
    this.detectingScenes = true;
    this.scenesError = null;
    try {
      const thumbnailDir = await sceneThumbnailDir();
      const result = await commands.detectMediaScenes(media.source_path, media.duration_us, thumbnailDir, null);
      if (result.status === "ok") {
        this.scenes = result.data;
      } else {
        this.scenesError = result.error.message;
      }
    } catch (err) {
      this.scenesError = String(err);
    } finally {
      this.detectingScenes = false;
    }
  }

  async generateTriggers(): Promise<void> {
    if (!this.canGenerateTriggers) return;
    this.generatingTriggers = true;
    this.triggersError = null;
    try {
      this.triggers = await commands.generateZoomTriggers(this.scenes, this.manualMarkerTimestampsUs, []);
      this.previewKeyframes = [];
    } catch (err) {
      this.triggersError = String(err);
    } finally {
      this.generatingTriggers = false;
    }
  }

  async apply(): Promise<void> {
    if (!this.canApply || !this.selectedClip) return;
    this.applying = true;
    this.applyError = null;
    try {
      const outcome = await timeline.applyExternalProjectResult(
        commands.applyAutoZoomToClip(this.selectedClip.id, this.triggers, this.intensity),
      );
      if (outcome.ok) {
        this.appliedThisSession = true;
      } else {
        this.applyError = outcome.error;
      }
    } finally {
      this.applying = false;
    }
  }
}

export const autoZoom = new AutoZoomStore();
