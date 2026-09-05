// Unit tests for `src/capcut/compat.ts` — the client-side "Export to CapCut"
// compatibility-warning computation (master prompt §31). Picked per
// IMPLEMENTATION_PLAN.md Phase 13: pure, synchronous, and checks real
// documented CapCut-adapter gaps (effects/animations passthrough,
// unsupported keyframe properties) per this module's own doc comment.

import { describe, expect, it } from "vitest";
import type { ProjectV1 } from "../types/bindings";
import { computeCapcutCompatWarnings } from "./compat";

/** A minimal, otherwise-empty `ProjectV1`-shaped fixture. Only the fields
 * `computeCapcutCompatWarnings` actually reads (`effects`/`animations`/
 * `keyframes`) vary between tests; everything else is a harmless stub. */
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

describe("computeCapcutCompatWarnings", () => {
  it("returns no warnings for a clean project with no effects/animations/keyframes", () => {
    expect(computeCapcutCompatWarnings(makeProject())).toEqual([]);
  });

  it("warns about effects with the real count when any are present", () => {
    const project = makeProject({
      effects: [
        { id: "e1", clip_id: "c1", kind: "blur", params: null },
        { id: "e2", clip_id: "c1", kind: "glow", params: null },
      ],
    });
    const warnings = computeCapcutCompatWarnings(project);
    expect(warnings).toContainEqual({ key: "capcutExport.warnEffects", params: { count: 2 } });
  });

  it("warns about animations with the real count when any are present", () => {
    const project = makeProject({
      animations: [{ id: "a1", clip_id: "c1", kind: "in", name: "Fade In", duration_us: 500_000 }],
    });
    const warnings = computeCapcutCompatWarnings(project);
    expect(warnings).toContainEqual({ key: "capcutExport.warnAnimations", params: { count: 1 } });
  });

  it("does not warn about a supported keyframe property", () => {
    const project = makeProject({
      keyframes: [
        { id: "k1", clip_id: "c1", property: "position_x", time_offset_us: 0, value: 10, curve: "linear" },
        { id: "k2", clip_id: "c1", property: "volume", time_offset_us: 1000, value: 0.5, curve: "linear" },
      ],
    });
    expect(computeCapcutCompatWarnings(project)).toEqual([]);
  });

  it("warns about unsupported keyframe properties, counting only those", () => {
    const project = makeProject({
      keyframes: [
        { id: "k1", clip_id: "c1", property: "position_x", time_offset_us: 0, value: 10, curve: "linear" },
        { id: "k2", clip_id: "c1", property: "saturation", time_offset_us: 1000, value: 0.5, curve: "linear" },
        { id: "k3", clip_id: "c1", property: "blur_radius", time_offset_us: 2000, value: 2, curve: "linear" },
      ],
    });
    const warnings = computeCapcutCompatWarnings(project);
    expect(warnings).toContainEqual({ key: "capcutExport.warnKeyframes", params: { count: 2 } });
  });

  it("produces all three warnings together when the project has every kind of gap", () => {
    const project = makeProject({
      effects: [{ id: "e1", clip_id: "c1", kind: "blur", params: null }],
      animations: [{ id: "a1", clip_id: "c1", kind: "in", name: "Fade In", duration_us: 500_000 }],
      keyframes: [{ id: "k1", clip_id: "c1", property: "unsupported_prop", time_offset_us: 0, value: 1, curve: "linear" }],
    });
    const warnings = computeCapcutCompatWarnings(project);
    expect(warnings).toHaveLength(3);
    expect(warnings.map((w) => w.key).sort()).toEqual(
      ["capcutExport.warnAnimations", "capcutExport.warnEffects", "capcutExport.warnKeyframes"].sort(),
    );
  });
});
