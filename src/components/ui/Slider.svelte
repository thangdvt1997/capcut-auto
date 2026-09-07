<!--
  Design System foundation (Phase D1, promt.md §19): a real numeric slider
  primitive — the app's existing numeric fields (e.g. AutomationRulesDialog's
  `.ar-number`) are plain `<input type="number">`; this covers the "drag a
  handle" case (volume/opacity/zoom-intensity-style settings), with an
  optional `formatValue` so a caller can render e.g. "35%" instead of a raw
  number without this component needing to know about units.
-->
<script lang="ts">
  let {
    value = $bindable(0),
    min = 0,
    max = 100,
    step = 1,
    disabled = false,
    id,
    label,
    formatValue,
    onchange,
  }: {
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    id?: string;
    label?: string;
    formatValue?: (v: number) => string;
    onchange?: (v: number) => void;
  } = $props();

  function handleInput(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    value = v;
    onchange?.(v);
  }

  let display = $derived(formatValue ? formatValue(value) : String(value));
</script>

<div class="ui-field">
  {#if label}
    <div class="ui-slider-header">
      <label class="ui-label" for={id}>{label}</label>
      <span class="ui-slider-value mono muted-2">{display}</span>
    </div>
  {/if}
  <input type="range" {id} class="ui-slider" {min} {max} {step} {disabled} {value} oninput={handleInput} />
</div>

<style>
  .ui-slider-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .ui-slider-value {
    font-size: 10.5px;
  }
  .ui-slider {
    width: 100%;
    height: 16px;
    padding: 0;
    margin: 0;
    background: transparent;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .ui-slider:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
