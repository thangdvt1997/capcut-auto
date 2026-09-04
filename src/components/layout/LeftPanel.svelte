<!--
  Left panel stack from the master-prompt §48 layout: Media / Transcript /
  Templates / AI. Media is real as of Phase 3 (Media Library: import,
  search, thumbnails); Transcript/Templates/AI remain placeholders.
-->
<script lang="ts">
  import { t } from "../../lib/i18n.svelte";

  type TabId = "media" | "transcript" | "templates" | "ai";
  const tabs: { id: TabId; labelKey: string; phaseKey: string }[] = [
    { id: "media", labelKey: "leftPanel.tabMedia", phaseKey: "leftPanel.phaseMedia" },
    { id: "transcript", labelKey: "leftPanel.tabTranscript", phaseKey: "leftPanel.phaseTranscript" },
    { id: "templates", labelKey: "leftPanel.tabTemplates", phaseKey: "leftPanel.phaseTemplates" },
    { id: "ai", labelKey: "leftPanel.tabAi", phaseKey: "leftPanel.phaseAi" },
  ];

  let active: TabId = $state("media");
  let activeTab = $derived(tabs.find((tab) => tab.id === active)!);

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
        {t(tab.labelKey)}
      </button>
    {/each}
  </div>
  <div class="panel-body">
    {#if active === "media"}
      <MediaLibrary />
    {:else}
      <PanelPlaceholder title={t(activeTab.labelKey)} phase={t(activeTab.phaseKey)} />
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
