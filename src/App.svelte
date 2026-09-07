<!--
  App shell (Phase D2+D3, `STUDIO_PLAN.md`'s "3-tab shell + Tab 1
  (Workspace) content" section, promt.md §2's "3 TAB" architecture):
    Menu/toolbar (TopBar) on top; below it, a real 3-top-level-tab strip
    (Workspace | Automation & AI Settings | Project/Asset/License
    Management) built from the Design System's real `Tabs` component.
    "Workspace" (Tab 1) renders `WorkspaceTab.svelte` — promt.md §3/§28's own
    simple pipeline-stepper mockup by default, with the pre-existing full
    multi-track editor (LeftPanel|CenterPreview|RightPanel + TimelinePanel —
    this app's single largest engineering investment) one click away via an
    "Advanced" mode toggle inside that same tab; see `WorkspaceTab.svelte`'s
    own doc comment for the full placement decision. Tab 2/Tab 3 are honest
    `EmptyState` placeholders this pass — their real content is Phase
    D5/D6's own separate scope, not faked here.

    Every dialog below is mounted exactly as before this pass, unconditionally
    (not gated by which top-level tab is active) — none of their own
    opening logic changed.
-->
<script lang="ts">
  import TopBar from "./components/layout/TopBar.svelte";
  import Tabs from "./components/ui/Tabs.svelte";
  import EmptyState from "./components/ui/EmptyState.svelte";
  import WorkspaceTab from "./components/workspace/WorkspaceTab.svelte";
  import { t } from "./lib/i18n.svelte";
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

  let activeAppTab = $state("workspace");
</script>

<main class="shell">
  <TopBar />

  <div class="app-tabs">
    <Tabs
      tabs={[
        { id: "workspace", label: t("appTabs.workspace") },
        { id: "automation", label: t("appTabs.automationAi") },
        { id: "projects", label: t("appTabs.projectAssetLicense") },
      ]}
      active={activeAppTab}
      onChange={(id) => (activeAppTab = id)}
    />
  </div>

  <section class="workspace">
    {#if activeAppTab === "workspace"}
      <WorkspaceTab />
    {:else if activeAppTab === "automation"}
      <EmptyState
        fullHeight
        title={t("appTabs.automationPlaceholderTitle")}
        description={t("appTabs.automationPlaceholderDesc")}
      />
    {:else}
      <EmptyState
        fullHeight
        title={t("appTabs.projectsPlaceholderTitle")}
        description={t("appTabs.projectsPlaceholderDesc")}
      />
    {/if}
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
    grid-template-rows: auto auto 1fr;
    height: 100vh;
    overflow: hidden;
  }
  .app-tabs {
    padding: 6px 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .workspace {
    min-height: 0;
    overflow: hidden;
    padding: 8px;
  }
</style>
