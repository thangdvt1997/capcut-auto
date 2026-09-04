<!--
  Export to CapCut dialog (Phase 9, master prompt §31). Mode defaults to
  "Create New Draft" (a fresh subfolder, named by the user, under the
  detected/overridden draft root from `CapCutSettingsDialog.svelte`) with
  "Update Existing Draft" (browse to a real existing folder) as the
  alternative. Either path always goes through an explicit "Confirm &
  Export" step before `export_project_to_capcut_draft` is actually called —
  see `stores/capcut.svelte.ts`'s module doc comment for why this applies to
  both modes (this pass has no frontend filesystem-read capability to check
  whether a fresh "Create" name happens to collide with something real, so
  every export is treated as a potential overwrite, matching master prompt
  §30's "never overwrite user drafts without confirmation").

  Before that confirm step, a best-effort, non-blocking compatibility check
  (`capcut/compat.ts`) flags project content the Rust adapter is documented
  to not fully resolve yet (effects/animations pass through unresolved,
  keyframes outside the six supported properties are skipped) — shown as
  warnings, never as a block on exporting.

  Placement: mirrors `ExportDialog.svelte`'s own precedent — mounted once in
  `App.svelte`, reachable from `TopBar.svelte`'s File menu
  ("File > Export to CapCut…"), backed by one shared `capcutStore` instance.

  Pure UI over `stores/capcut.svelte.ts`.
-->
<script lang="ts">
  import { capcutStore } from "../../stores/capcut.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      capcutStore.closeExport();
    }
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }
</script>

{#if capcutStore.exportOpen}
  <div class="ce-backdrop" role="presentation" onclick={() => capcutStore.closeExport()}>
    <div
      class="ce-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("capcutExport.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="ce-header">
        <span class="ce-title">{t("capcutExport.title")}</span>
        <button class="btn btn-ghost" onclick={() => capcutStore.closeExport()} title={t("capcutExport.close")}>×</button>
      </div>

      <div class="ce-body">
        {#if !timeline.project}
          <p class="ce-empty muted-2">{t("capcutExport.noProject")}</p>
        {:else}
          <section class="ce-section">
            <h3 class="ce-section-title">{t("capcutExport.modeSectionTitle")}</h3>
            <div class="ce-radio-group">
              <label class="ce-radio">
                <input
                  type="radio"
                  name="ce-mode"
                  value="create"
                  checked={capcutStore.mode === "create"}
                  onchange={() => capcutStore.setMode("create")}
                />
                {t("capcutExport.modeCreate")}
              </label>
              <label class="ce-radio">
                <input
                  type="radio"
                  name="ce-mode"
                  value="update"
                  checked={capcutStore.mode === "update"}
                  onchange={() => capcutStore.setMode("update")}
                />
                {t("capcutExport.modeUpdate")}
              </label>
            </div>

            {#if !capcutStore.effectiveDraftRoot}
              <div class="ce-warn">
                {t("capcutExport.noDraftRootKnown")}
                <button class="btn btn-ghost btn-sm" onclick={() => capcutStore.openSettings()}>
                  {t("capcutExport.openSettingsButton")}
                </button>
              </div>
            {/if}

            {#if capcutStore.mode === "create"}
              <div class="ce-row">
                <label class="ce-label" for="ce-draft-name">{t("capcutExport.draftNameLabel")}</label>
                <input
                  id="ce-draft-name"
                  class="ce-input"
                  type="text"
                  placeholder={t("capcutExport.draftNamePlaceholder")}
                  bind:value={capcutStore.draftName}
                />
              </div>
            {:else}
              <div class="ce-row">
                <label class="ce-label" for="ce-existing-draft">{t("capcutExport.existingDraftLabel")}</label>
                <button id="ce-existing-draft" class="btn btn-sm" onclick={() => void capcutStore.browseExistingDraft()}>
                  {t("capcutExport.browseButton")}
                </button>
                <span class="ce-path muted-2" title={capcutStore.existingDraftPath ?? undefined}>
                  {capcutStore.existingDraftPath ? basename(capcutStore.existingDraftPath) : t("capcutExport.noExistingDraftChosen")}
                </span>
              </div>
            {/if}

            {#if capcutStore.targetPath}
              <p class="ce-target-path">
                <span class="muted-2">{t("capcutExport.targetPathLabel")}:</span>
                {capcutStore.targetPath}
              </p>
            {/if}
          </section>

          <section class="ce-section">
            <h3 class="ce-section-title">{t("capcutExport.warningsSectionTitle")}</h3>
            {#if capcutStore.compatWarnings.length === 0}
              <p class="ce-ok muted-2">{t("capcutExport.noWarnings")}</p>
            {:else}
              <ul class="ce-warn-list">
                {#each capcutStore.compatWarnings as warning (warning.key)}
                  <li>{t(warning.key, warning.params)}</li>
                {/each}
              </ul>
            {/if}
            <p class="ce-note muted-2">{t("capcutExport.limitationsNote")}</p>
          </section>

          {#if capcutStore.confirmingExport}
            <section class="ce-section ce-confirm">
              <h3 class="ce-section-title">{t("capcutExport.confirmTitle")}</h3>
              <p class="ce-confirm-body">{t("capcutExport.confirmBody")}</p>
              <p class="ce-target-path">{capcutStore.targetPath}</p>
              <p class="ce-warn-strong">{t("capcutExport.confirmOverwriteWarning")}</p>
            </section>
          {/if}

          {#if capcutStore.exportError}
            <div class="ce-error">{t("capcutExport.exportFailed", { error: capcutStore.exportError })}</div>
          {/if}

          {#if capcutStore.exportedPath}
            <p class="ce-success">{t("capcutExport.exportComplete", { path: capcutStore.exportedPath })}</p>
          {/if}
        {/if}
      </div>

      <div class="ce-footer">
        {#if capcutStore.exportedPath}
          <button class="btn" onclick={() => capcutStore.startNewExport()}>{t("capcutExport.exportAnotherButton")}</button>
        {:else if capcutStore.confirmingExport}
          <button class="btn btn-danger" disabled={capcutStore.exporting} onclick={() => void capcutStore.confirmExport()}>
            {capcutStore.exporting ? t("capcutExport.exporting") : t("capcutExport.confirmButton")}
          </button>
          <button class="btn btn-ghost" disabled={capcutStore.exporting} onclick={() => capcutStore.cancelExportConfirm()}>
            {t("capcutExport.cancelButton")}
          </button>
        {:else}
          <button class="btn" disabled={!capcutStore.canExport} onclick={() => capcutStore.requestExport()}>
            {t("capcutExport.exportButton")}
          </button>
        {/if}
        <span class="ce-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => capcutStore.closeExport()}>{t("capcutExport.close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ce-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .ce-dialog {
    width: min(640px, 94vw);
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
  .ce-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ce-title {
    font-size: 13px;
    font-weight: 600;
  }
  .ce-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .ce-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .ce-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .ce-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .ce-radio-group {
    display: flex;
    gap: 14px;
  }
  .ce-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .ce-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .ce-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
    min-width: 100px;
  }
  .ce-input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .ce-path {
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn-sm {
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
  }
  .ce-target-path {
    margin: 0;
    padding: 8px 10px;
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow-wrap: anywhere;
  }
  .ce-warn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    font-size: 11px;
    color: var(--accent);
    background: hsl(213 94% 68% / 0.08);
    border: 1px solid hsl(213 94% 68% / 0.3);
    border-radius: var(--radius-sm);
  }
  .ce-ok {
    margin: 0;
    font-size: 11.5px;
    color: var(--pos, #3fb950);
  }
  .ce-warn-list {
    margin: 0;
    padding-left: 18px;
    font-size: 11px;
    line-height: 1.6;
    color: hsl(45 90% 55%);
  }
  .ce-note {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.5;
  }
  .ce-confirm {
    padding: 10px 12px;
    background: hsl(0 84% 65% / 0.06);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .ce-confirm-body {
    margin: 0;
    font-size: 11.5px;
  }
  .ce-warn-strong {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--neg);
  }
  .ce-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .ce-success {
    margin: 0;
    padding: 8px 10px;
    font-size: 11.5px;
    color: var(--pos, #3fb950);
    background: hsl(140 60% 50% / 0.08);
    border: 1px solid hsl(140 60% 50% / 0.3);
    border-radius: var(--radius-sm);
    word-break: break-all;
  }
  .ce-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ce-footer-spacer {
    flex: 1;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
</style>
