<!--
  Design System foundation (Phase D1, promt.md §19): a square, icon-only
  button variant of Button.svelte. `ariaLabel` is required (not optional) —
  an icon-only control with no visible text has no other accessible name.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  type Variant = "secondary" | "ghost" | "danger";
  type Size = "sm" | "md";

  let {
    variant = "ghost",
    size = "md",
    disabled = false,
    ariaLabel,
    title,
    onclick,
    children,
  }: {
    variant?: Variant;
    size?: Size;
    disabled?: boolean;
    ariaLabel: string;
    title?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  } = $props();

  const variantClass: Record<Variant, string> = {
    secondary: "",
    ghost: "btn-ghost",
    danger: "btn-danger",
  };
</script>

<button
  class="btn ui-icon-btn {variantClass[variant]}"
  class:ui-icon-btn-sm={size === "sm"}
  aria-label={ariaLabel}
  title={title ?? ariaLabel}
  {disabled}
  onclick={onclick}
>
  {@render children?.()}
</button>

<style>
  .ui-icon-btn {
    width: 28px;
    height: 28px;
    padding: 0;
  }
  .ui-icon-btn-sm {
    width: 22px;
    height: 22px;
  }
</style>
