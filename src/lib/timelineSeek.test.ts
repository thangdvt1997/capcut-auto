// Unit tests for `src/lib/timelineSeek.ts` — the shared source-media-time ->
// timeline-position resolver (Phase 10 follow-up). It reads the singleton
// `timeline` store's `clips` list directly, so the store module is mocked
// here (`vi.mock`) rather than driving the real Svelte-5-runes store through
// a full `loadProject`/Tauri round trip — this keeps the test a pure,
// synchronous unit test of `resolveTimelinePositionForMedia`'s own matching
// logic, not a re-test of the store itself (that's `stores/timeline.svelte.test.ts`).

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Clip, MediaItem } from "../types/bindings";

vi.mock("../stores/timeline.svelte", () => ({
  timeline: { clips: [] as Clip[] },
}));

import { timeline } from "../stores/timeline.svelte";
import { resolveTimelinePositionForMedia } from "./timelineSeek";

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

function makeMedia(overrides: Partial<MediaItem> = {}): MediaItem {
  return {
    id: "media-1",
    kind: "video",
    ...overrides,
  } as MediaItem;
}

describe("resolveTimelinePositionForMedia", () => {
  beforeEach(() => {
    timeline.clips = [];
  });

  it("returns null when no clip on the timeline uses this media", () => {
    const media = makeMedia({ id: "media-1" });
    expect(resolveTimelinePositionForMedia(media, 1_000_000)).toBeNull();
  });

  it("returns null when a matching clip exists but its trimmed range doesn't cover sourceUs", () => {
    timeline.clips = [makeClip({ media_id: "media-1", source_in_us: 0, source_out_us: 1_000_000 })];
    const media = makeMedia({ id: "media-1" });
    expect(resolveTimelinePositionForMedia(media, 2_000_000)).toBeNull();
  });

  it("maps a covered sourceUs to the clip's timeline position at 1x speed", () => {
    timeline.clips = [
      makeClip({ id: "c1", media_id: "media-1", position_us: 10_000_000, source_in_us: 1_000_000, source_out_us: 5_000_000, speed: 1 }),
    ];
    const media = makeMedia({ id: "media-1" });
    const result = resolveTimelinePositionForMedia(media, 2_000_000);
    expect(result).not.toBeNull();
    expect(result?.clip.id).toBe("c1");
    // sourceUs 2_000_000 is 1_000_000 past source_in_us (1_000_000) -> playhead = position + 1_000_000.
    expect(result?.playheadUs).toBe(11_000_000);
  });

  it("accounts for clip speed when mapping source time to playhead time", () => {
    timeline.clips = [
      makeClip({ id: "c1", media_id: "media-1", position_us: 0, source_in_us: 0, source_out_us: 4_000_000, speed: 2 }),
    ];
    const media = makeMedia({ id: "media-1" });
    // 2_000_000us into the source, at 2x speed, is 1_000_000us of timeline duration.
    const result = resolveTimelinePositionForMedia(media, 2_000_000);
    expect(result?.playheadUs).toBe(1_000_000);
  });

  it("prefers preferClipId when it qualifies, even if listed after another match", () => {
    timeline.clips = [
      makeClip({ id: "first", media_id: "media-1", position_us: 0, source_in_us: 0, source_out_us: 5_000_000 }),
      makeClip({ id: "preferred", media_id: "media-1", position_us: 100_000_000, source_in_us: 0, source_out_us: 5_000_000 }),
    ];
    const media = makeMedia({ id: "media-1" });
    const result = resolveTimelinePositionForMedia(media, 1_000_000, "preferred");
    expect(result?.clip.id).toBe("preferred");
    expect(result?.playheadUs).toBe(101_000_000);
  });

  it("falls back to another qualifying clip when preferClipId doesn't cover sourceUs", () => {
    timeline.clips = [
      makeClip({ id: "covers", media_id: "media-1", position_us: 0, source_in_us: 0, source_out_us: 5_000_000 }),
      makeClip({ id: "preferred-but-not-covering", media_id: "media-1", position_us: 100_000_000, source_in_us: 10_000_000, source_out_us: 12_000_000 }),
    ];
    const media = makeMedia({ id: "media-1" });
    const result = resolveTimelinePositionForMedia(media, 1_000_000, "preferred-but-not-covering");
    expect(result?.clip.id).toBe("covers");
  });

  it("ignores clips that use a different media id", () => {
    timeline.clips = [makeClip({ id: "other-media", media_id: "media-2", source_in_us: 0, source_out_us: 5_000_000 })];
    const media = makeMedia({ id: "media-1" });
    expect(resolveTimelinePositionForMedia(media, 1_000_000)).toBeNull();
  });
});
