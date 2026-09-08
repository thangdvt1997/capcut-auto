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

  **Phase D7a Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), each model row is
  now `Card.svelte`, the hand-rolled status pill (`.mm-status*`) is
  `Badge.svelte`, the progress track is `ProgressBar.svelte`, every button is
  `Button.svelte`, every error banner is `ErrorState.svelte`, and the initial
  "loading catalog" message is `LoadingState.svelte`. Every real behavior —
  every store call, every `disabled`/gating condition, the two-step delete
  confirm, the optimistic-progress-row download flow — is unchanged, only
  the markup underneath it. Left bespoke: the explainer paragraph (plain
  text, no component needed).

  **Phase D9 gap-fill retrofit (`STUDIO_PLAN.md`):** the "download complete"
  success line now uses the new `SuccessBanner.svelte` — the real "no success
  banner primitive exists" gap this file's own Phase D7a retrofit documented.
-->
<script lang="ts">
  import { modelManagerStore, type ModelView } from "../../stores/modelManager.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Modal from "../ui/Modal.svelte";
  import Card from "../ui/Card.svelte";
  import Badge from "../ui/Badge.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import LoadingState from "../ui/LoadingState.svelte";
  import SuccessBanner from "../ui/SuccessBanner.svelte";

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

  function statusBadgeVariant(m: ModelView): "neutral" | "pos" | "accent" {
    if (m.downloading) return "accent";
    if (m.installed) return "pos";
    return "neutral";
  }
</script>

<Modal open={modelManagerStore.open} title={t("modelManager.title")} width={620} onClose={() => modelManagerStore.close()}>
  <p class="mm-explainer muted-2">{t("modelManager.explainer")}</p>

  {#if modelManagerStore.loadError}
    <ErrorState message={t("modelManager.loadFailed", { error: modelManagerStore.loadError })} />
  {/if}

  {#if modelManagerStore.loading && modelManagerStore.available.length === 0}
    <LoadingState message={t("modelManager.loading")} />
  {/if}

  <div class="mm-list">
    {#each modelManagerStore.modelsView as m (m.entry.id)}
      <Card>
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
              <Badge variant={statusBadgeVariant(m)}>{t("modelManager.statusDownloading")}</Badge>
              <Button
                variant="ghost"
                size="sm"
                disabled={modelManagerStore.cancellingByModel[m.entry.id]}
                onclick={() => void modelManagerStore.cancelDownload(m.entry.id)}
              >
                {modelManagerStore.cancellingByModel[m.entry.id] ? t("modelManager.cancelling") : t("modelManager.cancelButton")}
              </Button>
            {:else if m.installed}
              <Badge variant={statusBadgeVariant(m)}>{t("modelManager.statusInstalled")}</Badge>
              {#if m.pendingDelete}
                <Button
                  variant="danger"
                  size="sm"
                  disabled={modelManagerStore.deletingByModel[m.entry.id]}
                  onclick={() => void modelManagerStore.confirmDelete(m.entry.id)}
                >
                  {modelManagerStore.deletingByModel[m.entry.id] ? t("modelManager.deleting") : t("modelManager.deleteConfirmButton")}
                </Button>
                <Button variant="ghost" size="sm" onclick={() => modelManagerStore.cancelDeleteRequest()}>
                  {t("modelManager.deleteCancelButton")}
                </Button>
              {:else}
                <Button variant="ghost" size="sm" onclick={() => modelManagerStore.requestDelete(m.entry.id)}>
                  {t("modelManager.deleteButton")}
                </Button>
              {/if}
            {:else}
              <Badge variant={statusBadgeVariant(m)}>{t("modelManager.statusNotInstalled")}</Badge>
              <Button size="sm" onclick={() => void modelManagerStore.download(m.entry.id)}>
                {t("modelManager.downloadButton")}
              </Button>
            {/if}
          </div>
        </div>

        {#if modelManagerStore.startErrorByModel[m.entry.id]}
          <ErrorState message={t("modelManager.downloadFailed", { error: modelManagerStore.startErrorByModel[m.entry.id] ?? "" })} />
        {/if}

        {#if m.progress}
          <div class="mm-progress-section">
            {#if m.progress.error}
              <ErrorState message={t("modelManager.downloadFailed", { error: m.progress.error })} />
              <Button variant="ghost" size="sm" onclick={() => modelManagerStore.dismissProgress(m.entry.id)}>
                {t("modelManager.dismissButton")}
              </Button>
            {:else if m.progress.done}
              <SuccessBanner message={t("modelManager.downloadComplete")} />
            {:else}
              <ProgressBar
                value={progressFraction(m)}
                max={1}
                label={`${t("modelManager.downloadedOfSize", {
                  downloaded: formatBytes(m.progress.downloaded),
                  size: formatBytes(m.progress.size),
                })} · ${formatSpeed(m.progress.speed_bytes_per_sec)} · ${t("modelManager.etaLabel", { eta: formatEta(m.progress.eta_secs) })}`}
              />
            {/if}
          </div>
        {/if}
      </Card>
    {/each}
  </div>

  {#snippet footer()}
    <span class="mm-footer-spacer"></span>
    <Button variant="ghost" onclick={() => modelManagerStore.close()}>{t("modelManager.closeButton")}</Button>
  {/snippet}
</Modal>

<style>
  /* Design System retrofit (Phase D7a/D9, `STUDIO_PLAN.md`): the dialog
     shell, status pills, buttons, error banners, loading message, progress
     track, and success message are all gone from here — `Modal`/`Card`/
     `Badge`/`Button`/`ErrorState`/`LoadingState`/`ProgressBar`/
     `SuccessBanner` (Design System) own that chrome now. Only what has no
     Design System equivalent remains: the explainer paragraph and the
     per-card internal layout (info/actions row). */
  .mm-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .mm-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
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
  .mm-progress-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mm-footer-spacer {
    flex: 1;
  }
</style>
