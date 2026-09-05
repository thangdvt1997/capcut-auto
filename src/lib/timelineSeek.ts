// Shared "source-media timestamp -> real on-timeline position" resolver
// (Phase 10 follow-up: Smart Edit / Highlight Detection's own "Preview"
// actions). Factored out of `stores/transcriptEditor.svelte.ts`'s private
// `resolveTimelinePosition` (Phase 7's "click word -> seek video") so there
// is exactly one seek-mapping mechanism in the app, not a second copy of
// this math for every feature that wants to jump the shared playhead to a
// moment in a piece of media — per this pass's own task brief ("reuse
// `stores/timeline.svelte.ts`'s real playhead state... don't build a second
// preview mechanism").
//
// `transcriptEditor.svelte.ts` itself is left untouched (its own method
// stays private and is not exported) — this is a standalone reimplementation
// of the same, small, already-proven mapping, not a refactor of that file.

import { timeline } from "../stores/timeline.svelte";
import type { Clip, MediaItem } from "../types/bindings";

export interface ResolvedTimelinePosition {
  clip: Clip;
  playheadUs: number;
}

/**
 * Resolves a source-media timestamp (`sourceUs`, in `media`'s own
 * timebase — e.g. a `SmartEditRecommendation.start_us` or a
 * `Highlight.start_us`) to a real timeline position: the first on-timeline
 * clip using `media` whose trimmed source range covers `sourceUs`,
 * preferring `preferClipId` (when given and it qualifies) the same way
 * `transcriptEditor`'s resolver prefers its anchor clip. Returns `null`
 * when no clip currently on the timeline covers that moment (e.g. it was
 * trimmed off every instance) — callers should surface that rather than
 * silently doing nothing.
 */
export function resolveTimelinePositionForMedia(
  media: MediaItem,
  sourceUs: number,
  preferClipId?: string | null,
): ResolvedTimelinePosition | null {
  const candidates = timeline.clips.filter((c) => c.media_id === media.id);
  const preferred = preferClipId ? candidates.find((c) => c.id === preferClipId) : undefined;
  const ordered = preferred ? [preferred, ...candidates.filter((c) => c.id !== preferred.id)] : candidates;
  for (const clip of ordered) {
    if (sourceUs >= clip.source_in_us && sourceUs < clip.source_out_us) {
      const speed = clip.speed > 0 ? clip.speed : 1;
      const playheadUs = clip.position_us + Math.round((sourceUs - clip.source_in_us) / speed);
      return { clip, playheadUs };
    }
  }
  return null;
}
