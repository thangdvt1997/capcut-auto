// Pure, framework-free timeline algebra: time<->pixel conversion, viewport
// visibility (for virtualization, master prompt §50), snap-candidate
// collection, ruler tick generation, and small geometry helpers shared by
// `src/components/timeline/*`.
//
// No Svelte imports on purpose — this module is unit-testable in isolation.
// It is a fresh design "informed by" (not copied from) autocut's `cuts.ts`
// module per `docs/upstream.md`/audit §7's license-gate note, and rewritten
// against this project's mandated i64-microsecond timebase: every function
// here takes/returns integer microseconds, never float seconds. The only
// place float seconds legitimately appear is at the `<video>` element
// boundary (`usToSec`/`secToUs`), mirroring the FFmpeg/FCPXML conversion
// boundaries documented in `docs/architecture.md`.

import type { Clip, ProjectV1, Track } from "../types/bindings";

/** Integer microseconds. Never store a fractional value in a variable of
 * this type — round at every conversion boundary instead of letting float
 * drift accumulate (master prompt §10's "avoid floating-point drift"). */
export type Us = number;

export const MICROS_PER_SECOND = 1_000_000;

export function usToSec(us: Us): number {
  return us / MICROS_PER_SECOND;
}

export function secToUs(sec: number): Us {
  return Math.round(sec * MICROS_PER_SECOND);
}

/** Pixels-per-second — the store's zoom state *is* this value directly
 * (not an abstract step index), so zoom controls can scale it directly. */
export type PxPerSecond = number;

export const MIN_PX_PER_SECOND = 2;
export const MAX_PX_PER_SECOND = 2000;
export const DEFAULT_PX_PER_SECOND = 60;
export const ZOOM_STEP_FACTOR = 1.4;

export function clampZoom(pxPerSecond: PxPerSecond): PxPerSecond {
  return Math.min(MAX_PX_PER_SECOND, Math.max(MIN_PX_PER_SECOND, pxPerSecond));
}

export function usToPx(us: Us, pxPerSecond: PxPerSecond): number {
  return (us / MICROS_PER_SECOND) * pxPerSecond;
}

export function pxToUs(px: number, pxPerSecond: PxPerSecond): Us {
  return Math.round((px / pxPerSecond) * MICROS_PER_SECOND);
}

/** Fixed row height shared by `Timeline.svelte`/`TrackHeader.svelte`/
 * `ClipView.svelte` so marquee-selection math (pixel Y -> track index) and
 * the rendered rows agree exactly. A plain UI layout constant, not a video
 * algebra concept, but kept here so every geometry number lives in one
 * framework-free place. */
export const TRACK_ROW_HEIGHT_PX = 56;

/** Timeline-span duration of a clip, in microseconds: how long it occupies
 * on the track, accounting for playback `speed` (a 2x-speed clip occupies
 * half the timeline span of the same trimmed source range). */
export function clipTimelineDurationUs(clip: Clip): Us {
  const sourceSpan = clip.source_out_us - clip.source_in_us;
  const speed = clip.speed > 0 ? clip.speed : 1;
  return Math.max(0, Math.round(sourceSpan / speed));
}

export function clipEndUs(clip: Clip): Us {
  return clip.position_us + clipTimelineDurationUs(clip);
}

export interface Viewport {
  startUs: Us;
  endUs: Us;
}

export function viewportFromScroll(
  scrollLeftPx: number,
  viewportWidthPx: number,
  pxPerSecond: PxPerSecond,
): Viewport {
  return {
    startUs: pxToUs(scrollLeftPx, pxPerSecond),
    endUs: pxToUs(scrollLeftPx + Math.max(0, viewportWidthPx), pxPerSecond),
  };
}

/**
 * Clips whose timeline span intersects `viewport`, expanded by `overscanUs`
 * on both sides so a clip just outside the visible area is still mounted
 * (avoids pop-in while scrolling/zooming) without rendering the entire
 * timeline (master prompt §50 virtualization requirement).
 */
export function visibleClips(clips: Clip[], viewport: Viewport, overscanUs: Us = 0): Clip[] {
  const start = viewport.startUs - overscanUs;
  const end = viewport.endUs + overscanUs;
  return clips.filter((c) => {
    const clipEnd = clipEndUs(c);
    return clipEnd > start && c.position_us < end;
  });
}

/**
 * Snap candidates for a drag/trim/playhead move: every other clip's start
 * and end (across the tracks the caller passes in — the frontend owns this
 * UI-level policy per `commands::timeline::snap_to_candidates`'s doc
 * comment), plus the playhead and time zero. `excludeClipId` omits the clip
 * currently being edited so it never snaps to its own edges.
 */
export function collectSnapCandidates(clips: Clip[], playheadUs: Us, excludeClipId?: string): Us[] {
  const out = new Set<Us>([0, playheadUs]);
  for (const c of clips) {
    if (c.id === excludeClipId) continue;
    out.add(c.position_us);
    out.add(clipEndUs(c));
  }
  return Array.from(out);
}

/** ~150ms: perceptible as "it snapped" but forgiving enough to still be
 * useful across most zoom levels; the store may pass a tighter/looser value
 * derived from the current zoom if that ever proves necessary. */
export const DEFAULT_SNAP_THRESHOLD_US = 150_000;

/** "Nice" ruler tick intervals, in whole/fractional seconds, chosen so a
 * tick's on-screen spacing stays legible across the zoom range — the same
 * kind of table every NLE ruler uses. */
const NICE_INTERVALS_SEC = [
  1 / 30, 1 / 10, 1 / 5, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200,
];

export function tickIntervalSec(pxPerSecond: PxPerSecond, targetPx = 90): number {
  for (const interval of NICE_INTERVALS_SEC) {
    if (interval * pxPerSecond >= targetPx) return interval;
  }
  return NICE_INTERVALS_SEC[NICE_INTERVALS_SEC.length - 1] as number;
}

export interface RulerTick {
  us: Us;
  label: string;
  major: boolean;
}

/**
 * Ruler tick positions within `viewport` at the interval `tickIntervalSec`
 * picks for `pxPerSecond`. Every 5th tick is flagged `major` (a taller
 * mark with a label in `Ruler.svelte`; minor ticks are unlabeled hairlines)
 * so the ruler doesn't get a label at literally every tick once zoomed in.
 */
export function rulerTicks(viewport: Viewport, pxPerSecond: PxPerSecond, targetPx = 90): RulerTick[] {
  const interval = tickIntervalSec(pxPerSecond, targetPx);
  const intervalUs = Math.round(interval * MICROS_PER_SECOND);
  if (intervalUs <= 0) return [];
  const startTick = Math.floor(Math.max(0, viewport.startUs) / intervalUs) * intervalUs;
  const ticks: RulerTick[] = [];
  let index = 0;
  for (let us = startTick; us <= viewport.endUs; us += intervalUs, index++) {
    if (us < 0) continue;
    ticks.push({ us, label: formatTimecode(us), major: index % 5 === 0 });
  }
  return ticks;
}

/** `h:mm:ss` / `m:ss` / `0:ss.cc` depending on magnitude — matches the
 * compact style `VideoPlayer.svelte`'s `formatTime` already uses, just
 * operating on integer microseconds instead of float seconds. */
export function formatTimecode(us: Us): string {
  const clamped = Math.max(0, us);
  const totalMs = Math.round(clamped / 1000);
  const totalSec = Math.floor(totalMs / 1000);
  const centis = Math.floor((totalMs % 1000) / 10);
  const s = totalSec % 60;
  const totalMin = Math.floor(totalSec / 60);
  const m = totalMin % 60;
  const h = Math.floor(totalMin / 60);
  const pad = (n: number) => n.toString().padStart(2, "0");
  if (h > 0) return `${h}:${pad(m)}:${pad(s)}`;
  if (totalMin > 0) return `${m}:${pad(s)}`;
  return `0:${pad(s)}.${pad(centis)}`;
}

/** Timeline extent: the end of the last clip across every track, floored at
 * `minimumUs` so an empty/near-empty project still shows a scrollable ruler
 * instead of a single-pixel-wide timeline. */
export function projectDurationUs(project: ProjectV1, minimumUs: Us = 30_000_000): Us {
  let max = 0;
  for (const c of project.clips) {
    max = Math.max(max, clipEndUs(c));
  }
  return Math.max(max, minimumUs);
}

/** Whether `us` falls strictly inside `clip`'s timeline span (used to decide
 * whether a split-at-playhead applies to a given clip). */
export function clipContainsUs(clip: Clip, us: Us): boolean {
  return us > clip.position_us && us < clipEndUs(clip);
}

/** A marquee/selection-region span: an inclusive track-row index range plus
 * a time range, in the domain units `Timeline.svelte`'s marquee gesture
 * tracks directly (row index from pointerdown/move deltas divided by
 * `TRACK_ROW_HEIGHT_PX`, time via `pxToUs`) rather than raw pixel
 * rectangles — avoids re-deriving one coordinate space from another. */
export interface SelectionRange {
  minTrackIndex: number;
  maxTrackIndex: number;
  minUs: Us;
  maxUs: Us;
}

/** Clip ids whose track row (by index into `tracks`) and timeline span
 * intersect `range` — the selection-region (marquee) query. */
export function clipsInSelectionRange(clips: Clip[], tracks: Track[], range: SelectionRange): string[] {
  const trackRow = new Map(tracks.map((t, i) => [t.id, i]));
  const out: string[] = [];
  for (const clip of clips) {
    const row = trackRow.get(clip.track_id);
    if (row === undefined || row < range.minTrackIndex || row > range.maxTrackIndex) continue;
    const clipStart = clip.position_us;
    const clipEnd = clipEndUs(clip);
    if (clipEnd <= range.minUs || clipStart >= range.maxUs) continue;
    out.push(clip.id);
  }
  return out;
}

/** Throttles `fn` to at most once per animation frame — used for
 * scroll/drag handlers so a fast pointer move doesn't recompute the visible
 * clip list or issue IPC calls on every intermediate pixel (master prompt
 * §50: "do not render every timeline object continuously"). Only the most
 * recent call's arguments within a frame are used. */
export function throttleRaf<Args extends unknown[]>(fn: (...args: Args) => void): (...args: Args) => void {
  let scheduled = false;
  let lastArgs: Args | null = null;
  return (...args: Args) => {
    lastArgs = args;
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      if (lastArgs) fn(...lastArgs);
    });
  };
}
