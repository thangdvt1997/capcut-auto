<!--
  Design System foundation (Phase D1, promt.md §19): a real iOS-style toggle
  switch. Confirmed via grep across `src/` before building this: no such
  visual toggle exists anywhere in the app today — every existing on/off
  control (e.g. AutomationRulesDialog's rule-enabled toggle) is a plain
  `<input type="checkbox">`, so this is a genuinely new primitive, not a
  reskin of something already there.
-->
<script lang="ts">
  let {
    checked = $bindable(false),
    disabled = false,
    id,
    ariaLabel,
    onchange,
  }: {
    checked?: boolean;
    disabled?: boolean;
    id?: string;
    ariaLabel?: string;
    onchange?: (checked: boolean) => void;
  } = $props();

  function toggle(): void {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel}
  {id}
  class="ui-switch"
  class:ui-switch-on={checked}
  {disabled}
  onclick={toggle}
>
  <span class="ui-switch-thumb"></span>
</button>

<style>
  .ui-switch {
    position: relative;
    width: 34px;
    height: 18px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2);
    cursor: pointer;
    flex-shrink: 0;
    transition:
      background 120ms,
      border-color 120ms;
  }
  .ui-switch:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .ui-switch-on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .ui-switch-thumb {
    position: absolute;
    top: 1px;
    left: 1px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--foreground);
    transition: transform 120ms ease-out;
  }
  .ui-switch-on .ui-switch-thumb {
    transform: translateX(16px);
    background: var(--primary-fg);
  }
</style>
