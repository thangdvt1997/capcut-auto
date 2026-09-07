<!--
  First-Run Wizard (Phase 12, master prompt §58): the exact 9-step sequence
  Welcome -> System Check -> FFmpeg -> GPU Detection -> CapCut Detection ->
  AI Provider (optional) -> Transcription Model (optional) -> Project Folder
  -> Ready. Shown automatically on first launch, gated by a `localStorage`
  "completed" flag (`stores/firstRunWizard.svelte.ts`); reachable manually
  afterwards from a "Setup Wizard…" button in `TopBar.svelte`.

  Every step's real data/actions come from an already-real store — this
  component only orchestrates *when* each store's detection runs (on
  entering the relevant step) and lays out the step content; it never
  re-implements detection/download/settings logic that already exists
  elsewhere:
    - System Check / FFmpeg: `stores/firstRunWizard.svelte.ts`'s own
      `get_system_information` fetch (the one new data source this pass
      adds — every other step reuses something that already existed before
      this pass).
    - GPU Detection: `stores/render.svelte.ts`'s `hardware`/
      `ensureHardwareDetected()` (Phase 6 — same data `ExportDialog.svelte`
      shows).
    - CapCut Detection: `stores/capcut.svelte.ts`'s `installations`/
      `ensureDetected()` (Phase 9) — a "Configure / Override…" button opens
      the real `CapCutSettingsDialog` for anything beyond simple detection.
    - AI Provider: `stores/aiSettings.svelte.ts` — this step does not
      duplicate the provider/key/model form at all, it only shows a status
      line and a button that opens the real `AiSettingsDialog`.
    - Transcription Model: `stores/modelManager.svelte.ts`'s `modelsView`/
      `download()` (Phase 7) — a condensed list embedded directly (not a
      link-out), since the task brief calls out that this store "already
      fetches models list" for exactly this purpose.
    - Project Folder: `stores/projectFolder.svelte.ts` — a brand-new,
      first-of-its-kind concept in this codebase (no Project Manager exists
      anywhere yet), explicitly scoped as a lightweight default
      save-browsing location, not a real enforced project directory. See
      that store's own doc comment.

  AI Provider and Transcription Model are both clearly optional: each has an
  explicit "Skip" affordance in the footer, and skipping either (or both)
  never blocks reaching "Ready" or using the basic editor afterward — master
  prompt §58's "AI configuration must be optional" / "the basic editor
  should work without cloud AI."

  **Phase D7a Design System retrofit (`STUDIO_PLAN.md`):** every button is
  now `Button.svelte`/`IconButton.svelte` (Phase D1), every "Working"/"Not
  working"/"Installed"/"Downloading" pill is `Badge.svelte`, the top step
  progress strip is `ProgressBar.svelte`, every error banner is
  `ErrorState.svelte`, every "detecting.../loading..." one-liner is
  `LoadingState.svelte`, and the Transcription Model step's per-model rows
  are `Card.svelte` (matching `ModelManagerDialog.svelte`'s own retrofit).
  Every real behavior — every store call, the `$effect` that lazily triggers
  each step's detection, every Skip/Back/Continue/Finish condition — is
  unchanged, only the markup underneath it.

  **Deliberately NOT switched to the shared `Modal.svelte` shell (a real,
  documented exception, not an oversight):** this wizard's own backdrop is
  hardcoded to `z-index: 90`, one below every standalone dialog's own 100 —
  because several wizard steps open one of those dialogs (CapCutSettingsDialog/
  AiSettingsDialog/ModelManagerDialog/SystemInfoDialog) as a sub-action, and
  those must render ON TOP of the wizard, not underneath it (see the original
  z-index comment, preserved below). `Modal.svelte` hardcodes `z-index: 100`
  with no prop to override it, and since those four dialogs are *also*
  `Modal.svelte` instances mounted later in `App.svelte`'s DOM order than
  this wizard, giving this wizard the same fixed z-index would make it paint
  ON TOP of them instead — a genuine stacking-order regression, not a purely
  visual one. Changing `Modal.svelte`'s own public API (e.g. adding a
  `zIndex` prop) was judged out of scope for this pass, since it's a shared
  component several other concurrently-retrofitted dialogs already depend on
  in this same working tree. So the backdrop/dialog shell, its own Escape
  handler, and the header/close-button/step-indicator stay hand-rolled,
  exactly as before. The checklist icons (`.frw-check-icon`) and the
  key/value FFmpeg detail list (`.frw-kv`) also stay bespoke — neither is a
  text-pill "status badge" or an actionable button/select/table, so nothing
  in the Design System maps onto either without changing what they actually
  look like.
-->
<script lang="ts">
  import { firstRunWizardStore, type WizardStep } from "../../stores/firstRunWizard.svelte";
  import { renderStore } from "../../stores/render.svelte";
  import { capcutStore } from "../../stores/capcut.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { modelManagerStore } from "../../stores/modelManager.svelte";
  import { projectFolderStore } from "../../stores/projectFolder.svelte";
  import { systemInfoStore } from "../../stores/systemInfo.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { AiProviderKind } from "../../types/bindings";
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Badge from "../ui/Badge.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import LoadingState from "../ui/LoadingState.svelte";
  import Card from "../ui/Card.svelte";

  const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
    const value = bytes / 1024 ** exp;
    return `${exp === 0 ? value.toFixed(0) : value.toFixed(1)} ${BYTE_UNITS[exp]}`;
  }

  function providerLabel(kind: AiProviderKind): string {
    switch (kind) {
      case "open_ai":
        return t("aiSettings.providerOpenAi");
      case "ollama":
        return t("aiSettings.providerOllama");
      case "custom_open_ai_compatible":
        return t("aiSettings.providerCustom");
      case "anthropic":
        return t("aiSettings.providerAnthropic");
      case "gemini":
        return t("aiSettings.providerGemini");
    }
  }

  // Lazily trigger each step's real detection exactly once on entering it —
  // every underlying store method already guards against redundant reloads
  // (`ensureHardwareDetected`/`ensureDetected`'s own `if (... already loaded)
  // return` checks), so re-running this effect on every step change is safe.
  $effect(() => {
    const step: WizardStep = firstRunWizardStore.currentStep;
    if (step === "gpu") {
      void renderStore.ensureHardwareDetected();
    } else if (step === "capcut") {
      void capcutStore.ensureDetected();
    } else if (step === "transcriptionModel" && modelManagerStore.available.length === 0) {
      void modelManagerStore.refresh();
    }
  });

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      firstRunWizardStore.close();
    }
  }

  function stepTitleKey(step: WizardStep): string {
    switch (step) {
      case "welcome":
        return "firstRunWizard.welcomeTitle";
      case "systemCheck":
        return "firstRunWizard.systemCheckTitle";
      case "ffmpeg":
        return "firstRunWizard.ffmpegTitle";
      case "gpu":
        return "firstRunWizard.gpuTitle";
      case "capcut":
        return "firstRunWizard.capcutTitle";
      case "aiProvider":
        return "firstRunWizard.aiProviderTitle";
      case "transcriptionModel":
        return "firstRunWizard.transcriptionModelTitle";
      case "projectFolder":
        return "firstRunWizard.projectFolderTitle";
      case "ready":
        return "firstRunWizard.readyTitle";
    }
  }
</script>

{#if firstRunWizardStore.open}
  <div class="frw-backdrop" role="presentation">
    <div
      class="frw-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("firstRunWizard.title")}
      tabindex="-1"
      onkeydown={onKeydown}
    >
      <div class="frw-header">
        <div class="frw-header-text">
          <span class="frw-title">{t("firstRunWizard.title")}</span>
          <span class="frw-step-indicator muted-2">
            {t("firstRunWizard.stepIndicator", { current: firstRunWizardStore.stepNumber, total: firstRunWizardStore.totalSteps })}
          </span>
        </div>
        <IconButton ariaLabel={t("firstRunWizard.closeTooltip")} onclick={() => firstRunWizardStore.close()}>×</IconButton>
      </div>

      <ProgressBar value={firstRunWizardStore.stepNumber} max={firstRunWizardStore.totalSteps} />

      <div class="frw-body">
        <h2 class="frw-step-title">{t(stepTitleKey(firstRunWizardStore.currentStep))}</h2>

        {#if firstRunWizardStore.currentStep === "welcome"}
          <p class="frw-p">{t("firstRunWizard.welcomeBody")}</p>

        {:else if firstRunWizardStore.currentStep === "systemCheck"}
          <p class="frw-p">{t("firstRunWizard.systemCheckBody")}</p>
          {#if firstRunWizardStore.systemInfoError}
            <ErrorState message={t("firstRunWizard.systemCheckErrorPrefix", { error: firstRunWizardStore.systemInfoError })} />
          {/if}
          {#if firstRunWizardStore.systemInfoLoading && !firstRunWizardStore.systemInfo}
            <LoadingState message={t("firstRunWizard.systemCheckLoading")} />
          {/if}
          {#if firstRunWizardStore.systemInfo}
            {@const info = firstRunWizardStore.systemInfo}
            <ul class="frw-checklist">
              <li>
                <span class="frw-check-icon" class:frw-check-ok={info.ffmpeg_version !== "not found"}>{info.ffmpeg_version !== "not found" ? "✓" : "!"}</span>
                {info.ffmpeg_version !== "not found" ? t("firstRunWizard.checkFfmpegOk") : t("firstRunWizard.checkFfmpegMissing")}
              </li>
              <li>
                <span class="frw-check-icon" class:frw-check-ok={info.hardware_encoders.some((e) => e.working)}>
                  {info.hardware_encoders.some((e) => e.working) ? "✓" : "!"}
                </span>
                {info.hardware_encoders.some((e) => e.working) ? t("firstRunWizard.checkEncoderOk") : t("firstRunWizard.checkEncoderSoftwareOnly")}
              </li>
              <li>
                <span class="frw-check-icon" class:frw-check-ok={info.capcut_installations.length > 0}>
                  {info.capcut_installations.length > 0 ? "✓" : "·"}
                </span>
                {info.capcut_installations.length > 0 ? t("firstRunWizard.checkCapcutFound") : t("firstRunWizard.checkCapcutNotFound")}
              </li>
              <li>
                <span class="frw-check-icon frw-check-ok">✓</span>
                {t("firstRunWizard.checkOsLabel")}: {info.os_version ?? info.os}
              </li>
              <li>
                <span class="frw-check-icon frw-check-ok">✓</span>
                {t("firstRunWizard.checkCpuLabel")}: {info.cpu_brand ?? "?"} ({info.cpu_core_count})
              </li>
              <li>
                <span class="frw-check-icon frw-check-ok">✓</span>
                {t("firstRunWizard.checkRamLabel")}: {formatBytes(info.total_memory_bytes)}
              </li>
            </ul>
          {/if}

        {:else if firstRunWizardStore.currentStep === "ffmpeg"}
          <p class="frw-p">{t("firstRunWizard.ffmpegBody")}</p>
          {#if firstRunWizardStore.systemInfo}
            {@const info = firstRunWizardStore.systemInfo}
            <dl class="frw-kv">
              <div class="frw-kv-row"><dt>{t("firstRunWizard.ffmpegVersionLabel")}</dt><dd>{info.ffmpeg_version}</dd></div>
              <div class="frw-kv-row"><dt>{t("firstRunWizard.ffmpegPathLabel")}</dt><dd class="mono">{info.ffmpeg_path}</dd></div>
              <div class="frw-kv-row"><dt>{t("firstRunWizard.ffprobeVersionLabel")}</dt><dd>{info.ffprobe_version}</dd></div>
              <div class="frw-kv-row"><dt>{t("firstRunWizard.ffprobePathLabel")}</dt><dd class="mono">{info.ffprobe_path}</dd></div>
            </dl>
            <p class="frw-p muted-2">{info.ffmpeg_source_note}</p>
            {#if info.ffmpeg_version === "not found"}
              <p class="frw-p">{t("firstRunWizard.ffmpegNotFoundHint")}</p>
            {/if}
          {:else}
            <LoadingState message={t("firstRunWizard.systemCheckLoading")} />
          {/if}

        {:else if firstRunWizardStore.currentStep === "gpu"}
          <p class="frw-p">{t("firstRunWizard.gpuBody")}</p>
          {#if renderStore.hardwareError}
            <ErrorState message={t("firstRunWizard.gpuErrorPrefix", { error: renderStore.hardwareError })} />
          {/if}
          {#if renderStore.hardwareLoading && !renderStore.hardware}
            <LoadingState message={t("firstRunWizard.gpuLoading")} />
          {/if}
          {#if renderStore.hardware}
            <p class="frw-p"><strong>{t("firstRunWizard.gpuActiveLabel")}:</strong> {renderStore.hardware.active_encoder_label}</p>
            {#if renderStore.hardware.encoders.length === 0}
              <p class="frw-p muted-2">{t("firstRunWizard.gpuNoneDetected")}</p>
            {:else}
              <ul class="frw-list">
                {#each renderStore.hardware.encoders as enc (enc.backend)}
                  <li>
                    {enc.label}
                    <Badge variant={enc.working ? "pos" : "warn"}>
                      {enc.working ? t("firstRunWizard.gpuWorkingBadge") : t("firstRunWizard.gpuNotWorkingBadge")}
                    </Badge>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}

        {:else if firstRunWizardStore.currentStep === "capcut"}
          <p class="frw-p">{t("firstRunWizard.capcutBody")}</p>
          {#if capcutStore.detectError}
            <ErrorState message={t("firstRunWizard.capcutErrorPrefix", { error: capcutStore.detectError })} />
          {/if}
          {#if capcutStore.detectLoading && capcutStore.installations.length === 0}
            <LoadingState message={t("firstRunWizard.capcutLoading")} />
          {:else if capcutStore.installations.length === 0}
            <p class="frw-p muted-2">{t("firstRunWizard.capcutNoneDetected")}</p>
          {:else}
            <ul class="frw-list">
              {#each capcutStore.installations as inst (inst.draft_root)}
                <li>
                  <span class="mono">{inst.draft_root}</span>
                </li>
              {/each}
            </ul>
          {/if}
          <Button variant="ghost" size="sm" onclick={() => capcutStore.openSettings()}>
            {t("firstRunWizard.capcutConfigureButton")}
          </Button>

        {:else if firstRunWizardStore.currentStep === "aiProvider"}
          <p class="frw-p">{t("firstRunWizard.aiProviderBody")}</p>
          <p class="frw-p">
            {#if aiSettingsStore.hasKeyConfigured}
              <strong class="frw-status-ok">{t("firstRunWizard.aiProviderConfiguredLabel", { provider: providerLabel(aiSettingsStore.provider) })}</strong>
            {:else}
              <span class="muted-2">{t("firstRunWizard.aiProviderNotConfiguredLabel")}</span>
            {/if}
          </p>
          <Button size="sm" onclick={() => aiSettingsStore.openDialog()}>
            {t("firstRunWizard.aiProviderConfigureButton")}
          </Button>

        {:else if firstRunWizardStore.currentStep === "transcriptionModel"}
          <p class="frw-p">{t("firstRunWizard.transcriptionModelBody")}</p>
          {#if modelManagerStore.loadError}
            <ErrorState message={modelManagerStore.loadError} />
          {/if}
          {#if modelManagerStore.loading && modelManagerStore.available.length === 0}
            <LoadingState message={t("firstRunWizard.transcriptionModelLoading")} />
          {:else}
            <div class="frw-list">
              {#each modelManagerStore.modelsView as m (m.entry.id)}
                <Card padding="sm">
                  <div class="frw-model-row">
                    <div class="frw-model-info">
                      <span class="frw-model-name">{m.entry.display_name}</span>
                      <span class="muted-2 frw-model-size">{formatBytes(m.installedSizeBytes ?? m.entry.approx_size_bytes)}</span>
                    </div>
                    {#if m.downloading}
                      <Badge variant="warn">{t("firstRunWizard.transcriptionModelDownloading")}</Badge>
                    {:else if m.installed}
                      <Badge variant="pos">{t("firstRunWizard.transcriptionModelInstalledBadge")}</Badge>
                    {:else}
                      <Button variant="ghost" size="sm" onclick={() => void modelManagerStore.download(m.entry.id)}>
                        {t("firstRunWizard.transcriptionModelDownloadButton")}
                      </Button>
                    {/if}
                  </div>
                </Card>
              {/each}
            </div>
          {/if}

        {:else if firstRunWizardStore.currentStep === "projectFolder"}
          <p class="frw-p">{t("firstRunWizard.projectFolderBody")}</p>
          <p class="frw-p mono">
            {projectFolderStore.path ?? t("firstRunWizard.projectFolderNoneChosen")}
          </p>
          <div class="frw-row">
            <Button size="sm" onclick={() => void projectFolderStore.browse()}>
              {t("firstRunWizard.projectFolderChooseButton")}
            </Button>
            {#if projectFolderStore.path}
              <Button variant="ghost" size="sm" onclick={() => projectFolderStore.clear()}>
                {t("firstRunWizard.projectFolderClearButton")}
              </Button>
            {/if}
          </div>

        {:else if firstRunWizardStore.currentStep === "ready"}
          <p class="frw-p">{t("firstRunWizard.readyBody")}</p>
          <div class="frw-row frw-row-wrap">
            <Button variant="ghost" size="sm" onclick={() => modelManagerStore.openDialog()}>{t("topBar.modelManagerButton")}</Button>
            <Button variant="ghost" size="sm" onclick={() => capcutStore.openSettings()}>{t("topBar.capcutSettingsButton")}</Button>
            <Button variant="ghost" size="sm" onclick={() => aiSettingsStore.openDialog()}>{t("topBar.aiSettingsButton")}</Button>
            <Button variant="ghost" size="sm" onclick={() => systemInfoStore.openDialog()}>{t("topBar.systemInfoButton")}</Button>
          </div>
          <p class="frw-p muted-2">{t("firstRunWizard.readyReopenHint")}</p>
        {/if}
      </div>

      <div class="frw-footer">
        {#if !firstRunWizardStore.isFirstStep}
          <Button variant="ghost" onclick={() => firstRunWizardStore.back()}>{t("firstRunWizard.backButton")}</Button>
        {/if}
        <span class="frw-footer-spacer"></span>

        {#if firstRunWizardStore.currentStep === "welcome"}
          <Button variant="ghost" onclick={() => firstRunWizardStore.finish()}>{t("firstRunWizard.welcomeSkipSetup")}</Button>
          <Button onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.welcomeGetStarted")}</Button>
        {:else if firstRunWizardStore.currentStep === "aiProvider" || firstRunWizardStore.currentStep === "transcriptionModel" || firstRunWizardStore.currentStep === "projectFolder"}
          <Button variant="ghost" onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.skipButton")}</Button>
          <Button onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.continueButton")}</Button>
        {:else if firstRunWizardStore.currentStep === "ready"}
          <Button onclick={() => firstRunWizardStore.finish()}>{t("firstRunWizard.finishButton")}</Button>
        {:else}
          <Button onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.nextButton")}</Button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* Design System retrofit (Phase D7a, `STUDIO_PLAN.md`): every button/badge/
     progress-bar/error-banner/loading-message class this file used to
     hand-roll (`.btn`/`.frw-badge*`/`.frw-progress*`/`.frw-error`/the plain
     loading `<p>`s) is gone — `Button`/`IconButton`/`Badge`/`ProgressBar`/
     `ErrorState`/`LoadingState`/`Card` (Design System) own that chrome now.
     The outer backdrop/dialog shell stays bespoke on purpose — see this
     file's own top doc comment for the real, documented z-index reason
     `Modal.svelte` isn't used here. */
  .frw-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.6);
    display: grid;
    place-items: center;
    /* Deliberately BELOW every other standalone dialog's z-index (100) —
       several wizard steps open a real dialog (CapCutSettingsDialog/
       AiSettingsDialog/ModelManagerDialog/SystemInfoDialog) as a sub-action
       (see this component's own doc comment), and that dialog must render
       and receive clicks on top of the wizard, not underneath it. */
    z-index: 90;
  }
  .frw-dialog {
    width: min(640px, 94vw);
    height: min(600px, 90vh);
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 20px 60px hsl(0 0% 0% / 0.5);
    overflow: hidden;
  }
  .frw-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .frw-header-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .frw-title {
    font-size: 13px;
    font-weight: 600;
  }
  .frw-step-indicator {
    font-size: 10.5px;
  }
  .frw-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .frw-step-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .frw-p {
    margin: 0;
    font-size: 12px;
    line-height: 1.6;
  }
  .frw-p.mono {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .frw-checklist {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
  }
  .frw-checklist li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .frw-check-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--warn, #d29922);
    font-size: 11px;
    flex-shrink: 0;
  }
  .frw-check-ok {
    color: var(--pos, #3fb950);
  }
  .frw-kv {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .frw-kv-row {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    font-size: 11.5px;
  }
  .frw-kv-row:last-child {
    border-bottom: none;
  }
  .frw-kv-row dt {
    color: var(--muted);
    font-size: 11px;
  }
  .frw-kv-row dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .frw-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .frw-list li {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
  }
  .frw-model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .frw-model-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .frw-model-name {
    font-size: 12px;
    font-weight: 600;
  }
  .frw-model-size {
    font-size: 10.5px;
  }
  .frw-status-ok {
    color: var(--pos, #3fb950);
  }
  .frw-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .frw-row-wrap {
    flex-wrap: wrap;
  }
  .frw-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .frw-footer-spacer {
    flex: 1;
  }
</style>
