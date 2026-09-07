<!--
  Design System foundation (Phase D1, promt.md §19): a real tab-strip
  component. Sampled `LeftPanel.svelte`/`RightPanel.svelte`'s own internal
  tab strips before designing this — both already use the exact
  `.panel-tabs`/`.panel-tab`/`.active` global classes in globals.css, so
  this component reuses those classes verbatim (no new/duplicate classes)
  to be a genuine drop-in replacement for both of those AND, later, a new
  top-level 3-tab shell.
-->
<script lang="ts">
  export type TabItem = { id: string; label: string };

  let {
    tabs,
    active = $bindable(""),
    onChange,
  }: {
    tabs: TabItem[];
    active?: string;
    onChange?: (id: string) => void;
  } = $props();

  function select(id: string): void {
    active = id;
    onChange?.(id);
  }
</script>

<div class="panel-tabs">
  {#each tabs as tab (tab.id)}
    <button class="panel-tab" class:active={tab.id === active} onclick={() => select(tab.id)}>
      {tab.label}
    </button>
  {/each}
</div>
