<!--
  Design System foundation (Phase D1, promt.md §19): the real bordered/
  elevated container repeated ad-hoc across existing dialogs — sampled
  CapCutSettingsDialog's `.cs-card` (detected-installation rows),
  AutomationRulesDialog's `.ar-row`, HighlightCard's `.hc-card`, and
  BatchJobsDialog's row styling before designing this: all converge on the
  same recipe (var(--surface-2) background, 1px var(--border), var(--radius-sm)).
  `interactive` adds hover/focus affordance + click/keyboard activation for
  a card that's itself a button (e.g. a future template/preset picker).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    padding = "md",
    interactive = false,
    onclick,
    children,
  }: {
    padding?: "sm" | "md";
    interactive?: boolean;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();

  function handleKeydown(e: KeyboardEvent): void {
    if (!interactive) return;
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onclick?.(e as unknown as MouseEvent);
    }
  }
</script>

{#if interactive}
  <div
    class="ui-card ui-card-{padding} ui-card-interactive"
    role="button"
    tabindex="0"
    onclick={onclick}
    onkeydown={handleKeydown}
  >
    {@render children()}
  </div>
{:else}
  <div class="ui-card ui-card-{padding}">
    {@render children()}
  </div>
{/if}

<style>
  .ui-card {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .ui-card-sm {
    padding: var(--space-2) var(--space-3);
  }
  .ui-card-md {
    padding: var(--space-3) var(--space-4);
  }
  .ui-card-interactive {
    cursor: pointer;
    transition:
      border-color 120ms,
      background 120ms;
  }
  .ui-card-interactive:hover {
    border-color: var(--border-strong);
    background: var(--elevated);
  }
  .ui-card-interactive:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
