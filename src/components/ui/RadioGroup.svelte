<!--
  Design System gap-fill (Phase D9, `STUDIO_PLAN.md`): a real radio-button
  group primitive. Every Phase D7 retrofit that hit a real radio picker
  (`ExportDialog.svelte`'s CRF-vs-bitrate toggle, `CapCutExportDialog.svelte`'s
  Create/Update mode picker, `UpdateSettingsDialog.svelte`'s three check-mode
  options) documented "no RadioGroup component exists" and left the group
  hand-rolled — same `<input type="radio">` markup, same `name`/`checked`/
  `onchange` wiring, just not shared. This component is that shared shape,
  sampled from those three dialogs' own near-identical recipes (an
  inline-flex label wrapping the native input, 11.5px text, pointer cursor)
  before writing this: `ExportDialog`'s `.rd-radio-group`/`.rd-radio` and
  `CapCutExportDialog`'s `.ce-radio-group`/`.ce-radio` converge on a
  horizontal row (`gap: 14px`); `UpdateSettingsDialog`'s per-row
  `.us-radio-row` (one `<label>` per line, no wrapping row div, relying on
  its parent Panel's own vertical flex gap) is this component's `vertical`
  orientation.
-->
<script module lang="ts">
  export type RadioOption = { value: string; label: string; disabled?: boolean };
</script>

<script lang="ts">
  let {
    value = $bindable(""),
    options,
    name,
    label,
    disabled = false,
    orientation = "horizontal",
    onchange,
  }: {
    value?: string;
    options: RadioOption[];
    name: string;
    label?: string;
    disabled?: boolean;
    orientation?: "horizontal" | "vertical";
    onchange?: (value: string) => void;
  } = $props();

  function handleChange(v: string): void {
    value = v;
    onchange?.(v);
  }
</script>

<div class="ui-field">
  {#if label}
    <span class="ui-label">{label}</span>
  {/if}
  <div class="ui-radio-group" class:ui-radio-group-vertical={orientation === "vertical"}>
    {#each options as opt (opt.value)}
      <label class="ui-radio" class:ui-radio-disabled={disabled || opt.disabled}>
        <input
          type="radio"
          {name}
          value={opt.value}
          checked={value === opt.value}
          disabled={disabled || opt.disabled}
          onchange={() => handleChange(opt.value)}
        />
        <span>{opt.label}</span>
      </label>
    {/each}
  </div>
</div>

<style>
  .ui-radio-group {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .ui-radio-group-vertical {
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }
  .ui-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .ui-radio-disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
