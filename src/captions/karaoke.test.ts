// Unit tests for `src/captions/karaoke.ts` — the binary-search active-word
// lookup and linear active-caption scan backing the karaoke overlay (master
// prompt §27). Picked per IMPLEMENTATION_PLAN.md Phase 13: pure, framework-
// free, and explicitly designed (per its own doc comment) to be independently
// testable.

import { describe, expect, it } from "vitest";
import type { Caption, Track, Word } from "../types/bindings";
import { findActiveCaption, findActiveWordIndex, hiddenTrackIdSet } from "./karaoke";

function makeWord(start_us: number, end_us: number, text = "w"): Word {
  return { text, start_us, end_us, confidence: 1 };
}

function makeCaption(overrides: Partial<Caption> = {}): Caption {
  return {
    id: "cap-1",
    track_id: "track-1",
    start_us: 0,
    end_us: 1_000_000,
    text: "hello",
    words: [],
    style_id: null,
    ...overrides,
  };
}

function makeTrack(overrides: Partial<Track> = {}): Track {
  return {
    id: "track-1",
    kind: "caption",
    name: "CC",
    render_index: 0,
    locked: false,
    hidden: false,
    muted: false,
    solo: false,
    clip_ids: [],
    ...overrides,
  };
}

describe("findActiveWordIndex", () => {
  // Words: [0,100) [100,250) [250,400) [500,600) (note the 400-500 gap)
  const words: Word[] = [
    makeWord(0, 100, "a"),
    makeWord(100, 250, "b"),
    makeWord(250, 400, "c"),
    makeWord(500, 600, "d"),
  ];

  it("returns -1 for an empty words array", () => {
    expect(findActiveWordIndex([], 50)).toBe(-1);
  });

  it("returns -1 before the first word", () => {
    expect(findActiveWordIndex(words, -1)).toBe(-1);
  });

  it("returns -1 after the last word", () => {
    expect(findActiveWordIndex(words, 600)).toBe(-1);
    expect(findActiveWordIndex(words, 10_000)).toBe(-1);
  });

  it("returns -1 in a silent gap between two words", () => {
    expect(findActiveWordIndex(words, 450)).toBe(-1);
  });

  it("finds the word containing a mid-span timestamp", () => {
    expect(findActiveWordIndex(words, 150)).toBe(1);
  });

  it("is inclusive of a word's start boundary", () => {
    expect(findActiveWordIndex(words, 250)).toBe(2);
  });

  it("is exclusive of a word's end boundary (belongs to the next word/gap instead)", () => {
    // 100 is word b's start, and word a's end -> must resolve to b (index 1), not a (index 0).
    expect(findActiveWordIndex(words, 100)).toBe(1);
  });

  it("handles a single-word caption", () => {
    const single = [makeWord(10, 20)];
    expect(findActiveWordIndex(single, 15)).toBe(0);
    expect(findActiveWordIndex(single, 10)).toBe(0);
    expect(findActiveWordIndex(single, 20)).toBe(-1);
    expect(findActiveWordIndex(single, 9)).toBe(-1);
  });
});

describe("findActiveCaption", () => {
  const captionA = makeCaption({ id: "a", track_id: "t1", start_us: 0, end_us: 1_000_000 });
  const captionB = makeCaption({ id: "b", track_id: "t1", start_us: 1_000_000, end_us: 2_000_000 });

  it("returns null when no caption is active at the given time", () => {
    expect(findActiveCaption([captionA, captionB], new Set(), 5_000_000)).toBeNull();
  });

  it("resolves the boundary between two adjacent captions to the later one (start-inclusive)", () => {
    // captionA ends exactly where captionB starts (1_000_000): must resolve
    // to B (start-inclusive), not A (end-exclusive).
    const result = findActiveCaption([captionA, captionB], new Set(), 1_000_000);
    expect(result?.id).toBe("b");
  });

  it("resolves just before the boundary to the earlier caption", () => {
    const result = findActiveCaption([captionA, captionB], new Set(), 999_999);
    expect(result?.id).toBe("a");
  });

  it("returns the first match when two captions legitimately overlap", () => {
    const overlappingA = makeCaption({ id: "a", track_id: "t1", start_us: 0, end_us: 2_000_000 });
    const overlappingB = makeCaption({ id: "b", track_id: "t2", start_us: 1_000_000, end_us: 3_000_000 });
    const result = findActiveCaption([overlappingA, overlappingB], new Set(), 1_500_000);
    expect(result?.id).toBe("a");
  });

  it("skips a caption whose track is hidden", () => {
    const result = findActiveCaption([captionA], new Set(["t1"]), 500_000);
    expect(result).toBeNull();
  });

  it("falls through to a later, non-hidden caption when an earlier one's track is hidden", () => {
    const hiddenA = makeCaption({ id: "a", track_id: "t1", start_us: 0, end_us: 2_000_000 });
    const visibleB = makeCaption({ id: "b", track_id: "t2", start_us: 0, end_us: 2_000_000 });
    const result = findActiveCaption([hiddenA, visibleB], new Set(["t1"]), 500_000);
    expect(result?.id).toBe("b");
  });
});

describe("hiddenTrackIdSet", () => {
  it("collects only hidden tracks' ids", () => {
    const tracks = [makeTrack({ id: "t1", hidden: true }), makeTrack({ id: "t2", hidden: false })];
    const result = hiddenTrackIdSet(tracks);
    expect(result.has("t1")).toBe(true);
    expect(result.has("t2")).toBe(false);
  });

  it("returns an empty set when no tracks are hidden", () => {
    const tracks = [makeTrack({ id: "t1", hidden: false })];
    expect(hiddenTrackIdSet(tracks).size).toBe(0);
  });
});
