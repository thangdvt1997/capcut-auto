<!--
  Design System foundation (Phase D1, promt.md §19): a real progress bar
  (determinate via `value`/`max`, or `indeterminate` for "working, unknown
  duration" — e.g. a future Worker/Slot dashboard, per-job batch progress).
-->
<script lang="ts">
  type Variant = "accent" | "pos" | "warn" | "neg";

  let {
    value = 0,
    max = 1,
    variant = "accent",
    label,
    indeterminate = false,
  }: {
    value?: number;
    max?: number;
    variant?: Variant;
    label?: string;
    indeterminate?: boolean;
  } = $props();

  let pct = $derived(max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0);
</script>

<div
  class="ui-progress"
  role="progressbar"
  aria-valuenow={indeterminate ? undefined : Math.round(pct)}
  aria-valuemin="0"
  aria-valuemax="100"
>
  <div class="ui-progress-track">
    <div
      class="ui-progress-fill ui-progress-{variant}"
      class:ui-progress-indeterminate={indeterminate}
      style={indeterminate ? undefined : `width: ${pct}%`}
    ></div>
  </div>
  {#if label}
    <span class="ui-progress-label mono muted-2">{label}</span>
  {/if}
</div>

<style>
  .ui-progress {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .ui-progress-track {
    flex: 1;
    min-width: 0;
    height: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    overflow: hidden;
  }
  .ui-progress-fill {
    height: 100%;
    border-radius: 999px;
    transition: width 160ms ease-out;
  }
  .ui-progress-accent {
    background: var(--accent);
  }
  .ui-progress-pos {
    background: var(--pos);
  }
  .ui-progress-warn {
    background: var(--warn);
  }
  .ui-progress-neg {
    background: var(--neg);
  }
  .ui-progress-indeterminate {
    width: 40% !important;
    animation: ui-progress-slide 1.1s ease-in-out infinite;
  }
  @keyframes ui-progress-slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(250%);
    }
  }
  .ui-progress-label {
    font-size: 10.5px;
    flex-shrink: 0;
  }
</style>
