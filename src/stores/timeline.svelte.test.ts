// Store test for `stores/timeline.svelte.ts` — the second half of Phase
// 13's "at least one real store test with mocked Tauri commands"
// requirement, and the deliberate answer to the task brief's "verify for
// real whether Svelte 5 runes work in a plain `.svelte.ts` module under
// Vitest" question.
//
// Verified finding: YES, real Svelte 5 runes (`$state`/`$derived`/
// `$derived.by`) work correctly in a plain `.svelte.ts` module under Vitest,
// with no component/mount step at all — reading/writing `TimelineStore`'s
// class fields directly (outside any component) recomputes `$derived`
// values exactly like it would inside one. This only required:
//   1. `vitest.config.ts` running the real `@sveltejs/vite-plugin-svelte`
//      plugin (so `.svelte.ts` files are compiled in "runes mode" at all),
//      and
//   2. `resolve.conditions: ["browser"]` in that config, so Node/Vitest's
//      module resolution picks Svelte's reactive client build instead of
//      its non-reactive server-rendering stub.
// No workaround, wrapper component, or `@testing-library/svelte` mount was
// needed for this store's `$state`/`$derived` logic specifically (it never
// uses `$effect`, which is a real, separate case — a `$effect` genuinely
// does need a running effect root, e.g. via `$effect.root(...)`, to fire at
// all; this store has none, so that limitation doesn't apply here and isn't
// exercised by this file).
//
// `commands` (from `../types/bindings`) is mocked so no real Tauri backend
// is needed — only the specific commands these tests actually call
// (`getTimelineProject`, `effectiveTrackMuteState`) are stubbed.

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Clip, ProjectV1, Track } from "../types/bindings";

const getTimelineProject = vi.fn();
const effectiveTrackMuteState = vi.fn();

vi.mock("../types/bindings", () => ({
  commands: {
    getTimelineProject: (...args: unknown[]) => getTimelineProject(...args),
    effectiveTrackMuteState: (...args: unknown[]) => effectiveTrackMuteState(...args),
  },
}));

const { timeline } = await import("./timeline.svelte");

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

function makeMediaVideo(id = "media-1") {
  return {
    id,
    kind: "video" as const,
    duration_us: 10_000_000,
  } as ProjectV1["media"][number];
}

function makeProject(overrides: Partial<ProjectV1> = {}): ProjectV1 {
  return {
    version: 1,
    project: {} as ProjectV1["project"],
    canvas: {} as ProjectV1["canvas"],
    media: [],
    tracks: [],
    clips: [],
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
    ...overrides,
  };
}

beforeEach(() => {
  getTimelineProject.mockReset();
  effectiveTrackMuteState.mockReset();
  effectiveTrackMuteState.mockResolvedValue({ status: "ok", data: {} });
  timeline.project = null;
  timeline.selectedClipIds = new Set();
  timeline.playheadUs = 0;
  timeline.effectiveMute = {};
  timeline.lastError = null;
});

describe("TimelineStore — $derived clipsByTrack / clips / tracks / media", () => {
  it("is empty when no project is loaded", () => {
    expect(timeline.clips).toEqual([]);
    expect(timeline.tracks).toEqual([]);
    expect(timeline.clipsByTrack.size).toBe(0);
  });

  it("groups clips by track id once a project is assigned", () => {
    const trackA = makeTrack({ id: "ta" });
    const trackB = makeTrack({ id: "tb" });
    const clip1 = makeClip({ id: "c1", track_id: "ta" });
    const clip2 = makeClip({ id: "c2", track_id: "ta" });
    const clip3 = makeClip({ id: "c3", track_id: "tb" });
    timeline.project = makeProject({ tracks: [trackA, trackB], clips: [clip1, clip2, clip3] });

    expect(timeline.clips.map((c) => c.id).sort()).toEqual(["c1", "c2", "c3"]);
    expect(timeline.clipsByTrack.get("ta")?.map((c) => c.id).sort()).toEqual(["c1", "c2"]);
    expect(timeline.clipsByTrack.get("tb")?.map((c) => c.id)).toEqual(["c3"]);
  });

  it("recomputes clipsByTrack reactively after reassigning the project ($derived.by re-runs on read)", () => {
    timeline.project = makeProject({ tracks: [makeTrack({ id: "ta" })], clips: [makeClip({ id: "c1", track_id: "ta" })] });
    expect(timeline.clipsByTrack.get("ta")?.length).toBe(1);

    timeline.project = makeProject({
      tracks: [makeTrack({ id: "ta" })],
      clips: [makeClip({ id: "c1", track_id: "ta" }), makeClip({ id: "c2", track_id: "ta" })],
    });
    expect(timeline.clipsByTrack.get("ta")?.length).toBe(2);
  });
});

describe("TimelineStore — $derived durationUs", () => {
  it("defaults to 30s floor when no project is loaded", () => {
    expect(timeline.durationUs).toBe(30_000_000);
  });

  it("reflects the loaded project's real duration", () => {
    const track = makeTrack({ id: "ta" });
    const clip = makeClip({ id: "c1", track_id: "ta", position_us: 0, source_in_us: 0, source_out_us: 45_000_000 });
    timeline.project = makeProject({ tracks: [track], clips: [clip] });
    expect(timeline.durationUs).toBe(45_000_000);
  });
});

describe("TimelineStore — $derived activeVideoTarget", () => {
  it("is null when no project is loaded", () => {
    expect(timeline.activeVideoTarget).toBeNull();
  });

  it("is null when the playhead is outside every clip's span", () => {
    const track = makeTrack({ id: "ta", kind: "video" });
    const clip = makeClip({ id: "c1", track_id: "ta", media_id: "m1", position_us: 0, source_in_us: 0, source_out_us: 1_000_000 });
    timeline.project = makeProject({ tracks: [track], clips: [clip], media: [makeMediaVideo("m1")] });
    timeline.playheadUs = 5_000_000;
    expect(timeline.activeVideoTarget).toBeNull();
  });

  it("resolves the media and source time under the playhead", () => {
    const track = makeTrack({ id: "ta", kind: "video" });
    const clip = makeClip({ id: "c1", track_id: "ta", media_id: "m1", position_us: 1_000_000, source_in_us: 2_000_000, source_out_us: 6_000_000 });
    timeline.project = makeProject({ tracks: [track], clips: [clip], media: [makeMediaVideo("m1")] });
    timeline.playheadUs = 1_500_000; // 500_000 into the clip
    const result = timeline.activeVideoTarget;
    expect(result).not.toBeNull();
    expect(result?.media.id).toBe("m1");
    expect(result?.sourceTimeUs).toBe(2_500_000); // source_in_us + 500_000
  });

  it("skips a hidden video track", () => {
    const track = makeTrack({ id: "ta", kind: "video", hidden: true });
    const clip = makeClip({ id: "c1", track_id: "ta", media_id: "m1", position_us: 0, source_in_us: 0, source_out_us: 5_000_000 });
    timeline.project = makeProject({ tracks: [track], clips: [clip], media: [makeMediaVideo("m1")] });
    timeline.playheadUs = 1_000_000;
    expect(timeline.activeVideoTarget).toBeNull();
  });

  it("prefers the track with the higher render_index when two video tracks both qualify", () => {
    const trackLow = makeTrack({ id: "low", kind: "video", render_index: 0 });
    const trackHigh = makeTrack({ id: "high", kind: "video", render_index: 5 });
    const clipLow = makeClip({ id: "cl", track_id: "low", media_id: "mlow", position_us: 0, source_in_us: 0, source_out_us: 5_000_000 });
    const clipHigh = makeClip({ id: "ch", track_id: "high", media_id: "mhigh", position_us: 0, source_in_us: 0, source_out_us: 5_000_000 });
    timeline.project = makeProject({
      tracks: [trackLow, trackHigh],
      clips: [clipLow, clipHigh],
      media: [makeMediaVideo("mlow"), makeMediaVideo("mhigh")],
    });
    timeline.playheadUs = 1_000_000;
    expect(timeline.activeVideoTarget?.media.id).toBe("mhigh");
  });
});

describe("TimelineStore — selection", () => {
  beforeEach(() => {
    const track = makeTrack({ id: "ta" });
    timeline.project = makeProject({
      tracks: [track],
      clips: [makeClip({ id: "c1", track_id: "ta" }), makeClip({ id: "c2", track_id: "ta" })],
    });
  });

  it("selectClip replaces the selection by default", () => {
    timeline.selectClip("c1");
    timeline.selectClip("c2");
    expect(Array.from(timeline.selectedClipIds)).toEqual(["c2"]);
  });

  it("selectClip with additive:true toggles membership", () => {
    timeline.selectClip("c1", { additive: true });
    timeline.selectClip("c2", { additive: true });
    expect(new Set(timeline.selectedClipIds)).toEqual(new Set(["c1", "c2"]));
    timeline.selectClip("c1", { additive: true });
    expect(new Set(timeline.selectedClipIds)).toEqual(new Set(["c2"]));
  });

  it("selectedClips derives the real Clip objects from selectedClipIds", () => {
    timeline.setSelection(["c1", "c2"]);
    expect(timeline.selectedClips.map((c) => c.id).sort()).toEqual(["c1", "c2"]);
  });

  it("clearSelection empties the selection", () => {
    timeline.setSelection(["c1"]);
    timeline.clearSelection();
    expect(timeline.selectedClipIds.size).toBe(0);
  });
});

describe("TimelineStore — refresh() (mocked commands.getTimelineProject)", () => {
  it("loads the project returned by the backend and folds it into $state", async () => {
    const track = makeTrack({ id: "ta" });
    const project = makeProject({ tracks: [track], clips: [makeClip({ id: "c1", track_id: "ta" })] });
    getTimelineProject.mockResolvedValue({ status: "ok", data: project });

    await timeline.refresh();

    expect(timeline.project).toEqual(project);
    expect(timeline.clips.map((c) => c.id)).toEqual(["c1"]);
    expect(timeline.lastError).toBeNull();
  });

  it("sets lastError and leaves project unset on a backend error", async () => {
    getTimelineProject.mockResolvedValue({ status: "error", error: { message: "no active session" } });

    await timeline.refresh();

    expect(timeline.project).toBeNull();
    expect(timeline.lastError).toBe("no active session");
  });

  it("prunes selected clip ids that no longer exist in the refreshed project", async () => {
    timeline.selectedClipIds = new Set(["stale-id"]);
    const project = makeProject({ tracks: [], clips: [] });
    getTimelineProject.mockResolvedValue({ status: "ok", data: project });

    await timeline.refresh();

    expect(timeline.selectedClipIds.size).toBe(0);
  });
});
