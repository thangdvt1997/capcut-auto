<!--
  Left panel stack from the master-prompt §48 layout: Media / Transcript /
  Templates / AI. Media is real as of Phase 3 (Media Library: import,
  search, thumbnails); Transcript/Templates/AI remain placeholders.
-->
<script lang="ts">
  type TabId = "media" | "transcript" | "templates" | "ai";
  const tabs: { id: TabId; label: string; phase: string }[] = [
    { id: "media", label: "Media", phase: "Media Library — Phase 3" },
    { id: "transcript", label: "Transcript", phase: "Transcript Editor — Phase 7" },
    { id: "templates", label: "Templates", phase: "Templates — Phase 11" },
    { id: "ai", label: "AI", phase: "AI Editor — Phase 10" },
  ];

  let active: TabId = $state("media");
  let activeTab = $derived(tabs.find((t) => t.id === active)!);

  import PanelPlaceholder from "./PanelPlaceholder.svelte";
  import MediaLibrary from "../media/MediaLibrary.svelte";
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
    {#if active === "media"}
      <MediaLibrary />
    {:else}
      <PanelPlaceholder title={activeTab.label} phase={activeTab.phase} />
    {/if}
  </div>
</div>

<style>
  .stack {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-right: 1px solid var(--border);
  }
  .panel-body {
    flex: 1;
    min-height: 0;
  }
</style>
