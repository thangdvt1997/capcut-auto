<!--
  Right panel stack from the master-prompt §48 layout: Inspector / AI Edit /
  Properties. Placeholders in Phase 2, same pattern as LeftPanel.svelte.
-->
<script lang="ts">
  type TabId = "inspector" | "ai-edit" | "properties";
  const tabs: { id: TabId; label: string; phase: string }[] = [
    { id: "inspector", label: "Inspector", phase: "Inspector — Phase 4" },
    { id: "ai-edit", label: "AI Edit", phase: "AI Edit Plan — Phase 10" },
    { id: "properties", label: "Properties", phase: "Clip Properties — Phase 4" },
  ];

  let active: TabId = $state("inspector");
  let activeTab = $derived(tabs.find((t) => t.id === active)!);

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
        {tab.label}
      </button>
    {/each}
  </div>
  <div class="panel-body">
    <PanelPlaceholder title={activeTab.label} phase={activeTab.phase} />
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
