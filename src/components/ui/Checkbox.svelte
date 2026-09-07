<!--
  Design System foundation (Phase D1, promt.md §19): a real checkbox
  primitive on the new `.ui-checkbox` class in globals.css — matches the
  exact inline-checkbox-plus-label recipe every existing dialog already
  hand-rolls (e.g. AutomationRulesDialog's `.ar-checkbox`).
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    checked = $bindable(false),
    disabled = false,
    id,
    onchange,
    children,
  }: {
    checked?: boolean;
    disabled?: boolean;
    id?: string;
    onchange?: (checked: boolean) => void;
    children?: Snippet;
  } = $props();

  function handleChange(e: Event): void {
    const v = (e.target as HTMLInputElement).checked;
    checked = v;
    onchange?.(v);
  }
</script>

<label class="ui-checkbox" class:ui-checkbox-disabled={disabled}>
  <input type="checkbox" {id} {checked} {disabled} onchange={handleChange} />
  {#if children}<span>{@render children()}</span>{/if}
</label>
