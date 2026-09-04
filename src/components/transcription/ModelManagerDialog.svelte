<!--
  Model Manager dialog (Phase 7, master prompt §14/§60). Pure UI over
  `stores/modelManager.svelte.ts`: lists the 5 static whisper.cpp model
  sizes (tiny/base/small/medium/large) with real size/language metadata from
  the backend catalog, install state, Download (with a real progress bar/
  speed/ETA fed by the `models:download-progress` event — no client-side
  progress simulation) + Cancel while downloading, and Delete (two-step
  confirm) once installed.

  Placement decision (documented here + `IMPLEMENTATION_PLAN.md`, since the
  concurrent Transcript Editor pass may want to link into this same dialog):
  master prompt §46's full Settings surface (General/Editing/AI/
  Transcription/Performance/Storage/CapCut/Export/Shortcuts/Updates/About)
  has not been built yet as of this pass — confirmed via `LeftPanel.svelte`/
  `TopBar.svelte`, both master-prompt-designed layout files with no Settings
  entry point anywhere. Building that whole shell just to host this one
  panel would be out of scope for this pass. Instead this is a standalone
  dialog, mounted once in `App.svelte` (matching `ExportDialog`'s own
  precedent) and reachable from:
    - `TopBar.svelte`'s "Models…" toolbar button (this pass's own entry
      point), and
    - `openModelManager()` (exported from the store module), a zero-import-
      surface convenience function any other component — e.g. the
      Transcript Editor's own "no model installed" prompt — can call
      directly without importing the store class shape.
  When Phase 7's later Settings-surface work actually lands, this dialog's
  contents can be lifted wholesale into a "Transcription" settings section;
  nothing here assumes dialog-only presentation.
-->
<script lang="ts">
  import { modelManagerStore, type ModelView } from "../../stores/modelManager.svelte";
  import { t } from "../../lib/i18n.svelte";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      modelManagerStore.close();
    }
  }

  const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
    const value = bytes / 1024 ** exp;
    return `${exp === 0 ? value : value.toFixed(1)} ${BYTE_UNITS[exp]}`;
  }

  function formatSpeed(bytesPerSec: number): string {
    return t("modelManager.speedLabel", { speed: formatBytes(bytesPerSec) });
  }

  function formatEta(secs: number | null): string {
    if (secs === null || !Number.isFinite(secs) || secs < 0) return "—";
    const total = Math.round(secs);
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function progressFraction(m: ModelView): number {
    if (!m.progress || m.progress.size <= 0) return 0;
    return Math.min(1, m.progress.downloaded / m.progress.size);
  }
</script>

{#if modelManagerStore.open}
  <div class="mm-backdrop" role="presentation" onclick={() => modelManagerStore.close()}>
    <div
      class="mm-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("modelManager.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="mm-header">
        <span class="mm-title">{t("modelManager.title")}</span>
        <button class="btn btn-ghost" onclick={() => modelManagerStore.close()} title={t("modelManager.close")}>×</button>
      </div>

      <div class="mm-body">
        <p class="mm-explainer muted-2">{t("modelManager.explainer")}</p>

        {#if modelManagerStore.loadError}
          <div class="mm-error">{t("modelManager.loadFailed", { error: modelManagerStore.loadError })}</div>
        {/if}

        {#if modelManagerStore.loading && modelManagerStore.available.length === 0}
          <p class="mm-empty muted-2">{t("modelManager.loading")}</p>
        {/if}

        <div class="mm-list">
          {#each modelManagerStore.modelsView as m (m.entry.id)}
            <div class="mm-card">
              <div class="mm-card-main">
                <div class="mm-card-info">
                  <span class="mm-name">{m.entry.display_name}</span>
                  <span class="mm-filename muted-2">{m.entry.filename}</span>
                  <span class="mm-meta muted-2">
                    {formatBytes(m.installedSizeBytes ?? m.entry.approx_size_bytes)}
                    · {m.entry.multilingual ? t("modelManager.languageMultilingual") : t("modelManager.languageEnglishOnly")}
                  </span>
                </div>

                <div class="mm-card-actions">
                  {#if m.downloading}
                    <span class="mm-status mm-status-downloading">{t("modelManager.statusDownloading")}</span>
                    <button
                      class="btn btn-ghost btn-sm"
                      disabled={modelManagerStore.cancellingByModel[m.entry.id]}
                      onclick={() => void modelManagerStore.cancelDownload(m.entry.id)}
                    >
                      {modelManagerStore.cancellingByModel[m.entry.id] ? t("modelManager.cancelling") : t("modelManager.cancelButton")}
                    </button>
                  {:else if m.installed}
                    <span class="mm-status mm-status-installed">{t("modelManager.statusInstalled")}</span>
                    {#if m.pendingDelete}
                      <button
                        class="btn btn-danger btn-sm"
                        disabled={modelManagerStore.deletingByModel[m.entry.id]}
                        onclick={() => void modelManagerStore.confirmDelete(m.entry.id)}
                      >
                        {modelManagerStore.deletingByModel[m.entry.id] ? t("modelManager.deleting") : t("modelManager.deleteConfirmButton")}
                      </button>
                      <button class="btn btn-ghost btn-sm" onclick={() => modelManagerStore.cancelDeleteRequest()}>
                        {t("modelManager.deleteCancelButton")}
                      </button>
                    {:else}
                      <button class="btn btn-ghost btn-sm" onclick={() => modelManagerStore.requestDelete(m.entry.id)}>
                        {t("modelManager.deleteButton")}
                      </button>
                    {/if}
                  {:else}
                    <span class="mm-status mm-status-not-installed">{t("modelManager.statusNotInstalled")}</span>
                    <button class="btn btn-sm" onclick={() => void modelManagerStore.download(m.entry.id)}>
                      {t("modelManager.downloadButton")}
                    </button>
                  {/if}
                </div>
              </div>

              {#if modelManagerStore.startErrorByModel[m.entry.id]}
                <div class="mm-error">{t("modelManager.downloadFailed", { error: modelManagerStore.startErrorByModel[m.entry.id] ?? "" })}</div>
              {/if}

              {#if m.progress}
                <div class="mm-progress-section">
                  {#if m.progress.error}
                    <div class="mm-error">{t("modelManager.downloadFailed", { error: m.progress.error })}</div>
                    <button class="btn btn-ghost btn-sm" onclick={() => modelManagerStore.dismissProgress(m.entry.id)}>
                      {t("modelManager.dismissButton")}
                    </button>
                  {:else if m.progress.done}
                    <p class="mm-success">{t("modelManager.downloadComplete")}</p>
                  {:else}
                    <div class="mm-progress-track">
                      <div class="mm-progress-fill" style="width:{progressFraction(m) * 100}%"></div>
                    </div>
                    <p class="mm-progress-label muted-2">
                      {t("modelManager.downloadedOfSize", {
                        downloaded: formatBytes(m.progress.downloaded),
                        size: formatBytes(m.progress.size),
                      })}
                      · {formatSpeed(m.progress.speed_bytes_per_sec)}
                      · {t("modelManager.etaLabel", { eta: formatEta(m.progress.eta_secs) })}
                    </p>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      <div class="mm-footer">
        <span class="mm-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => modelManagerStore.close()}>{t("modelManager.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .mm-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .mm-dialog {
    width: min(620px, 94vw);
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
  .mm-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .mm-title {
    font-size: 13px;
    font-weight: 600;
  }
  .mm-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .mm-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .mm-empty {
    margin: 0;
    font-size: 11.5px;
  }
  .mm-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .mm-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .mm-card-main {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
  }
  .mm-card-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .mm-name {
    font-size: 12.5px;
    font-weight: 600;
  }
  .mm-filename {
    font-size: 10.5px;
    font-family: var(--font-mono, monospace);
  }
  .mm-meta {
    font-size: 10.5px;
  }
  .mm-card-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .mm-status {
    font-size: 10.5px;
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .mm-status-installed {
    color: var(--pos, #3fb950);
    background: hsl(140 60% 50% / 0.1);
  }
  .mm-status-not-installed {
    color: var(--muted);
    background: var(--surface);
  }
  .mm-status-downloading {
    color: var(--accent);
    background: hsl(213 94% 68% / 0.1);
  }
  .btn-sm {
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
  }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
  .mm-progress-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mm-progress-track {
    height: 6px;
    background: var(--surface);
    border-radius: 3px;
    overflow: hidden;
  }
  .mm-progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .mm-progress-label {
    margin: 0;
    font-size: 10.5px;
  }
  .mm-success {
    margin: 0;
    font-size: 11px;
    color: var(--pos, #3fb950);
  }
  .mm-error {
    padding: 6px 10px;
    font-size: 10.5px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .mm-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .mm-footer-spacer {
    flex: 1;
  }
</style>
