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
        <button class="btn btn-ghost" onclick={() => firstRunWizardStore.close()} title={t("firstRunWizard.closeTooltip")}>×</button>
      </div>

      <div class="frw-progress-track">
        <div
          class="frw-progress-fill"
          style="width:{(firstRunWizardStore.stepNumber / firstRunWizardStore.totalSteps) * 100}%"
        ></div>
      </div>

      <div class="frw-body">
        <h2 class="frw-step-title">{t(stepTitleKey(firstRunWizardStore.currentStep))}</h2>

        {#if firstRunWizardStore.currentStep === "welcome"}
          <p class="frw-p">{t("firstRunWizard.welcomeBody")}</p>

        {:else if firstRunWizardStore.currentStep === "systemCheck"}
          <p class="frw-p">{t("firstRunWizard.systemCheckBody")}</p>
          {#if firstRunWizardStore.systemInfoError}
            <div class="frw-error">{t("firstRunWizard.systemCheckErrorPrefix", { error: firstRunWizardStore.systemInfoError })}</div>
          {/if}
          {#if firstRunWizardStore.systemInfoLoading && !firstRunWizardStore.systemInfo}
            <p class="frw-p muted-2">{t("firstRunWizard.systemCheckLoading")}</p>
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
            <p class="frw-p muted-2">{t("firstRunWizard.systemCheckLoading")}</p>
          {/if}

        {:else if firstRunWizardStore.currentStep === "gpu"}
          <p class="frw-p">{t("firstRunWizard.gpuBody")}</p>
          {#if renderStore.hardwareError}
            <div class="frw-error">{t("firstRunWizard.gpuErrorPrefix", { error: renderStore.hardwareError })}</div>
          {/if}
          {#if renderStore.hardwareLoading && !renderStore.hardware}
            <p class="frw-p muted-2">{t("firstRunWizard.gpuLoading")}</p>
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
                    <span class:frw-badge-ok={enc.working} class:frw-badge-warn={!enc.working} class="frw-badge">
                      {enc.working ? t("firstRunWizard.gpuWorkingBadge") : t("firstRunWizard.gpuNotWorkingBadge")}
                    </span>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}

        {:else if firstRunWizardStore.currentStep === "capcut"}
          <p class="frw-p">{t("firstRunWizard.capcutBody")}</p>
          {#if capcutStore.detectError}
            <div class="frw-error">{t("firstRunWizard.capcutErrorPrefix", { error: capcutStore.detectError })}</div>
          {/if}
          {#if capcutStore.detectLoading && capcutStore.installations.length === 0}
            <p class="frw-p muted-2">{t("firstRunWizard.capcutLoading")}</p>
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
          <button class="btn btn-ghost btn-sm" onclick={() => capcutStore.openSettings()}>
            {t("firstRunWizard.capcutConfigureButton")}
          </button>

        {:else if firstRunWizardStore.currentStep === "aiProvider"}
          <p class="frw-p">{t("firstRunWizard.aiProviderBody")}</p>
          <p class="frw-p">
            {#if aiSettingsStore.hasKeyConfigured}
              <strong class="frw-status-ok">{t("firstRunWizard.aiProviderConfiguredLabel", { provider: providerLabel(aiSettingsStore.provider) })}</strong>
            {:else}
              <span class="muted-2">{t("firstRunWizard.aiProviderNotConfiguredLabel")}</span>
            {/if}
          </p>
          <button class="btn btn-sm" onclick={() => aiSettingsStore.openDialog()}>
            {t("firstRunWizard.aiProviderConfigureButton")}
          </button>

        {:else if firstRunWizardStore.currentStep === "transcriptionModel"}
          <p class="frw-p">{t("firstRunWizard.transcriptionModelBody")}</p>
          {#if modelManagerStore.loadError}
            <div class="frw-error">{modelManagerStore.loadError}</div>
          {/if}
          {#if modelManagerStore.loading && modelManagerStore.available.length === 0}
            <p class="frw-p muted-2">{t("firstRunWizard.transcriptionModelLoading")}</p>
          {:else}
            <div class="frw-list">
              {#each modelManagerStore.modelsView as m (m.entry.id)}
                <div class="frw-model-row">
                  <div class="frw-model-info">
                    <span class="frw-model-name">{m.entry.display_name}</span>
                    <span class="muted-2 frw-model-size">{formatBytes(m.installedSizeBytes ?? m.entry.approx_size_bytes)}</span>
                  </div>
                  {#if m.downloading}
                    <span class="frw-badge frw-badge-warn">{t("firstRunWizard.transcriptionModelDownloading")}</span>
                  {:else if m.installed}
                    <span class="frw-badge frw-badge-ok">{t("firstRunWizard.transcriptionModelInstalledBadge")}</span>
                  {:else}
                    <button class="btn btn-ghost btn-sm" onclick={() => void modelManagerStore.download(m.entry.id)}>
                      {t("firstRunWizard.transcriptionModelDownloadButton")}
                    </button>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

        {:else if firstRunWizardStore.currentStep === "projectFolder"}
          <p class="frw-p">{t("firstRunWizard.projectFolderBody")}</p>
          <p class="frw-p mono">
            {projectFolderStore.path ?? t("firstRunWizard.projectFolderNoneChosen")}
          </p>
          <div class="frw-row">
            <button class="btn btn-sm" onclick={() => void projectFolderStore.browse()}>
              {t("firstRunWizard.projectFolderChooseButton")}
            </button>
            {#if projectFolderStore.path}
              <button class="btn btn-ghost btn-sm" onclick={() => projectFolderStore.clear()}>
                {t("firstRunWizard.projectFolderClearButton")}
              </button>
            {/if}
          </div>

        {:else if firstRunWizardStore.currentStep === "ready"}
          <p class="frw-p">{t("firstRunWizard.readyBody")}</p>
          <div class="frw-row frw-row-wrap">
            <button class="btn btn-ghost btn-sm" onclick={() => modelManagerStore.openDialog()}>{t("topBar.modelManagerButton")}</button>
            <button class="btn btn-ghost btn-sm" onclick={() => capcutStore.openSettings()}>{t("topBar.capcutSettingsButton")}</button>
            <button class="btn btn-ghost btn-sm" onclick={() => aiSettingsStore.openDialog()}>{t("topBar.aiSettingsButton")}</button>
            <button class="btn btn-ghost btn-sm" onclick={() => systemInfoStore.openDialog()}>{t("topBar.systemInfoButton")}</button>
          </div>
          <p class="frw-p muted-2">{t("firstRunWizard.readyReopenHint")}</p>
        {/if}
      </div>

      <div class="frw-footer">
        {#if !firstRunWizardStore.isFirstStep}
          <button class="btn btn-ghost" onclick={() => firstRunWizardStore.back()}>{t("firstRunWizard.backButton")}</button>
        {/if}
        <span class="frw-footer-spacer"></span>

        {#if firstRunWizardStore.currentStep === "welcome"}
          <button class="btn btn-ghost" onclick={() => firstRunWizardStore.finish()}>{t("firstRunWizard.welcomeSkipSetup")}</button>
          <button class="btn" onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.welcomeGetStarted")}</button>
        {:else if firstRunWizardStore.currentStep === "aiProvider" || firstRunWizardStore.currentStep === "transcriptionModel" || firstRunWizardStore.currentStep === "projectFolder"}
          <button class="btn btn-ghost" onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.skipButton")}</button>
          <button class="btn" onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.continueButton")}</button>
        {:else if firstRunWizardStore.currentStep === "ready"}
          <button class="btn" onclick={() => firstRunWizardStore.finish()}>{t("firstRunWizard.finishButton")}</button>
        {:else}
          <button class="btn" onclick={() => firstRunWizardStore.next()}>{t("firstRunWizard.nextButton")}</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
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
  .frw-progress-track {
    height: 3px;
    background: var(--surface-2);
    flex-shrink: 0;
  }
  .frw-progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms ease;
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
  .frw-badge {
    font-size: 10.5px;
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .frw-badge-ok {
    color: var(--pos, #3fb950);
    background: hsl(140 60% 50% / 0.1);
  }
  .frw-badge-warn {
    color: var(--warn, #d29922);
    background: hsl(38 92% 60% / 0.1);
  }
  .frw-model-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
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
  .btn-sm {
    height: 24px;
    padding: 0 10px;
    font-size: 11px;
  }
  .frw-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
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
