// Svelte 5 runes-based store for the Phase 10 natural-language AI command box
// (master prompt §20): "Natural language -> AI Provider -> EditPlan -> Schema
// validation -> Preview -> Apply". Structurally mirrors
// `stores/fillerWordDetector.svelte.ts`/`stores/silenceDetector.svelte.ts`'s
// own "propose candidates -> preview -> explicit Apply, never auto-apply"
// shape (same trackId/clipId/applyMode selection, same
// `timeline.applyExternalProjectResult` plumbing, same one-atomic-undo-step
// Reset) — this is the same workflow shape, just with an AI-generated
// `EditPlan` as the thing being previewed instead of a VAD/dictionary-derived
// cutlist.
//
// ## Why this store never calls `validate_edit_plan` itself
//
// `generate_edit_plan_from_nl_command` (`src-tauri/src/commands/ai.rs`)
// already chains straight into the exact same `edit_plan::parse_and_validate`
// that command wraps (its own doc comment: "never a second, parallel
// validation path") — so the `EditPlan` this store receives from a
// successful call is already validated, or the call itself returned a clear
// `AppErrorPayload`. A second, redundant validation pass here would just be
// re-checking something the backend already guarantees.
//
// ## Why Apply sends `plan` directly, not a client-rebuilt cutlist
//
// `apply_edit_plan_to_clip`/`apply_edit_plan_to_track` already take the
// validated `EditPlan` itself and do the `Remove`-operations-to-`Cut`s
// conversion server-side (`edit_plan::plan_to_remove_cuts`) before applying
// through the existing `apply_silence_cuts`/`apply_silence_cuts_to_track`
// path. `build_cuts_from_edit_plan` is called once per `generate()` purely
// to drive an honest "will cut N region(s) totaling Xs" preview line — its
// result (`previewCuts`) is never itself sent anywhere; Apply always re-sends
// the original `plan`.
//
// ## Grounding: transcript + duration are scoped to one media, not the whole project
//
// `generate_edit_plan_from_nl_command`'s own prompt builder
// (`ai::nl_command::build_edit_plan_prompt`) labels its duration parameter
// "Total media duration" (not "project duration") — so, exactly like
// `fillerWordDetector.transcriptEntries`, this store scopes both the
// transcript passed to the AI and the duration figure to the *selected
// clip's underlying media*. This also matches what `EditPlan.operations`'
// `start_us`/`end_us` are actually relative to once applied
// (`source_media_id`-scoped, per `apply_edit_plan_to_clip`'s own signature).
//
// ## `Zoom` operations are shown, never hidden (task brief, master prompt §18)
//
// `EditOperation::Zoom` parses/validates cleanly but
// `apply_edit_plan_to_clip`/`_to_track` silently skip it (`ai::edit_plan`
// module doc comment — no keyframe-authoring UI exists yet to make "zoom"
// meaningful on this timeline). `zoomOperationsCount` exists purely so the
// dialog can render an honest "not applied yet" badge on every `Zoom` entry
// rather than implying every listed operation takes effect on Apply.

import { commands } from "../types/bindings";
import type { Clip, Cut, EditPlan, MediaItem, Track, TranscriptEntry } from "../types/bindings";
import { timeline } from "./timeline.svelte";
import { currentAiProviderSettings } from "./aiSettings.svelte";

export type ApplyMode = "clip" | "track";

class AiNlCommandStore {
  open = $state(false);

  trackId = $state<string | null>(null);
  clipId = $state<string | null>(null);
  applyMode = $state<ApplyMode>("clip");

  nlCommand = $state("");

  // ---- Workflow results -------------------------------------------------

  plan = $state<EditPlan | null>(null);
  /** Real `Cut`s the plan's `Remove` operations resolve to (via the exact
   * same conversion Apply ultimately exercises server-side) — used only for
   * an honest "will cut" preview line, never itself sent to Apply. See
   * module doc comment. */
  previewCuts = $state<Cut[]>([]);

  generating = $state(false);
  applying = $state(false);
  lastError = $state<string | null>(null);

  /** Set once Apply has succeeded this session, so Reset knows to also call
   * `timeline.undo()` — same precedent `silenceDetector`/
   * `fillerWordDetector`'s own `appliedThisSession` already establishes. */
  appliedThisSession = $state(false);

  // ---- Derived selection context (same shape as silenceDetector/fillerWordDetector) ----

  eligibleTracks = $derived.by((): Track[] => {
    return timeline.tracks.filter((t) => t.kind === "audio" || t.kind === "video");
  });

  clipsForSelectedTrack = $derived.by((): Clip[] => {
    if (!this.trackId) return [];
    return (timeline.clipsByTrack.get(this.trackId) ?? []).filter((c) => c.media_id !== null);
  });

  selectedClip = $derived.by((): Clip | null => {
    if (!this.clipId) return null;
    return this.clipsForSelectedTrack.find((c) => c.id === this.clipId) ?? null;
  });

  selectedMedia = $derived.by((): MediaItem | null => {
    const clip = this.selectedClip;
    if (!clip?.media_id) return null;
    return timeline.mediaById.get(clip.media_id) ?? null;
  });

  /** The transcript entries grounding the AI's prompt — empty (with a hint
   * in the UI) when the selected media has none, matching
   * `fillerWordDetector.transcriptEntries` exactly. Pure timing commands
   * ("remove all silence longer than 800ms") can still work without a
   * transcript; wording-based ones ("remove filler words", "remove the
   * intro") are much better grounded with one. */
  transcriptEntries = $derived.by((): TranscriptEntry[] => {
    const media = this.selectedMedia;
    if (!media) return [];
    return (timeline.project?.transcript ?? []).filter((e) => e.media_id === media.id);
  });

  removeOperationsCount = $derived.by((): number => {
    return this.plan ? this.plan.operations.filter((op) => op.type === "remove").length : 0;
  });

  zoomOperationsCount = $derived.by((): number => {
    return this.plan ? this.plan.operations.filter((op) => op.type === "zoom").length : 0;
  });

  previewTotalCutUs = $derived(this.previewCuts.reduce((sum, c) => sum + (c.end_us - c.start_us), 0));

  canGenerate = $derived(this.nlCommand.trim().length > 0 && this.selectedMedia !== null && !this.generating);
  canApply = $derived(
    this.removeOperationsCount > 0 &&
      !this.applying &&
      (this.applyMode === "clip" ? this.clipId !== null : this.trackId !== null),
  );

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openFor(opts: { trackId?: string; clipId?: string } = {}): void {
    this.resetResults();
    this.open = true;
    const tracks = this.eligibleTracks;
    this.trackId = opts.trackId ?? tracks[0]?.id ?? null;
    const clips = this.trackId ? (timeline.clipsByTrack.get(this.trackId) ?? []).filter((c) => c.media_id !== null) : [];
    this.clipId = opts.clipId ?? clips[0]?.id ?? null;
  }

  close(): void {
    this.open = false;
    this.resetResults();
  }

  private resetResults(): void {
    this.plan = null;
    this.previewCuts = [];
    this.lastError = null;
    this.appliedThisSession = false;
  }

  setTrack(trackId: string): void {
    if (trackId === this.trackId) return;
    this.trackId = trackId;
    const clips = (timeline.clipsByTrack.get(trackId) ?? []).filter((c) => c.media_id !== null);
    this.clipId = clips[0]?.id ?? null;
    this.resetResults();
  }

  setClip(clipId: string): void {
    if (clipId === this.clipId) return;
    this.clipId = clipId;
    this.resetResults();
  }

  // -------------------------------------------------------------------
  // Workflow: Generate Plan (Preview is built in) -> Apply -> Reset
  // -------------------------------------------------------------------

  /** **NL -> AI Provider -> EditPlan -> Schema validation** (master prompt
   * §20's pipeline up through validation — already chained together
   * server-side, see module doc comment). Also fetches the real `Cut`s the
   * plan's `Remove` operations resolve to, purely for an honest preview
   * line ("will cut N region(s) totaling Xs"). */
  async generate(): Promise<void> {
    const media = this.selectedMedia;
    if (!this.canGenerate || !media) return;
    this.generating = true;
    this.lastError = null;
    this.plan = null;
    this.previewCuts = [];
    try {
      const settings = currentAiProviderSettings();
      const result = await commands.generateEditPlanFromNlCommand(
        settings,
        this.nlCommand,
        this.transcriptEntries,
        media.duration_us,
      );
      if (result.status === "ok") {
        this.plan = result.data;
        this.previewCuts = await commands.buildCutsFromEditPlan(media.id, result.data);
      } else {
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.generating = false;
    }
  }

  /** **User Approves -> Timeline Engine**: sends the *exact* validated `plan`
   * this store received from `generate()` to `apply_edit_plan_to_clip`/
   * `apply_edit_plan_to_track` — never a client-rebuilt cutlist — as one
   * atomic undo step, then folds the resulting project back into
   * `stores/timeline.svelte.ts` exactly like every other detector's Apply
   * step. Never called automatically; only ever from this dialog's own
   * explicit "Apply" click (master prompt §18's "User Approves" stage). */
  async apply(): Promise<void> {
    const media = this.selectedMedia;
    if (!this.canApply || !this.plan || !media) return;
    this.applying = true;
    this.lastError = null;
    try {
      const outcome =
        this.applyMode === "clip"
          ? await timeline.applyExternalProjectResult(
              commands.applyEditPlanToClip(this.clipId as string, media.id, this.plan),
            )
          : await timeline.applyExternalProjectResult(
              commands.applyEditPlanToTrack(this.trackId as string, media.id, this.plan),
            );
      if (outcome.ok) {
        this.appliedThisSession = true;
      } else {
        this.lastError = outcome.error;
      }
    } finally {
      this.applying = false;
    }
  }

  /** **Reset**: discards the current plan/preview and, if Apply already ran
   * this session, undoes that one atomic apply via the existing
   * `timeline.undo()` — same "no second undo mechanism" precedent
   * `silenceDetector`/`fillerWordDetector` already establish. */
  async reset(): Promise<void> {
    if (this.appliedThisSession) {
      await timeline.undo();
    }
    this.resetResults();
  }
}

export const aiNlCommandStore = new AiNlCommandStore();
