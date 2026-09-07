<!--
  Phase D2+D3 (`STUDIO_PLAN.md`'s "3-tab shell + Tab 1 (Workspace) content"
  section): the pre-existing full multi-track editor layout (LeftPanel |
  CenterPreview | RightPanel over a docked TimelinePanel) — moved out of
  `App.svelte` verbatim, unchanged, when Tab 1 ("Workspace") gained a
  Simple/Advanced mode toggle. This component IS the "Advanced" mode.

  Nothing here is new: same `ResizableSplit` nesting, same `storageKey`s (so
  a user's already-persisted split ratios keep working exactly as before),
  same panels (`LeftPanel`/`CenterPreview`/`RightPanel`/`TimelinePanel`) —
  the real multi-track drag/drop, zoom/scroll, keyframes, Media Library, and
  Inspector this app's single largest engineering investment lives in.
  See `WorkspaceTab.svelte`'s own doc comment for why a mode toggle (not a
  4th top-level tab) is how this stays reachable now that promt.md's own
  simpler pipeline-stepper mockup (`WorkspaceSimple.svelte`) is Tab 1's
  default view.
-->
<script lang="ts">
  import LeftPanel from "./LeftPanel.svelte";
  import CenterPreview from "./CenterPreview.svelte";
  import RightPanel from "./RightPanel.svelte";
  import TimelinePanel from "./TimelinePanel.svelte";
  import ResizableSplit from "./ResizableSplit.svelte";
</script>

<div class="advanced">
  <ResizableSplit
    direction="vertical"
    initial={0.72}
    min={0.4}
    max={0.88}
    storageKey="ave:split:main-timeline"
  >
    {#snippet a()}
      <ResizableSplit
        direction="horizontal"
        initial={0.2}
        min={0.12}
        max={0.34}
        storageKey="ave:split:left"
      >
        {#snippet a()}
          <LeftPanel />
        {/snippet}
        {#snippet b()}
          <ResizableSplit
            direction="horizontal"
            initial={0.76}
            min={0.5}
            max={0.9}
            storageKey="ave:split:right"
          >
            {#snippet a()}
              <CenterPreview />
            {/snippet}
            {#snippet b()}
              <RightPanel />
            {/snippet}
          </ResizableSplit>
        {/snippet}
      </ResizableSplit>
    {/snippet}
    {#snippet b()}
      <TimelinePanel />
    {/snippet}
  </ResizableSplit>
</div>

<style>
  .advanced {
    height: 100%;
    min-height: 0;
  }
</style>
