<!--
  Design System gap-fill (Phase D9, `STUDIO_PLAN.md`): a real numeric-input
  form primitive. `Input.svelte`'s own doc comment explicitly scopes it to
  text-like `type`s only ("a dedicated numeric Input variant is left for a
  later pass") — every Phase D7 retrofit that hit a real `<input
  type="number">` (`ExportDialog.svelte`'s resolution/CRF/bitrate fields,
  `AiSettingsDialog.svelte`'s Timeout field) documented this exact gap and
  left the field bespoke rather than touching `Input.svelte`'s own public
  API, since other already-retrofitted dialogs depend on its current shape.

  This is a new, separate sibling component (not a variant bolted onto
  Input.svelte) on the same `.ui-field`/`.ui-input` classes from globals.css,
  so it looks identical to every other Design System text field. Two
  interaction paths, both needed to reproduce every real call site's
  existing behavior exactly:
    - live two-way `bind:value` (via an internal `oninput` handler), for
      callers that bound a store's numeric `$state` field directly to a
      native `<input type="number" bind:value>` (e.g. ExportDialog's width/
      height/CRF/bitrate fields) — updates on every keystroke, same cadence
      Svelte's own native number binding uses.
    - an `onchange` callback firing on the native `change` event (blur/Enter
      commit, not every keystroke), receiving the same parsed number
      `Number(target.value)` would have produced — for callers that commit
      on blur with their own clamping logic (e.g. AiSettingsDialog's Timeout
      field: `Math.max(1000, v || 1000)`).
-->
<script lang="ts">
  let {
    value = $bindable(0),
    min,
    max,
    step,
    placeholder,
    disabled = false,
    id,
    label,
    ariaLabel,
    error,
    hint,
    onblur,
    onchange,
    onkeydown,
  }: {
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    placeholder?: string;
    disabled?: boolean;
    id?: string;
    label?: string;
    ariaLabel?: string;
    error?: string;
    hint?: string;
    onblur?: (e: FocusEvent) => void;
    onchange?: (value: number) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  } = $props();

  function handleInput(e: Event): void {
    value = (e.target as HTMLInputElement).valueAsNumber;
  }

  function handleChange(e: Event): void {
    onchange?.(Number((e.target as HTMLInputElement).value));
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
    type="number"
    {placeholder}
    {disabled}
    {min}
    {max}
    {step}
    {value}
    aria-label={!label ? ariaLabel : undefined}
    oninput={handleInput}
    onchange={handleChange}
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
