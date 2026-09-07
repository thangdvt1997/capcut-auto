<!--
  Design System foundation (Phase D1, promt.md §19): a real `variant` prop
  implementing promt.md's own explicit 4-tier button hierarchy (Primary/
  Secondary/Danger/Ghost), on top of the existing `.btn`/`.btn-ghost`
  classes in globals.css (never duplicated — `.btn` IS the Secondary tier,
  `.btn-ghost` IS the Ghost tier; only `.btn-primary`/`.btn-danger`, added
  to globals.css this same pass, are new).

  Not yet wired into any existing dialog (out of scope this pass — see
  STUDIO_PLAN.md Phase D1 section) — every existing `<button class="btn ...">`
  call site keeps working unmodified; this component is for new call sites
  (and a later retrofit) to use instead of hand-rolling the same markup.
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  type Variant = "primary" | "secondary" | "danger" | "ghost";
  type Size = "sm" | "md";

  let {
    variant = "secondary",
    size = "md",
    disabled = false,
    loading = false,
    type = "button",
    title,
    onclick,
    children,
  }: {
    variant?: Variant;
    size?: Size;
    disabled?: boolean;
    loading?: boolean;
    type?: "button" | "submit" | "reset";
    title?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  } = $props();

  const variantClass: Record<Variant, string> = {
    primary: "btn-primary",
    secondary: "",
    danger: "btn-danger",
    ghost: "btn-ghost",
  };
</script>

<button
  class="btn {variantClass[variant]}"
  class:ui-btn-sm={size === "sm"}
  disabled={disabled || loading}
  {type}
  {title}
  onclick={onclick}
>
  {#if loading}
    <span class="ui-btn-spinner" aria-hidden="true"></span>
  {/if}
  {@render children?.()}
</button>

<style>
  .ui-btn-sm {
    height: 24px;
    padding: 0 var(--space-2);
    font-size: 11px;
  }
  .ui-btn-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid currentColor;
    border-top-color: transparent;
    border-radius: 50%;
    animation: ui-btn-spin 0.6s linear infinite;
  }
  @keyframes ui-btn-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
