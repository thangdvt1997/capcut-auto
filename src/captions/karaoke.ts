// Pure functions backing the karaoke/active-word caption rendering model
// (master prompt §27). Mirrors `src-tauri/src/project/types.rs`'s `Caption`
// doc comment: `Caption::words` is guaranteed time-ordered and non-overlapping
// by every backend producer (generation, split, merge, retime), so the
// active-word-at-time-T lookup here is a real O(log n) binary search, not a
// linear scan — the per-word cost §27 explicitly warns against.
//
// Deliberately kept out of `stores/captions.svelte.ts` (which owns the
// reactive `$derived` wiring) so this logic is plain, synchronous,
// unit-testable-in-spirit TypeScript with no Svelte runtime dependency.

import type { Caption, Track } from "../types/bindings";

/**
 * Binary search over one caption's `words` for the index of the word whose
 * `[start_us, end_us)` span contains `us`. Returns `-1` when `us` falls
 * before the first word, after the last word, or in a gap between two words
 * (captions may have brief silent gaps between words) — the caller then
 * renders the caption's plain text with no word highlighted, which is a
 * legitimate "no word is currently being spoken" state, not an error.
 */
export function findActiveWordIndex(words: Caption["words"], us: number): number {
  let lo = 0;
  let hi = words.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const word = words[mid]!;
    if (us < word.start_us) {
      hi = mid - 1;
    } else if (us >= word.end_us) {
      lo = mid + 1;
    } else {
      return mid;
    }
  }
  return -1;
}

/**
 * Finds the `Caption` active at `us`, if any. Captions are few per project
 * (unlike words within one caption), so a linear scan here is fine — see
 * this module's doc comment and master prompt §27: the efficiency
 * requirement is specifically about per-word lookups, not per-caption ones.
 * Captions on a hidden track are skipped (mirrors
 * `stores/timeline.svelte.ts`'s `activeVideoTarget` treatment of hidden
 * video tracks) so toggling a caption track's visibility actually hides its
 * karaoke overlay too.
 */
export function findActiveCaption(
  captions: readonly Caption[],
  hiddenTrackIds: ReadonlySet<string>,
  us: number,
): Caption | null {
  for (const caption of captions) {
    if (hiddenTrackIds.has(caption.track_id)) continue;
    if (us >= caption.start_us && us < caption.end_us) return caption;
  }
  return null;
}

/** Convenience wrapper building the hidden-track-id set `findActiveCaption`
 * needs directly from the track list, so call sites don't repeat the
 * `.filter(...).map(...)` themselves. */
export function hiddenTrackIdSet(tracks: readonly Track[]): ReadonlySet<string> {
  return new Set(tracks.filter((t) => t.hidden).map((t) => t.id));
}
