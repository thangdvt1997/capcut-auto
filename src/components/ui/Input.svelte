<!--
  Design System foundation (Phase D1, promt.md §19): a real text-input form
  primitive on the new `.ui-field`/`.ui-input` classes in globals.css
  (consistent 28px row height/spacing, replacing every dialog's own ad-hoc
  `.xx-input`, e.g. CapCutSettingsDialog's `.cs-input`). Scope note: numeric
  inputs (e.g. AutomationRulesDialog's `.ar-number`) are not covered here —
  Slider.svelte is this pass's numeric primitive; a dedicated numeric Input
  variant is left for a later pass if a real need for a bare number field
  (not a slider) shows up.
-->
<script lang="ts">
  let {
    value = $bindable(""),
    type = "text",
    placeholder,
    disabled = false,
    id,
    label,
    error,
    hint,
    onblur,
    onkeydown,
  }: {
    value?: string;
    type?: "text" | "password" | "search" | "email" | "url";
    placeholder?: string;
    disabled?: boolean;
    id?: string;
    label?: string;
    error?: string;
    hint?: string;
    onblur?: (e: FocusEvent) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();

  function handleInput(e: Event): void {
    value = (e.target as HTMLInputElement).value;
  }
</script>

<div class="ui-field">
  {#if label}
    <label class="ui-label" for={id}>{label}</label>
  {/if}
  <input
    {id}
    class="ui-input"
    class:ui-input-error={!!error}
    {type}
    {placeholder}
    {disabled}
    {value}
    oninput={handleInput}
    {onblur}
    {onkeydown}
  />
  {#if error}
    <span class="ui-field-error">{error}</span>
  {:else if hint}
    <span class="ui-hint">{hint}</span>
  {/if}
</div>

<style>
  .ui-input-error {
    border-color: var(--neg-border);
  }
</style>
