<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): Tab 1 ("Workspace")'s "Simple" mode —
  promt.md §3/§28's own mockup shape (video preview | script/subtitle
  editor, a pipeline stepper below both, a Job Queue below that). This is
  the real, non-placeholder Tab 1 content the task asked for; see
  `WorkspaceTab.svelte`'s own doc comment for why this coexists with the
  pre-existing full editor (`WorkspaceAdvanced.svelte`) via a mode toggle
  rather than replacing it.

  Translation ("Translate" column/step) is now a real review/apply flow
  (`STUDIO_PLAN.md` Phase D12), not an inline propose-only call with no
  review UI behind it: `ScriptEditor.svelte`'s "Translate…" button opens
  `TranslationReviewDialog.svelte` (mounted here, once), which owns the
  `translate_captions` call, per-caption Accept/Reject, and the real
  `apply_caption_translations` write-back — all backed by
  `stores/translationReview.svelte.ts`. `PipelineStepper.svelte` reads that
  same store directly (no props threaded through this component) for its
  "Translate" step state, matching how it already reads
  `stores/captions.svelte.ts`/`stores/batch.svelte.ts` directly for the
  Subtitle/Render steps.

  Phase D19 pass (`STUDIO_PLAN.md`, promt.md §3.1's own "Nếu framework cho
  phép: Space/Left/Right/Ctrl+S" shortcut list): this root `<div>` gained the
  same Space/Left/Right transport shortcuts `Timeline.svelte`'s own
  `onKeyDown` already established for Advanced mode — reusing its exact keys
  and the same shared `timeline.previewApi.togglePlayPause`/`seekBy` calls,
  not a reimplementation — since Simple mode (this component) has no
  keyboard handler of its own today and those shortcuts silently do nothing
  here otherwise (confirmed via grep before this pass: `Timeline.svelte` is
  only ever mounted by `WorkspaceAdvanced.svelte`). Ctrl+S is new (no prior
  handler anywhere in this app — confirmed via grep for `ctrlKey` combined
  with `'s'`/`"s"` across `src/`), backed by the real
  `recentProjectsStore.saveCurrentProjectAs()` this same phase added (see
  its own doc comment for why it always opens the native picker rather than
  silently no-op-ing). Undo/redo/copy/paste/zoom/split/delete are
  deliberately NOT added here — those are Advanced-mode Timeline-editing
  actions with no Simple-mode surface to act on (no clip selection exists in
  this mode), so adding them would be dead code, not a real shortcut.
-->
<script lang="ts">
  import ResizableSplit from "../layout/ResizableSplit.svelte";
  import CenterPreview from "../layout/CenterPreview.svelte";
  import ScriptEditor from "./ScriptEditor.svelte";
  import PipelineStepper from "./PipelineStepper.svelte";
  import JobQueuePanel from "./JobQueuePanel.svelte";
  import TranslationReviewDialog from "../captions/TranslationReviewDialog.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { recentProjectsStore } from "../../stores/recentProjects.svelte";
  import { t } from "../../lib/i18n.svelte";

  let rootEl: HTMLDivElement | undefined = $state();
  let saving = false;

  // Matches `Timeline.svelte`'s own identically-named guard exactly — typing
  // in a form field elsewhere on this tab (e.g. a caption text input, once
  // one exists) must never be hijacked by Space/Arrow/Ctrl+S.
  function isTypingTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
  }

  // Same formula as `Timeline.svelte`'s own `frameDurationUs`/`LARGE_SEEK_US`
  // — kept as a literal duplicate rather than a shared import since both are
  // four lines, and this component has no other reason to depend on
  // `Timeline.svelte`'s module.
  function frameDurationUs(): number {
    const fps = timeline.project?.canvas.fps;
    if (!fps || fps.num <= 0) return 33_333;
    return Math.round((fps.den / fps.num) * 1_000_000);
  }
  const LARGE_SEEK_US = 1_000_000;

  function onKeyDown(e: KeyboardEvent): void {
    if (isTypingTarget(e.target)) return;
    const ctrl = e.ctrlKey || e.metaKey;

    if (ctrl && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (saving || !timeline.project) return;
      saving = true;
      void recentProjectsStore.saveCurrentProjectAs().finally(() => (saving = false));
      return;
    }
    if (ctrl) return;

    if (e.key === " ") {
      e.preventDefault();
      timeline.previewApi.togglePlayPause?.();
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      timeline.seekBy(e.shiftKey ? -LARGE_SEEK_US : -frameDurationUs());
      return;
    }
    if (e.key === "ArrowRight") {
      e.preventDefault();
      timeline.seekBy(e.shiftKey ? LARGE_SEEK_US : frameDurationUs());
      return;
    }
  }

  function focusRoot(): void {
    rootEl?.focus();
  }
</script>

<!--
  Same "custom keyboard-driven widget, no single ARIA role fits" rationale
  as `Timeline.svelte`'s own identical suppression — see that file's doc
  comment.
-->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="simple"
  bind:this={rootEl}
  tabindex="0"
  onkeydown={onKeyDown}
  onpointerdowncapture={focusRoot}
  role="application"
  aria-label={t("workspaceTab.simpleShortcutsAriaLabel")}
>
  <div class="top-split">
    <ResizableSplit
      direction="horizontal"
      initial={0.45}
      min={0.25}
      max={0.7}
      storageKey="ave:split:workspace-simple"
    >
      {#snippet a()}
        <CenterPreview />
      {/snippet}
      {#snippet b()}
        <ScriptEditor />
      {/snippet}
    </ResizableSplit>
  </div>

  <PipelineStepper />

  <JobQueuePanel />
</div>

<TranslationReviewDialog />

<style>
  .simple {
    height: 100%;
    min-height: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto auto;
    gap: var(--space-3);
    padding: var(--space-3);
    /* Keyboard-driven root (Phase D19) — no visible focus ring, matching
       `Timeline.svelte`'s own identical `.timeline-panel` rule, since this
       whole panel (not one focusable child) is the real Space/Arrow/Ctrl+S
       target. */
    outline: none;
  }
  .top-split {
    min-height: 0;
    height: 100%;
  }
</style>
