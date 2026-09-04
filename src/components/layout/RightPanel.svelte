<!--
  Right panel stack from the master-prompt §48 layout: Inspector / AI Edit /
  Properties. Placeholders in Phase 2, same pattern as LeftPanel.svelte.
-->
<script lang="ts">
  import { t } from "../../lib/i18n.svelte";

  type TabId = "inspector" | "ai-edit" | "properties";
  const tabs: { id: TabId; labelKey: string; phaseKey: string }[] = [
    { id: "inspector", labelKey: "rightPanel.tabInspector", phaseKey: "rightPanel.phaseInspector" },
    { id: "ai-edit", labelKey: "rightPanel.tabAiEdit", phaseKey: "rightPanel.phaseAiEdit" },
    { id: "properties", labelKey: "rightPanel.tabProperties", phaseKey: "rightPanel.phaseProperties" },
  ];

  let active: TabId = $state("inspector");
  let activeTab = $derived(tabs.find((tab) => tab.id === active)!);

  import PanelPlaceholder from "./PanelPlaceholder.svelte";
</script>

<div class="stack">
  <div class="panel-tabs">
    {#each tabs as tab (tab.id)}
      <button
        class="panel-tab"
        class:active={tab.id === active}
        onclick={() => (active = tab.id)}
      >
        {t(tab.labelKey)}
      </button>
    {/each}
  </div>
  <div class="panel-body">
    <PanelPlaceholder title={t(activeTab.labelKey)} phase={t(activeTab.phaseKey)} />
  </div>
</div>

<style>
  .stack {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
  .panel-body {
    flex: 1;
    min-height: 0;
  }
</style>
