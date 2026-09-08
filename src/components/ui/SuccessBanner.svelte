<!--
  Design System gap-fill (Phase D9, `STUDIO_PLAN.md`): a real full-sentence
  positive-outcome banner — the mirror image of `ErrorState.svelte`, minus
  the retry action (a success message never needs one). Every Phase D7a/D7b
  retrofit that hit a full-sentence success message (`ExportDialog.svelte`'s
  render-complete, `ModelManagerDialog.svelte`'s download-complete,
  `SystemInfoDialog.svelte`'s copy-done confirmation, `CapCutExportDialog.svelte`'s
  export-complete/validation-healthy/no-warnings messages,
  `AiSettingsDialog.svelte`'s test-connection-succeeded message) documented
  "no success banner component exists yet" and left it as its own bespoke
  colored `<p>` — `.rd-success`/`.mm-success`/`.si-copy-done`/`.ce-success`/
  `.ce-ok`/`.as-test-ok`, all independently converging on the same recipe
  (padding 8-10px, `var(--pos)` text, a tinted background, `var(--radius-sm)`)
  sampled before writing this. Deliberately NOT used for
  `UpdateSettingsDialog.svelte`'s status line — that text is multi-state
  (idle/disabled/up-to-date/available/deferred/check-failed/installing), not
  a single positive outcome, so a "success" banner isn't the right fit for it
  (matches that dialog's own Phase D7b note that `Badge` wasn't a fit either,
  for the same "not just a short pill" reason).
-->
<script lang="ts">
  let { message }: { message: string } = $props();
</script>

<div class="ui-success-banner">
  <span class="ui-success-banner-icon" aria-hidden="true">✓</span>
  <span class="ui-success-banner-text">{message}</span>
</div>

<style>
  .ui-success-banner {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 8px 10px;
    color: var(--pos);
    background: var(--pos-bg);
    border: 1px solid var(--pos-border);
    border-radius: var(--radius-sm);
    font-size: 11px;
  }
  .ui-success-banner-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--pos-bg);
    font-weight: 700;
    flex-shrink: 0;
  }
  .ui-success-banner-text {
    flex: 1;
    min-width: 0;
    line-height: 1.4;
    word-break: break-word;
  }
</style>
