<!--
  Export dialog (Phase 6, master prompt §32/§33/§43/§44). Mounted once in
  `App.svelte` (not inside `Timeline.svelte`/`TopBar.svelte` themselves) since
  it's opened from two places — `TopBar.svelte`'s File menu ("File > Export…")
  and a toolbar button in `Timeline.svelte`, mirroring Phase 5's
  `SilenceDetector`/`SyncGroupDialog` two-entry-point precedent (see each
  caller's own comment) — and a single shared `renderStore` instance should
  back both rather than each mounting its own dialog copy.

  Pure UI over `stores/render.svelte.ts`: preset picker seeds the settings
  form, every setting is independently overridable, hardware encoders are
  detected lazily on open, output path uses `save()` (a destination file),
  and Export/Cancel/progress wire to the real `start_render_job`/
  `cancel_render_job` commands + the `render:progress` event — no
  client-side progress simulation.
-->
<script lang="ts">
  import { renderStore, X264_PRESETS } from "../../stores/render.svelte";
  import { t } from "../../lib/i18n.svelte";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      renderStore.close();
    }
  }

  function formatPercent(fraction: number | null): string {
    return fraction !== null ? `${Math.round(fraction * 100)}%` : "…";
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() || path;
  }

  function encoderLabel(backend: string): string {
    switch (backend) {
      case "nvenc":
        return "NVIDIA NVENC";
      case "quick_sync":
        return "Intel Quick Sync";
      case "amf":
        return "AMD AMF";
      default:
        return t("exportDialog.hwSoftware");
    }
  }
</script>

{#if renderStore.open}
  <div class="rd-backdrop" role="presentation" onclick={() => renderStore.close()}>
    <div
      class="rd-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("exportDialog.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="rd-header">
        <span class="rd-title">{t("exportDialog.title")}</span>
        <button class="btn btn-ghost" onclick={() => renderStore.close()} title={t("exportDialog.close")}>×</button>
      </div>

      <div class="rd-body">
        <section class="rd-section">
          <h3 class="rd-section-title">{t("exportDialog.presetSectionTitle")}</h3>
          {#if renderStore.presetsError}
            <div class="rd-error">{renderStore.presetsError}</div>
          {/if}
          <div class="rd-preset-grid">
            {#each renderStore.presets as preset (preset.id)}
              <button
                class="rd-preset-card"
                class:selected={renderStore.selectedPresetId === preset.id}
                onclick={() => renderStore.selectPreset(preset.id)}
              >
                <span class="rd-preset-name">{preset.name}</span>
                <span class="rd-preset-desc muted-2">{preset.description}</span>
              </button>
            {/each}
            {#if renderStore.presetsLoading}
              <span class="muted-2">{t("exportDialog.loadingPresets")}</span>
            {/if}
          </div>
        </section>

        <section class="rd-section">
          <h3 class="rd-section-title">{t("exportDialog.settingsSectionTitle")}</h3>

          <div class="rd-row">
            <label class="rd-label" for="rd-width">{t("exportDialog.resolutionLabel")}</label>
            <input id="rd-width" class="rd-number rd-number-sm" type="number" min="2" step="2" bind:value={renderStore.width} />
            <span class="rd-x muted-2">×</span>
            <input aria-label={t("exportDialog.heightLabel")} class="rd-number rd-number-sm" type="number" min="2" step="2" bind:value={renderStore.height} />
          </div>

          <div class="rd-row">
            <label class="rd-label" for="rd-fps">{t("exportDialog.fpsLabel")}</label>
            <select
              id="rd-fps"
              class="rd-select"
              value={renderStore.fpsSelectValue}
              onchange={(e) => renderStore.setFpsByKey((e.target as HTMLSelectElement).value)}
            >
              {#each renderStore.fpsSelectOptions as opt (opt.key)}
                <option value={opt.key}>{opt.label}</option>
              {/each}
            </select>
          </div>

          <div class="rd-row">
            <label class="rd-label" for="rd-container">{t("exportDialog.containerLabel")}</label>
            <select
              id="rd-container"
              class="rd-select"
              value={renderStore.container}
              onchange={(e) => renderStore.setContainer((e.target as HTMLSelectElement).value as "mp_4" | "web_m")}
            >
              <option value="mp_4">MP4</option>
              <option value="web_m">WebM</option>
            </select>
          </div>

          <div class="rd-row">
            <label class="rd-label" for="rd-video-codec">{t("exportDialog.videoCodecLabel")}</label>
            <select id="rd-video-codec" class="rd-select" bind:value={renderStore.videoCodec}>
              {#each renderStore.videoCodecOptions as codec (codec)}
                <option value={codec}>{codec === "h264" ? "H.264" : codec === "h265" ? "H.265" : "VP9"}</option>
              {/each}
            </select>
          </div>

          {#if renderStore.videoCodec === "h264" || renderStore.videoCodec === "h265"}
            <div class="rd-row">
              <label class="rd-label" for="rd-x264-preset">{t("exportDialog.encodeSpeedLabel")}</label>
              <select id="rd-x264-preset" class="rd-select" bind:value={renderStore.x264Preset}>
                {#each X264_PRESETS as p (p)}
                  <option value={p}>{p}</option>
                {/each}
              </select>
            </div>
          {/if}

          <div class="rd-row">
            <span class="rd-label">{t("exportDialog.qualityModeLabel")}</span>
            <div class="rd-radio-group">
              <label class="rd-radio">
                <input type="radio" name="rd-bitrate-mode" value="crf" checked={renderStore.bitrateMode === "crf"} onchange={() => (renderStore.bitrateMode = "crf")} />
                {t("exportDialog.qualityModeCrf")}
              </label>
              <label class="rd-radio">
                <input type="radio" name="rd-bitrate-mode" value="bitrate" checked={renderStore.bitrateMode === "bitrate"} onchange={() => (renderStore.bitrateMode = "bitrate")} />
                {t("exportDialog.qualityModeBitrate")}
              </label>
            </div>
          </div>

          {#if renderStore.bitrateMode === "crf"}
            <div class="rd-row">
              <label class="rd-label" for="rd-crf">{t("exportDialog.crfLabel")}</label>
              <input id="rd-crf" class="rd-number rd-number-sm" type="number" min="0" max="51" bind:value={renderStore.crf} />
              <span class="rd-hint muted-2">{t("exportDialog.crfHint")}</span>
            </div>
          {:else}
            <div class="rd-row">
              <label class="rd-label" for="rd-video-bitrate">{t("exportDialog.videoBitrateLabel")}</label>
              <input id="rd-video-bitrate" class="rd-number" type="number" min="1" bind:value={renderStore.videoBitrateKbps} />
              <span class="rd-hint muted-2">kbps</span>
            </div>
          {/if}

          <div class="rd-row">
            <label class="rd-label" for="rd-audio-codec">{t("exportDialog.audioCodecLabel")}</label>
            <select id="rd-audio-codec" class="rd-select" bind:value={renderStore.audioCodec}>
              {#each renderStore.audioCodecOptions as codec (codec)}
                <option value={codec}>{codec.toUpperCase()}</option>
              {/each}
            </select>
          </div>

          <div class="rd-row">
            <label class="rd-label" for="rd-audio-bitrate">{t("exportDialog.audioBitrateLabel")}</label>
            <input id="rd-audio-bitrate" class="rd-number" type="number" min="1" bind:value={renderStore.audioBitrateKbps} />
            <span class="rd-hint muted-2">kbps</span>
          </div>
        </section>

        <section class="rd-section">
          <h3 class="rd-section-title">{t("exportDialog.hwSectionTitle")}</h3>
          {#if renderStore.hardwareLoading}
            <p class="rd-empty muted-2">{t("exportDialog.hwDetecting")}</p>
          {:else if renderStore.hardwareError}
            <div class="rd-error">{renderStore.hardwareError}</div>
          {:else if renderStore.hardware}
            <p class="rd-hw-active">{t("exportDialog.hwActiveEncoder", { label: renderStore.hardware.active_encoder_label })}</p>
            <div class="rd-row">
              <label class="rd-label" for="rd-hw-encoder">{t("exportDialog.hwForceLabel")}</label>
              <select
                id="rd-hw-encoder"
                class="rd-select"
                value={renderStore.hardwareEncoder ?? "auto"}
                onchange={(e) => {
                  const v = (e.target as HTMLSelectElement).value;
                  renderStore.hardwareEncoder = v === "auto" ? null : (v as "software" | "nvenc" | "quick_sync" | "amf");
                }}
              >
                <option value="auto">{t("exportDialog.hwAuto")}</option>
                <option value="software">{t("exportDialog.hwSoftware")}</option>
                {#each renderStore.detectedWorkingEncoders as enc (enc.backend)}
                  {#if enc.backend !== "software"}
                    <option value={enc.backend}>{encoderLabel(enc.backend)}</option>
                  {/if}
                {/each}
              </select>
            </div>
            {#if renderStore.hardware.encoders.length > 0}
              <ul class="rd-hw-list muted-2">
                {#each renderStore.hardware.encoders as enc (enc.backend)}
                  <li>{encoderLabel(enc.backend)}: {enc.working ? t("exportDialog.hwWorking") : t("exportDialog.hwNotAvailable")}</li>
                {/each}
              </ul>
            {/if}
          {/if}
        </section>

        <section class="rd-section">
          <h3 class="rd-section-title">{t("exportDialog.outputSectionTitle")}</h3>
          <div class="rd-row">
            <button class="btn" onclick={() => void renderStore.chooseOutputPath()}>{t("exportDialog.chooseOutputButton")}</button>
            <span class="rd-output-path muted-2" title={renderStore.outputPath ?? undefined}>
              {renderStore.outputPath ? basename(renderStore.outputPath) : t("exportDialog.noOutputChosen")}
            </span>
          </div>
        </section>

        {#if renderStore.startError}
          <div class="rd-error">{renderStore.startError}</div>
        {/if}

        {#if renderStore.progress}
          <section class="rd-section">
            <h3 class="rd-section-title">{t("exportDialog.progressSectionTitle")}</h3>
            {#if renderStore.progress.error}
              <div class="rd-error">{t("exportDialog.renderFailed", { error: renderStore.progress.error })}</div>
            {:else if renderStore.progress.done}
              <p class="rd-success">
                {t("exportDialog.renderComplete", { path: renderStore.progress.output_path ?? "" })}
              </p>
            {:else}
              <div class="rd-progress-track">
                <div
                  class="rd-progress-fill"
                  style="width:{renderStore.progress.fraction !== null ? renderStore.progress.fraction * 100 : 0}%"
                ></div>
              </div>
              <p class="rd-progress-label muted-2">
                {formatPercent(renderStore.progress.fraction)}
                {#if renderStore.progress.speed !== null}
                  · {t("exportDialog.speedLabel", { speed: renderStore.progress.speed.toFixed(2) })}
                {/if}
              </p>
            {/if}
          </section>
        {/if}
      </div>

      <div class="rd-footer">
        {#if renderStore.isRendering}
          <button class="btn btn-danger" disabled={renderStore.cancelling} onclick={() => void renderStore.cancel()}>
            {renderStore.cancelling ? t("exportDialog.cancelling") : t("exportDialog.cancelButton")}
          </button>
        {:else if renderStore.progress?.done}
          <button class="btn" onclick={() => renderStore.startNewExport()}>{t("exportDialog.newExportButton")}</button>
        {:else}
          <button class="btn" disabled={!renderStore.canExport} onclick={() => void renderStore.startExport()}>
            {renderStore.starting ? t("exportDialog.starting") : t("exportDialog.exportButton")}
          </button>
        {/if}
        <span class="rd-footer-spacer"></span>
        <button class="btn btn-ghost" onclick={() => renderStore.close()}>{t("exportDialog.closeButton")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .rd-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .rd-dialog {
    width: min(720px, 94vw);
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
  .rd-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .rd-title {
    font-size: 13px;
    font-weight: 600;
  }
  .rd-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .rd-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .rd-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .rd-preset-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 8px;
  }
  .rd-preset-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .rd-preset-card:hover { border-color: var(--border-strong); }
  .rd-preset-card.selected { border-color: var(--accent); background: hsl(213 94% 68% / 0.08); }
  .rd-preset-name { font-size: 11.5px; font-weight: 600; }
  .rd-preset-desc { font-size: 10.5px; line-height: 1.4; }
  .rd-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .rd-label {
    font-size: 11.5px;
    color: var(--muted);
    flex-shrink: 0;
    min-width: 120px;
  }
  .rd-select {
    flex: 1;
    min-width: 0;
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11.5px;
  }
  .rd-number {
    width: 100px;
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 11.5px;
  }
  .rd-number-sm { width: 76px; }
  .rd-x { flex-shrink: 0; }
  .rd-hint { font-size: 10.5px; }
  .rd-radio-group { display: flex; gap: 14px; }
  .rd-radio {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    cursor: pointer;
  }
  .rd-empty { margin: 0; font-size: 11.5px; }
  .rd-hw-active { margin: 0; font-size: 12px; font-weight: 600; }
  .rd-hw-list {
    margin: 0;
    padding-left: 18px;
    font-size: 10.5px;
    line-height: 1.6;
  }
  .rd-output-path {
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rd-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .rd-success {
    margin: 0;
    padding: 8px 10px;
    font-size: 11.5px;
    color: var(--pos, #3fb950);
    background: hsl(140 60% 50% / 0.08);
    border: 1px solid hsl(140 60% 50% / 0.3);
    border-radius: var(--radius-sm);
    word-break: break-all;
  }
  .rd-progress-track {
    height: 8px;
    background: var(--surface-2);
    border-radius: 4px;
    overflow: hidden;
  }
  .rd-progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .rd-progress-label {
    margin: 0;
    font-size: 11px;
  }
  .rd-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .rd-footer-spacer { flex: 1; }
  .btn-danger {
    background: hsl(0 84% 65% / 0.12);
    border: 1px solid hsl(0 84% 65% / 0.4);
    color: var(--neg);
  }
</style>
