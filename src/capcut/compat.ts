// Best-effort, client-side feature-compatibility check for "Export to
// CapCut" (master prompt §31, `IMPLEMENTATION_PLAN.md` Phase 9's "feature-
// compatibility warnings when an edit can't map to CapCut" bullet).
//
// This is deliberately NOT exhaustive — it checks the current `ProjectV1`
// against the specific, *documented* gaps in the Rust CapCut adapter
// (`src-tauri/src/capcut/mod.rs`'s module doc comment "Deliberate scope
// reductions", `src-tauri/src/capcut/keyframe.rs`'s
// `KeyframeProperty::from_project_property`), rather than trying to predict
// every possible mismatch:
//
// - `project.effects`/`project.animations` carry through the adapter as an
//   honest structural passthrough (`add_sticker`/`add_effect`, and
//   `SegmentAnimation`) — no name -> real-CapCut-resource-id catalog exists
//   in this app yet (there's no effect/filter/transition/font catalog or
//   authoring UI at all), so any project that actually uses either of these
//   will export *something*, but CapCut/Jianying may not render it as the
//   author intended.
// - `project.keyframes` only maps six properties
//   (`position_x`/`position_y`/`rotation`/`scale`/`alpha`/`volume`) onto a
//   real CapCut `KFType*`; anything else is silently skipped by the export
//   pipeline (`capcut::export::add_video_clip`/`add_audio_clip`), not
//   fabricated — flagged here so that skip isn't a silent surprise.
//
// Masks/stickers are NOT checked here: `project::types` (`docs/project-
// format.md`) has no mask or sticker concept at all yet, so no real project
// can ever populate the CapCut adapter's (already-ported) `add_mask`/
// `add_sticker` paths in a way that would need a warning today.
//
// Caption styling is NOT checked here either: `caption_style.rs` maps every
// `CaptionStyle` field (including `CaptionBackground`, corrected mid-Phase-9
// to have a real capcut-mate equivalent after all) onto a real CapCut
// `TextStyle`/`TextBorder`/`TextBackground`/`TextShadow` — there is no known
// caption-styling gap to warn about.

import type { ProjectV1 } from "../types/bindings";

/** One compatibility warning: an i18n key under the `capcutExport`
 * namespace (`src/locales/{en,vi}.json`) plus any interpolation params it
 * needs. Kept as data rather than a pre-formatted string so the UI layer
 * stays in charge of translation/formatting, matching this codebase's `t()`
 * convention everywhere else. */
export interface CapcutCompatWarning {
  key: string;
  params?: Record<string, string | number>;
}

/** `project::Keyframe::property` values `capcut::keyframe::KeyframeProperty::from_project_property`
 * actually maps onto a real CapCut keyframe type — kept in sync with that
 * Rust `match` by hand (there's no shared codegen for this, only for
 * types). */
const SUPPORTED_KEYFRAME_PROPERTIES: ReadonlySet<string> = new Set([
  "position_x",
  "position_y",
  "rotation",
  "scale",
  "alpha",
  "volume",
]);

/** Scans `project` for the specific documented CapCut-adapter gaps above and
 * returns a non-blocking warning list — empty when nothing known to be
 * unsupported is actually present. Pure and synchronous: safe to call from
 * a `$derived.by` on every keystroke of the export dialog. */
export function computeCapcutCompatWarnings(project: ProjectV1): CapcutCompatWarning[] {
  const warnings: CapcutCompatWarning[] = [];

  if (project.effects.length > 0) {
    warnings.push({ key: "capcutExport.warnEffects", params: { count: project.effects.length } });
  }

  if (project.animations.length > 0) {
    warnings.push({
      key: "capcutExport.warnAnimations",
      params: { count: project.animations.length },
    });
  }

  const unsupportedKeyframeCount = project.keyframes.filter(
    (k) => !SUPPORTED_KEYFRAME_PROPERTIES.has(k.property),
  ).length;
  if (unsupportedKeyframeCount > 0) {
    warnings.push({
      key: "capcutExport.warnKeyframes",
      params: { count: unsupportedKeyframeCount },
    });
  }

  return warnings;
}
