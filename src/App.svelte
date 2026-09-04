<!--
  App shell (master-prompt §48 layout):
    Menu/toolbar (TopBar) on top; below it, a three-pane workspace
    (Left tabs | Center preview | Right tabs) over a docked Timeline, all
    built from real, working ResizableSplit panes with localStorage-
    persisted ratios. Panel *contents* are Phase 2 placeholders — the
    resizing/layout persistence is the part that actually works.
-->
<script lang="ts">
  import TopBar from "./components/layout/TopBar.svelte";
  import LeftPanel from "./components/layout/LeftPanel.svelte";
  import CenterPreview from "./components/layout/CenterPreview.svelte";
  import RightPanel from "./components/layout/RightPanel.svelte";
  import TimelinePanel from "./components/layout/TimelinePanel.svelte";
  import ResizableSplit from "./components/layout/ResizableSplit.svelte";
  import ExportDialog from "./components/render/ExportDialog.svelte";
  import ModelManagerDialog from "./components/transcription/ModelManagerDialog.svelte";
  import CapCutSettingsDialog from "./components/capcut/CapCutSettingsDialog.svelte";
  import CapCutExportDialog from "./components/capcut/CapCutExportDialog.svelte";
</script>

<main class="shell">
  <TopBar />

  <section class="workspace">
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
  </section>

  <!-- Mounted once here (not inside TopBar/Timeline) since the Export
       dialog has two entry points — TopBar's "File" menu and Timeline's
       toolbar button, per Phase 6's placement decision (see
       ExportDialog.svelte's doc comment) — both should drive one shared
       `renderStore`-backed dialog instance, not two independent copies. -->
  <ExportDialog />

  <!-- Mounted once here for the same reason as ExportDialog: reachable from
       multiple entry points (TopBar's "Models…" button today, plus
       `openModelManager()` for the concurrently-built Transcript Editor's
       own "no model installed" prompt — see ModelManagerDialog.svelte's
       doc comment) that should all drive one shared dialog instance. -->
  <ModelManagerDialog />

  <!-- Phase 9: mounted once here for the same "multiple entry points, one
       shared store-backed dialog" reason as the two dialogs above —
       `CapCutSettingsDialog` is reachable from TopBar's "CapCut…" button
       (and from `CapCutExportDialog` itself, via an "Open CapCut Settings…"
       link shown when no draft directory is known yet);
       `CapCutExportDialog` is reachable from TopBar's File menu ("Export to
       CapCut…"). See each component's own doc comment. -->
  <CapCutSettingsDialog />
  <CapCutExportDialog />
</main>

<style>
  .shell {
    display: grid;
    grid-template-rows: auto 1fr;
    height: 100vh;
    overflow: hidden;
  }
  .workspace {
    min-height: 0;
    overflow: hidden;
    padding: 8px;
  }
</style>
