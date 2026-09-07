<!--
  Design System foundation (Phase D1, promt.md §19): a real "error + retry"
  pattern, matching the exact `.xx-error` visual recipe every existing
  dialog already hand-rolls (padding 8-10px, color var(--neg), a tinted
  background, var(--radius-sm) — see CapCutSettingsDialog's `.cs-error`/
  AutomationRulesDialog's `.ar-error`) via the new `--neg-bg`/`--neg-border`
  tokens instead of each dialog's own slightly different literal alpha.
-->
<script lang="ts">
  import { t } from "../../lib/i18n.svelte";

  let {
    message,
    onRetry,
    retryLabel,
    fullHeight = false,
  }: {
    message?: string;
    onRetry?: () => void;
    retryLabel?: string;
    fullHeight?: boolean;
  } = $props();

  let displayMessage = $derived(message ?? t("ui.errorState.default"));
</script>

<div class="ui-error-state" class:ui-error-state-full={fullHeight}>
  <span class="ui-error-state-icon" aria-hidden="true">!</span>
  <span class="ui-error-state-text">{displayMessage}</span>
  {#if onRetry}
    <button class="btn btn-ghost ui-error-state-retry" onclick={onRetry}>
      {retryLabel ?? t("ui.errorState.retryButton")}
    </button>
  {/if}
</div>

<style>
  .ui-error-state {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 8px 10px;
    color: var(--neg);
    background: var(--neg-bg);
    border: 1px solid var(--neg-border);
    border-radius: var(--radius-sm);
    font-size: 11px;
  }
  .ui-error-state-full {
    height: 100%;
    justify-content: center;
  }
  .ui-error-state-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: hsl(0 84% 65% / 0.2);
    font-weight: 700;
    flex-shrink: 0;
  }
  .ui-error-state-text {
    flex: 1;
    min-width: 0;
    line-height: 1.4;
  }
  .ui-error-state-retry {
    height: 22px;
    padding: 0 var(--space-2);
    font-size: 10.5px;
    flex-shrink: 0;
  }
</style>
