<!--
  Design System foundation (Phase D1, promt.md §19): the real section-with-
  uppercase-title shape repeated ad-hoc across existing dialogs — sampled
  CapCutSettingsDialog's `.cs-section`/`.cs-section-title`,
  AutomationRulesDialog's `.ar-section-title`, and BatchJobsDialog's
  toolbar-row-plus-body shape: all converge on "uppercase, letter-spaced,
  var(--muted) title + optional header-right actions + a body". Distinct
  from Card.svelte — a Panel has no border/background of its own (it's a
  layout section within a dialog body, not a bordered box); nest a Card
  inside a Panel's body when a bordered box is also wanted.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    actions,
    children,
  }: {
    title?: string;
    actions?: Snippet;
    children: Snippet;
  } = $props();
</script>

<section class="ui-panel">
  {#if title || actions}
    <div class="ui-panel-header">
      {#if title}<h3 class="ui-panel-title">{title}</h3>{/if}
      {#if actions}<div class="ui-panel-actions">{@render actions()}</div>{/if}
    </div>
  {/if}
  <div class="ui-panel-body">
    {@render children()}
  </div>
</section>

<style>
  .ui-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .ui-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .ui-panel-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .ui-panel-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .ui-panel-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
</style>
