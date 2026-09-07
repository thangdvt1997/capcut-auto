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
  import AiSettingsDialog from "./components/ai/AiSettingsDialog.svelte";
  import BatchJobsDialog from "./components/batch/BatchJobsDialog.svelte";
  import UpdateSettingsDialog from "./components/update/UpdateSettingsDialog.svelte";
  import SystemInfoDialog from "./components/system/SystemInfoDialog.svelte";
  import FirstRunWizard from "./components/onboarding/FirstRunWizard.svelte";
  import AssetLibraryDialog from "./components/assets/AssetLibraryDialog.svelte";
  import HistoryDialog from "./components/history/HistoryDialog.svelte";
  import TemplateGeneratorDialog from "./components/templates/TemplateGeneratorDialog.svelte";
  import AutomationRulesDialog from "./components/automation/AutomationRulesDialog.svelte";
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

  <!-- Phase 10: mounted once here for the same "one shared store-backed
       dialog, reachable from a TopBar button" reason as the two dialogs
       above — see AiSettingsDialog.svelte's own doc comment. The
       NL-command-box dialog (`AiCommandBox.svelte`) is mounted inside
       `components/timeline/Timeline.svelte` instead, matching
       `SilenceDetector`/`FillerWordDetector`'s own precedent there (its
       only entry point is that toolbar). -->
  <AiSettingsDialog />

  <!-- Phase 11: mounted once here for the same "one shared store-backed
       dialog, reachable from a TopBar button" reason as the dialogs above —
       see BatchJobsDialog.svelte's own doc comment. Renders its own nested
       `StartBatchDialog` internally, so nothing else needs mounting here. -->
  <BatchJobsDialog />

  <!-- Phase 12: mounted once here for the same "one shared store-backed
       dialog, reachable from a TopBar button" reason as the dialogs above —
       see UpdateSettingsDialog.svelte's own doc comment. -->
  <UpdateSettingsDialog />

  <!-- Phase 12: System Information panel (master prompt §78) — same "one
       shared store-backed dialog, reachable from a TopBar button" reason as
       the dialogs above — see SystemInfoDialog.svelte's own doc comment. -->
  <SystemInfoDialog />

  <!-- Phase 12: First-Run Wizard (master prompt §58) — mounted once here,
       above every other dialog (highest z-index), since it can auto-open on
       first launch independent of any button click, and its own steps open
       several of the dialogs above (CapCutSettingsDialog/AiSettingsDialog/
       ModelManagerDialog/SystemInfoDialog) as sub-actions. See
       FirstRunWizard.svelte's own doc comment. -->
  <FirstRunWizard />

  <!-- Upgrade U3: Asset Library management dialog (upgrade spec §17) — same
       "one shared store-backed dialog, reachable from a TopBar button"
       reason as the dialogs above — see AssetLibraryDialog.svelte's own doc
       comment. Also the shared catalog `TemplatesPanel.svelte`'s
       intro/outro/watermark/background-music pickers read from. -->
  <AssetLibraryDialog />

  <!-- Upgrade U3: Video Processing History dialog (upgrade spec §21) — same
       "one shared store-backed dialog, reachable from a TopBar button"
       reason as the dialogs above — see HistoryDialog.svelte's own doc
       comment. Its "Clone settings"/re-run actions drive `batchStore`'s own
       `StartBatchDialog`/Jobs dialog (already mounted via `BatchJobsDialog`
       above), so nothing else needs mounting here. -->
  <HistoryDialog />

  <!-- Upgrade U2: AI Template Generator dialog (upgrade spec §8) — same "one
       shared store-backed dialog, reachable from a TopBar button" reason as
       the dialogs above — see TemplateGeneratorDialog.svelte's own doc
       comment. Its own sibling, the AI Auto Template dialog (upgrade spec
       §7), is mounted inside `components/timeline/Timeline.svelte` instead,
       since it needs that toolbar's source track/clip picker context — see
       that component's own doc comment. -->
  <TemplateGeneratorDialog />

  <!-- Upgrade U4: Smart Automation rules dialog (upgrade spec §27) — same
       "one shared store-backed dialog, reachable from a TopBar button"
       reason as the dialogs above — see AutomationRulesDialog.svelte's own
       doc comment. Its Create Rule form reads the same shared
       `templatesStore.allTemplates` catalog `StartBatchDialog.svelte`
       already reads from, so nothing else needs mounting here. -->
  <AutomationRulesDialog />
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
