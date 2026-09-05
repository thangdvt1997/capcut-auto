// Unit tests for `src/timeline/algebra.ts` — the pure, framework-free
// timeline math layer. Chosen first per `IMPLEMENTATION_PLAN.md` Phase 13's
// "Frontend component/store/timeline-operation tests" bullet: this module
// has zero Svelte/Tauri dependency, so every exported function is testable
// with plain inputs/outputs and no mocking at all.

import { describe, expect, it } from "vitest";
import type { Clip, ProjectV1, Track } from "../types/bindings";
import {
  clampZoom,
  clipContainsUs,
  clipEndUs,
  clipsInSelectionRange,
  clipTimelineDurationUs,
  collectSnapCandidates,
  DEFAULT_SNAP_THRESHOLD_US,
  formatTimecode,
  MAX_PX_PER_SECOND,
  MIN_PX_PER_SECOND,
  projectDurationUs,
  pxToUs,
  rulerTicks,
  tickIntervalSec,
  usToPx,
  usToSec,
  secToUs,
  viewportFromScroll,
  visibleClips,
} from "./algebra";

// -----------------------------------------------------------------------
// Fixture helpers
// -----------------------------------------------------------------------

function makeClip(overrides: Partial<Clip> = {}): Clip {
  return {
    id: "clip-1",
    track_id: "track-1",
    media_id: "media-1",
    source_in_us: 0,
    source_out_us: 5_000_000,
    position_us: 0,
    speed: 1,
    enabled: true,
    group_id: null,
    clip_settings: {
      opacity: 1,
      flip_h: false,
      flip_v: false,
      rotation_deg: 0,
      scale_x: 1,
      scale_y: 1,
      transform_x: 0,
      transform_y: 0,
    },
    ...overrides,
  };
}

function makeTrack(overrides: Partial<Track> = {}): Track {
  return {
    id: "track-1",
    kind: "video",
    name: "V1",
    render_index: 1,
    locked: false,
    hidden: false,
    muted: false,
    solo: false,
    clip_ids: [],
    ...overrides,
  };
}

function makeProject(clips: Clip[], tracks: Track[] = [makeTrack()]): ProjectV1 {
  return {
    version: 1,
    project: { name: "test", created_at: "", modified_at: "" } as ProjectV1["project"],
    canvas: {
      width: 1080,
      height: 1920,
      fps: { num: 30, den: 1 },
      ratio_preset: "9:16",
    },
    media: [],
    tracks,
    clips,
    captions: [],
    caption_styles: [],
    transcript: [],
    effects: [],
    animations: [],
    keyframes: [],
    cuts: [],
    ai: {} as ProjectV1["ai"],
    export: {} as ProjectV1["export"],
    sync_groups: [],
    audio_clip_settings: {},
    audio_track_roles: {},
    track_ducking: {},
  };
}

// -----------------------------------------------------------------------
// usToSec / secToUs
// -----------------------------------------------------------------------

describe("usToSec / secToUs", () => {
  it("round-trips whole seconds", () => {
    expect(usToSec(5_000_000)).toBe(5);
    expect(secToUs(5)).toBe(5_000_000);
  });

  it("rounds fractional-microsecond results instead of leaving float drift", () => {
    // 1/3 second in microseconds is not an integer; secToUs must round it.
    expect(secToUs(1 / 3)).toBe(Math.round((1 / 3) * 1_000_000));
    expect(Number.isInteger(secToUs(1 / 3))).toBe(true);
  });
});

// -----------------------------------------------------------------------
// clampZoom
// -----------------------------------------------------------------------

describe("clampZoom", () => {
  it("passes through an in-range value unchanged", () => {
    expect(clampZoom(60)).toBe(60);
  });

  it("clamps below MIN_PX_PER_SECOND up to the minimum", () => {
    expect(clampZoom(0)).toBe(MIN_PX_PER_SECOND);
    expect(clampZoom(-100)).toBe(MIN_PX_PER_SECOND);
  });

  it("clamps above MAX_PX_PER_SECOND down to the maximum", () => {
    expect(clampZoom(1_000_000)).toBe(MAX_PX_PER_SECOND);
  });

  it("holds exactly at both boundary values", () => {
    expect(clampZoom(MIN_PX_PER_SECOND)).toBe(MIN_PX_PER_SECOND);
    expect(clampZoom(MAX_PX_PER_SECOND)).toBe(MAX_PX_PER_SECOND);
  });
});

// -----------------------------------------------------------------------
// usToPx / pxToUs
// -----------------------------------------------------------------------

describe("usToPx / pxToUs", () => {
  it("converts microseconds to pixels at a given zoom", () => {
    expect(usToPx(1_000_000, 60)).toBe(60);
    expect(usToPx(500_000, 60)).toBe(30);
  });

  it("converts pixels back to microseconds, rounding to an integer", () => {
    expect(pxToUs(60, 60)).toBe(1_000_000);
    // 1px at 60px/s is 16666.66us -> rounds to an integer.
    expect(pxToUs(1, 60)).toBe(Math.round((1 / 60) * 1_000_000));
    expect(Number.isInteger(pxToUs(1, 60))).toBe(true);
  });
});

// -----------------------------------------------------------------------
// clipTimelineDurationUs / clipEndUs
// -----------------------------------------------------------------------

describe("clipTimelineDurationUs", () => {
  it("is the trimmed source span at 1x speed", () => {
    const clip = makeClip({ source_in_us: 1_000_000, source_out_us: 3_000_000, speed: 1 });
    expect(clipTimelineDurationUs(clip)).toBe(2_000_000);
  });

  it("halves the timeline span at 2x speed", () => {
    const clip = makeClip({ source_in_us: 0, source_out_us: 2_000_000, speed: 2 });
    expect(clipTimelineDurationUs(clip)).toBe(1_000_000);
  });

  it("doubles the timeline span at 0.5x speed", () => {
    const clip = makeClip({ source_in_us: 0, source_out_us: 1_000_000, speed: 0.5 });
    expect(clipTimelineDurationUs(clip)).toBe(2_000_000);
  });

  it("treats a non-positive speed as 1x rather than dividing by zero/negative", () => {
    const clip = makeClip({ source_in_us: 0, source_out_us: 1_000_000, speed: 0 });
    expect(clipTimelineDurationUs(clip)).toBe(1_000_000);
    const negative = makeClip({ source_in_us: 0, source_out_us: 1_000_000, speed: -1 });
    expect(clipTimelineDurationUs(negative)).toBe(1_000_000);
  });

  it("never returns a negative duration for an inverted source range", () => {
    const clip = makeClip({ source_in_us: 5_000_000, source_out_us: 1_000_000 });
    expect(clipTimelineDurationUs(clip)).toBe(0);
  });
});

describe("clipEndUs", () => {
  it("is position plus timeline duration", () => {
    const clip = makeClip({ position_us: 10_000_000, source_in_us: 0, source_out_us: 2_000_000 });
    expect(clipEndUs(clip)).toBe(12_000_000);
  });
});

// -----------------------------------------------------------------------
// viewportFromScroll / visibleClips
// -----------------------------------------------------------------------

describe("viewportFromScroll", () => {
  it("derives start/end microseconds from scroll position and width", () => {
    const viewport = viewportFromScroll(60, 600, 60);
    expect(viewport.startUs).toBe(1_000_000);
    expect(viewport.endUs).toBe(11_000_000);
  });

  it("clamps a negative viewport width to zero rather than inverting the range", () => {
    const viewport = viewportFromScroll(0, -100, 60);
    expect(viewport.startUs).toBe(0);
    expect(viewport.endUs).toBe(0);
  });
});

describe("visibleClips", () => {
  const clips = [
    makeClip({ id: "a", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 }),
    makeClip({ id: "b", position_us: 5_000_000, source_in_us: 0, source_out_us: 1_000_000 }),
    makeClip({ id: "c", position_us: 20_000_000, source_in_us: 0, source_out_us: 1_000_000 }),
  ];

  it("includes only clips intersecting the viewport", () => {
    const result = visibleClips(clips, { startUs: 4_000_000, endUs: 7_000_000 });
    expect(result.map((c) => c.id)).toEqual(["b"]);
  });

  it("expands the intersection test by overscanUs on both sides", () => {
    // Without overscan, viewport [8M,15M] catches neither b ([5M,6M]) nor c
    // ([20M,21M]). A 3M overscan expands it to [5M,18M], which pulls in b
    // (its end, 6M, is now just inside the expanded start) but still not c.
    const result = visibleClips(clips, { startUs: 8_000_000, endUs: 15_000_000 }, 3_000_000);
    expect(result.map((c) => c.id)).toEqual(["b"]);
  });

  it("excludes a clip that only touches the viewport edge (exclusive end)", () => {
    // clip a ends exactly at viewport.startUs=1_000_000 -> clipEnd > start is false.
    const result = visibleClips(clips, { startUs: 1_000_000, endUs: 2_000_000 });
    expect(result.map((c) => c.id)).toEqual([]);
  });
});

// -----------------------------------------------------------------------
// collectSnapCandidates
// -----------------------------------------------------------------------

describe("collectSnapCandidates", () => {
  it("always includes zero and the playhead", () => {
    const result = collectSnapCandidates([], 3_000_000);
    expect(result.sort((a, b) => a - b)).toEqual([0, 3_000_000]);
  });

  it("includes every clip's start and end", () => {
    const clips = [
      makeClip({ id: "a", position_us: 1_000_000, source_in_us: 0, source_out_us: 2_000_000 }),
      makeClip({ id: "b", position_us: 5_000_000, source_in_us: 0, source_out_us: 1_000_000 }),
    ];
    const result = collectSnapCandidates(clips, 0);
    expect(result.sort((a, b) => a - b)).toEqual([0, 1_000_000, 3_000_000, 5_000_000, 6_000_000]);
  });

  it("excludes the clip currently being edited (excludeClipId)", () => {
    const clips = [makeClip({ id: "a", position_us: 1_000_000, source_in_us: 0, source_out_us: 2_000_000 })];
    const result = collectSnapCandidates(clips, 0, "a");
    expect(result.sort((a, b) => a - b)).toEqual([0]);
  });

  it("de-duplicates candidates that land on the same microsecond", () => {
    const clips = [makeClip({ id: "a", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 })];
    // playhead exactly at the clip start (0) and clip end (1_000_000 below, added separately)
    const result = collectSnapCandidates(clips, 0);
    expect(result.sort((a, b) => a - b)).toEqual([0, 1_000_000]);
  });
});

describe("DEFAULT_SNAP_THRESHOLD_US", () => {
  it("is 150ms", () => {
    expect(DEFAULT_SNAP_THRESHOLD_US).toBe(150_000);
  });
});

// -----------------------------------------------------------------------
// tickIntervalSec / rulerTicks
// -----------------------------------------------------------------------

describe("tickIntervalSec", () => {
  it("picks the smallest nice interval whose on-screen spacing meets targetPx", () => {
    // At 90px/s, a 1-second tick is already 90px wide -> exactly meets target.
    expect(tickIntervalSec(90)).toBe(1);
  });

  it("falls back to the largest interval when even the largest is below targetPx", () => {
    expect(tickIntervalSec(0.0001)).toBe(7200);
  });

  it("picks a sub-second interval when zoomed in far enough", () => {
    // At 2000px/s (max zoom), 1/30s already spans 66.67px; need >= 90px target,
    // so it should pick a coarser-than-1/30 but still sub-second interval.
    const interval = tickIntervalSec(2000);
    expect(interval * 2000).toBeGreaterThanOrEqual(90);
  });
});

describe("rulerTicks", () => {
  it("returns an empty array when the viewport's first tick already lands past its own end", () => {
    // An inverted/empty viewport (startUs far past endUs): the first
    // candidate tick, floored from startUs, is already beyond endUs, so the
    // generation loop never runs.
    const ticks = rulerTicks({ startUs: 10_000_000, endUs: 1_000_000 }, 60);
    expect(ticks).toEqual([]);
  });

  it("marks every 5th tick as major, starting from the first", () => {
    const ticks = rulerTicks({ startUs: 0, endUs: 10_000_000 }, 90);
    expect(ticks.length).toBeGreaterThan(5);
    expect(ticks[0]!.major).toBe(true);
    expect(ticks[1]!.major).toBe(false);
    expect(ticks[5]!.major).toBe(true);
  });

  it("never includes a negative timestamp even when the viewport starts negative", () => {
    const ticks = rulerTicks({ startUs: -5_000_000, endUs: 3_000_000 }, 90);
    for (const tick of ticks) {
      expect(tick.us).toBeGreaterThanOrEqual(0);
    }
  });
});

// -----------------------------------------------------------------------
// formatTimecode
// -----------------------------------------------------------------------

describe("formatTimecode", () => {
  it("formats zero as 0:00.00", () => {
    expect(formatTimecode(0)).toBe("0:00.00");
  });

  it("formats a sub-second value with centiseconds", () => {
    expect(formatTimecode(1_230_000)).toBe("0:01.23");
  });

  it("formats a value under a minute without a leading minute/hour", () => {
    expect(formatTimecode(45_670_000)).toBe("0:45.67");
  });

  it("formats a value right at the 1-minute rollover", () => {
    expect(formatTimecode(60_000_000)).toBe("1:00");
  });

  it("formats a value just under a minute vs. just at it distinctly", () => {
    expect(formatTimecode(59_990_000)).toBe("0:59.99");
    expect(formatTimecode(60_000_000)).toBe("1:00");
  });

  it("formats minutes:seconds under an hour", () => {
    expect(formatTimecode(125_000_000)).toBe("2:05");
  });

  it("formats a value right at the 1-hour rollover with hh:mm:ss", () => {
    expect(formatTimecode(3_600_000_000)).toBe("1:00:00");
  });

  it("formats a value just under an hour vs. just at it distinctly", () => {
    expect(formatTimecode(3_599_000_000)).toBe("59:59");
    expect(formatTimecode(3_600_000_000)).toBe("1:00:00");
  });

  it("formats over an hour with padded minutes/seconds", () => {
    expect(formatTimecode(3_725_000_000)).toBe("1:02:05");
  });

  it("clamps a negative input to zero", () => {
    expect(formatTimecode(-5_000_000)).toBe("0:00.00");
  });
});

// -----------------------------------------------------------------------
// projectDurationUs
// -----------------------------------------------------------------------

describe("projectDurationUs", () => {
  it("floors at minimumUs for an empty project", () => {
    const project = makeProject([]);
    expect(projectDurationUs(project)).toBe(30_000_000);
  });

  it("respects a caller-supplied minimumUs", () => {
    const project = makeProject([]);
    expect(projectDurationUs(project, 5_000_000)).toBe(5_000_000);
  });

  it("is the single clip's end when above the minimum", () => {
    const clip = makeClip({ position_us: 0, source_in_us: 0, source_out_us: 40_000_000 });
    const project = makeProject([clip]);
    expect(projectDurationUs(project)).toBe(40_000_000);
  });

  it("is the max end across multiple overlapping tracks", () => {
    const clipA = makeClip({ id: "a", track_id: "t1", position_us: 0, source_in_us: 0, source_out_us: 10_000_000 });
    const clipB = makeClip({ id: "b", track_id: "t2", position_us: 5_000_000, source_in_us: 0, source_out_us: 40_000_000 });
    const project = makeProject([clipA, clipB], [makeTrack({ id: "t1" }), makeTrack({ id: "t2" })]);
    // clipB ends at 5_000_000 + 40_000_000 = 45_000_000
    expect(projectDurationUs(project)).toBe(45_000_000);
  });
});

// -----------------------------------------------------------------------
// clipContainsUs
// -----------------------------------------------------------------------

describe("clipContainsUs", () => {
  const clip = makeClip({ position_us: 1_000_000, source_in_us: 0, source_out_us: 2_000_000 }); // spans [1_000_000, 3_000_000)

  it("is true strictly inside the span", () => {
    expect(clipContainsUs(clip, 2_000_000)).toBe(true);
  });

  it("is false exactly at the start boundary (not strictly inside)", () => {
    expect(clipContainsUs(clip, 1_000_000)).toBe(false);
  });

  it("is false exactly at the end boundary", () => {
    expect(clipContainsUs(clip, 3_000_000)).toBe(false);
  });

  it("is false before the start or after the end", () => {
    expect(clipContainsUs(clip, 0)).toBe(false);
    expect(clipContainsUs(clip, 4_000_000)).toBe(false);
  });
});

// -----------------------------------------------------------------------
// Clip overlap / containment edge cases via visibleClips + clipsInSelectionRange
// -----------------------------------------------------------------------

describe("clip overlap/containment edge cases", () => {
  it("two clips touching but not overlapping (a ends exactly where b starts) do not both match a range at the touch point", () => {
    const a = makeClip({ id: "a", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 }); // [0, 1_000_000)
    const b = makeClip({ id: "b", position_us: 1_000_000, source_in_us: 0, source_out_us: 1_000_000 }); // [1_000_000, 2_000_000)
    const track = makeTrack();
    const range = { minTrackIndex: 0, maxTrackIndex: 0, minUs: 1_000_000, maxUs: 1_000_000 };
    // A zero-width range exactly at the touch point should match neither
    // (a's end is exclusive-outside via clipEnd <= range.minUs; b's start is
    // exclusive-outside via clipStart >= range.maxUs).
    expect(clipsInSelectionRange([a, b], [track], range)).toEqual([]);
  });

  it("one clip fully inside another's timeline span still both intersect the same selection range", () => {
    const outer = makeClip({ id: "outer", position_us: 0, source_in_us: 0, source_out_us: 10_000_000 });
    const inner = makeClip({ id: "inner", position_us: 2_000_000, source_in_us: 0, source_out_us: 1_000_000 });
    const track = makeTrack();
    const range = { minTrackIndex: 0, maxTrackIndex: 0, minUs: 2_500_000, maxUs: 3_000_000 };
    const result = clipsInSelectionRange([outer, inner], [track], range);
    expect(result.sort()).toEqual(["inner", "outer"]);
  });

  it("identical spans on the same track both match the same selection range", () => {
    const a = makeClip({ id: "a", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 });
    const b = makeClip({ id: "b", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 });
    const track = makeTrack();
    const range = { minTrackIndex: 0, maxTrackIndex: 0, minUs: 0, maxUs: 1_000_000 };
    expect(clipsInSelectionRange([a, b], [track], range).sort()).toEqual(["a", "b"]);
  });

  it("excludes clips outside the track-row range even when their time span matches", () => {
    const a = makeClip({ id: "a", track_id: "t1", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 });
    const tracks = [makeTrack({ id: "t1" }), makeTrack({ id: "t2" })];
    const range = { minTrackIndex: 1, maxTrackIndex: 1, minUs: 0, maxUs: 1_000_000 };
    expect(clipsInSelectionRange([a], tracks, range)).toEqual([]);
  });
});
