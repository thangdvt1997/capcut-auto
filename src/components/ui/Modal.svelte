<!--
  Design System foundation (Phase D1, promt.md §19): the real backdrop/
  dialog/header/close-button/footer shell every one of the app's 18
  existing dialogs currently hand-rolls independently. Sampled
  CapCutSettingsDialog.svelte, AutomationRulesDialog.svelte, and
  BatchJobsDialog.svelte's own backdrop/dialog markup + `role="dialog"`/
  `aria-modal`/Escape-key-handling before writing this — all three are
  byte-for-byte the same shape (`.xx-backdrop` fixed/inset-0/dimmed/grid-
  centered; `.xx-dialog` width min(Npx,94vw)/max-height 88vh/flex-column/
  var(--surface)/var(--border-strong)/var(--radius-lg)/box-shadow; header
  with title + `×` close button; scrollable body; footer with border-top).
  This component is that exact shape, parameterized by `width`/`title`, with
  `children` as the body and an optional `footer` snippet.

  Not wired into any existing dialog yet (out of scope this pass — a later
  retrofit pass can delete each dialog's own ~40-60 lines of this same
  chrome and replace it with `<Modal>` + slotted content, per STUDIO_PLAN.md
  Phase D1/D7).
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "../../lib/i18n.svelte";

  let {
    open,
    title,
    width = 640,
    onClose,
    footer,
    children,
  }: {
    open: boolean;
    title: string;
    width?: number;
    onClose: () => void;
    footer?: Snippet;
    children: Snippet;
  } = $props();

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <div class="ui-modal-backdrop" role="presentation" onclick={onClose}>
    <div
      class="ui-modal"
      style="width: min({width}px, 94vw);"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="ui-modal-header">
        <span class="ui-modal-title">{title}</span>
        <button class="btn btn-ghost" onclick={onClose} title={t("ui.modal.close")} aria-label={t("ui.modal.close")}>
          ×
        </button>
      </div>
      <div class="ui-modal-body">
        {@render children()}
      </div>
      {#if footer}
        <div class="ui-modal-footer">
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .ui-modal-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .ui-modal {
    max-height: 88vh;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
    overflow: hidden;
  }
  .ui-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ui-modal-title {
    font-size: 13px;
    font-weight: 600;
  }
  .ui-modal-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .ui-modal-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
</style>
