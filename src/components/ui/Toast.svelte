<!--
  Design System foundation (Phase D1, promt.md §19): the real transient-
  notification renderer for `stores/toast.svelte.ts`. Not currently used
  anywhere (out of scope this pass to wire a mount point into `App.svelte`
  or call `toastStore` from any existing dialog — see STUDIO_PLAN.md Phase
  D1 section); a later pass mounts one `<Toast />` in `App.svelte` (same
  "one instance, module-level store" precedent every other dialog here
  follows) and existing dialogs' own ad-hoc inline success/error text can
  progressively adopt `toastStore` instead.
-->
<script lang="ts">
  import { toastStore } from "../../stores/toast.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { ToastVariant } from "../../stores/toast.svelte";

  function iconFor(variant: ToastVariant): string {
    switch (variant) {
      case "success":
        return "✓";
      case "warning":
        return "!";
      case "error":
        return "×";
      default:
        return "i";
    }
  }
</script>

<div class="ui-toast-stack" aria-live="polite">
  {#each toastStore.messages as msg (msg.id)}
    <div class="ui-toast ui-toast-{msg.variant}" role="status">
      <span class="ui-toast-icon" aria-hidden="true">{iconFor(msg.variant)}</span>
      <span class="ui-toast-text">{msg.text}</span>
      <button class="ui-toast-close" onclick={() => toastStore.dismiss(msg.id)} aria-label={t("ui.toast.dismiss")}>
        ×
      </button>
    </div>
  {/each}
</div>

<style>
  .ui-toast-stack {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 300;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 360px;
  }
  .ui-toast {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 10px 12px;
    background: var(--elevated);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px hsl(0 0% 0% / 0.4);
    font-size: 11.5px;
    animation: ui-toast-in 160ms ease-out;
  }
  @keyframes ui-toast-in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .ui-toast-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    font-size: 11px;
    font-weight: 700;
    flex-shrink: 0;
  }
  .ui-toast-info .ui-toast-icon {
    background: var(--accent-bg);
    color: var(--accent);
  }
  .ui-toast-success .ui-toast-icon {
    background: var(--pos-bg);
    color: var(--pos);
  }
  .ui-toast-warning .ui-toast-icon {
    background: var(--warn-bg);
    color: var(--warn);
  }
  .ui-toast-error .ui-toast-icon {
    background: var(--neg-bg);
    color: var(--neg);
  }
  .ui-toast-text {
    flex: 1;
    min-width: 0;
    line-height: 1.4;
  }
  .ui-toast-close {
    flex-shrink: 0;
    background: transparent;
    border: none;
    color: var(--muted-2);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0 2px;
  }
  .ui-toast-close:hover {
    color: var(--foreground);
  }
</style>
