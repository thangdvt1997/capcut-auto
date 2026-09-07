<!--
  Design System foundation (Phase D1, promt.md §19): a real "spinner +
  message" pattern — every existing dialog today hand-rolls its own loading
  text with no spinner at all (e.g. CapCutSettingsDialog's `{t("capcutSettings.detecting")}`
  plain-text button label, AutomationRulesDialog's plain `{t("automationRules.loading")}`
  paragraph). `message` defaults to a real i18n key when the caller doesn't
  need a more specific one.
-->
<script lang="ts">
  import { t } from "../../lib/i18n.svelte";

  let {
    message,
    fullHeight = false,
  }: {
    message?: string;
    fullHeight?: boolean;
  } = $props();

  let displayMessage = $derived(message ?? t("ui.loadingState.default"));
</script>

<div class="ui-loading-state" class:ui-loading-state-full={fullHeight}>
  <span class="ui-spinner" aria-hidden="true"></span>
  <span class="muted-2">{displayMessage}</span>
</div>

<style>
  .ui-loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-4);
    font-size: 11.5px;
  }
  .ui-loading-state-full {
    height: 100%;
  }
  .ui-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: ui-spin 0.7s linear infinite;
    flex-shrink: 0;
  }
  @keyframes ui-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
