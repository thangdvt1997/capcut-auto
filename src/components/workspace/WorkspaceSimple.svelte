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
-->
<script lang="ts">
  import ResizableSplit from "../layout/ResizableSplit.svelte";
  import CenterPreview from "../layout/CenterPreview.svelte";
  import ScriptEditor from "./ScriptEditor.svelte";
  import PipelineStepper from "./PipelineStepper.svelte";
  import JobQueuePanel from "./JobQueuePanel.svelte";
  import TranslationReviewDialog from "../captions/TranslationReviewDialog.svelte";
</script>

<div class="simple">
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
  }
  .top-split {
    min-height: 0;
    height: 100%;
  }
</style>
