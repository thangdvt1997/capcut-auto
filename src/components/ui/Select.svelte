<!--
  Design System foundation (Phase D1, promt.md §19): a real `<select>` form
  primitive on the new `.ui-field`/`.ui-select` classes in globals.css,
  replacing every dialog's own ad-hoc `.xx-select` (e.g. AutomationRulesDialog's
  `.ar-select`).
-->
<script module lang="ts">
  export type SelectOption = { value: string; label: string; disabled?: boolean };
</script>

<script lang="ts">
  let {
    value = $bindable(""),
    options,
    placeholder,
    disabled = false,
    id,
    label,
    error,
    onchange,
  }: {
    value?: string;
    options: SelectOption[];
    placeholder?: string;
    disabled?: boolean;
    id?: string;
    label?: string;
    error?: string;
    onchange?: (value: string) => void;
  } = $props();

  function handleChange(e: Event): void {
    const v = (e.target as HTMLSelectElement).value;
    value = v;
    onchange?.(v);
  }
</script>

<div class="ui-field">
  {#if label}
    <label class="ui-label" for={id}>{label}</label>
  {/if}
  <select {id} class="ui-select" class:ui-input-error={!!error} {disabled} {value} onchange={handleChange}>
    {#if placeholder}
      <option value="" disabled>{placeholder}</option>
    {/if}
    {#each options as opt (opt.value)}
      <option value={opt.value} disabled={opt.disabled}>{opt.label}</option>
    {/each}
  </select>
  {#if error}
    <span class="ui-field-error">{error}</span>
  {/if}
</div>

<style>
  .ui-input-error {
    border-color: var(--neg-border);
  }
</style>
