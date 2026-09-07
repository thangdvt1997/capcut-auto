<!--
  Design System foundation (Phase D1, promt.md §19): a real hover/focus-
  triggered tooltip with real positioning. Pure-CSS (no JS positioning
  library, matching this app's own established "don't add a dependency for
  something this small" bar) — `:hover`/`:focus-within` on the wrapper
  toggles the floating panel's visibility, `placement` picks which side.

  Every existing dialog approximates a tooltip today with a plain
  `title="..."` attribute (native browser tooltip, no styling control) —
  this is the first real, styled tooltip in the app.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    text,
    placement = "top",
    children,
  }: {
    text: string;
    placement?: "top" | "bottom" | "left" | "right";
    children: Snippet;
  } = $props();
</script>

<span class="ui-tooltip-wrap">
  {@render children()}
  <span class="ui-tooltip ui-tooltip-{placement}" role="tooltip">{text}</span>
</span>

<style>
  .ui-tooltip-wrap {
    position: relative;
    display: inline-flex;
  }
  .ui-tooltip {
    position: absolute;
    z-index: 200;
    max-width: 220px;
    padding: 4px 8px;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 10.5px;
    line-height: 1.4;
    white-space: normal;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition: opacity 120ms ease-out;
    box-shadow: 0 4px 16px hsl(0 0% 0% / 0.4);
  }
  .ui-tooltip-wrap:hover .ui-tooltip,
  .ui-tooltip-wrap:focus-within .ui-tooltip {
    opacity: 1;
    visibility: visible;
  }
  .ui-tooltip-top {
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  .ui-tooltip-bottom {
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
  }
  .ui-tooltip-left {
    right: calc(100% + 6px);
    top: 50%;
    transform: translateY(-50%);
  }
  .ui-tooltip-right {
    left: calc(100% + 6px);
    top: 50%;
    transform: translateY(-50%);
  }
</style>
