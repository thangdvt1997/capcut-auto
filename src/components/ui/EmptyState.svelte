<!--
  Design System foundation (Phase D1, promt.md §19): a real, reusable
  "nothing here yet" pattern. Deliberately matches this app's own existing
  visual language rather than inventing a new one — the same centered,
  `muted`/`muted-2` recipe `PanelPlaceholder.svelte`'s `.panel-placeholder`
  already uses, and the same `<p class="xx-empty muted-2">` one-liner every
  dialog (BatchJobsDialog/AutomationRulesDialog/CapCutSettingsDialog, etc.)
  already hand-rolls for its own empty list, generalized with an optional
  icon and action slot.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    icon,
    title,
    description,
    fullHeight = false,
    action,
  }: {
    icon?: string;
    title: string;
    description?: string;
    fullHeight?: boolean;
    action?: Snippet;
  } = $props();
</script>

<div class="ui-empty-state" class:ui-empty-state-full={fullHeight}>
  {#if icon}
    <div class="ui-empty-state-icon" aria-hidden="true">{icon}</div>
  {/if}
  <div class="ui-empty-state-title">{title}</div>
  {#if description}
    <p class="ui-empty-state-desc muted-2">{description}</p>
  {/if}
  {#if action}
    <div class="ui-empty-state-action">{@render action()}</div>
  {/if}
</div>

<style>
  .ui-empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-4);
    color: var(--muted-2);
  }
  .ui-empty-state-full {
    height: 100%;
    justify-content: center;
  }
  .ui-empty-state-icon {
    font-size: 24px;
    opacity: 0.7;
  }
  .ui-empty-state-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .ui-empty-state-desc {
    margin: 0;
    max-width: 320px;
    font-size: 11px;
    line-height: 1.5;
  }
  .ui-empty-state-action {
    margin-top: var(--space-1);
  }
</style>
