<!--
  Design System foundation (Phase D1, promt.md §19/§20): a status/label
  pill reusing --pos/--neg/--warn/--accent semantically (a "hàng chục màu
  status khác nhau" — dozens of different status colors — is exactly what
  §19 asks this app to stop doing; every existing ad-hoc status chip today,
  e.g. CapCutSettingsDialog's `.cs-status-inuse`, hand-rolls its own color).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  type Variant = "neutral" | "pos" | "neg" | "warn" | "accent";

  let { variant = "neutral", children }: { variant?: Variant; children?: Snippet } = $props();
</script>

<span class="ui-badge ui-badge-{variant}">{@render children?.()}</span>

<style>
  .ui-badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 2px var(--space-2);
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 600;
    line-height: 1.4;
    white-space: nowrap;
  }
  .ui-badge-neutral {
    color: var(--muted);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .ui-badge-pos {
    color: var(--pos);
    background: var(--pos-bg);
    border: 1px solid var(--pos-border);
  }
  .ui-badge-neg {
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
  }
  .ui-badge-warn {
    color: var(--warn);
    background: var(--warn-bg);
    border: 1px solid var(--warn-border);
  }
  .ui-badge-accent {
    color: var(--accent);
    background: var(--accent-bg);
    border: 1px solid var(--accent-border);
  }
</style>
