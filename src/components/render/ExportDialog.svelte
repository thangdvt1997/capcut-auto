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

  **Phase D7a Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog shell is now `Modal.svelte` (Phase D1), each section
  heading is now `Panel.svelte`, every native `<select>` with a real, finite
  option list (fps/container/video codec/x264 preset/audio codec/hardware
  encoder) is now `Select.svelte`, every action button is `Button.svelte`,
  the two error banners (`presetsError`/`hardwareError`/`startError`/a failed
  render) are `ErrorState.svelte`, the two "detecting…"/"loading…" one-liners
  are `LoadingState.svelte`, and the render-progress track is
  `ProgressBar.svelte`. Every real behavior — every store call, every
  `disabled`/gating condition, every conditional branch — is unchanged, only
  the markup underneath it. The preset-card grid stays bespoke:
  `Card.svelte`'s `interactive` variant has no "currently selected" visual
  state to key off without changing that shared component, and the
  selected/unselected distinction here is a real, load-bearing piece of the
  preset-picker's own affordance, not just chrome.

  **Phase D9 gap-fill retrofit (`STUDIO_PLAN.md`):** the numeric fields
  (resolution width/height, CRF, video/audio bitrate) now use the new
  `NumberInput.svelte`, the CRF-vs-bitrate pair now uses the new
  `RadioGroup.svelte`, and the render-complete message now uses the new
  `SuccessBanner.svelte` — the three real gaps Phase D7a's own retrofit
  documented for this file. Every `bind:value`/`onchange`/gating condition on
  these fields is unchanged; only the markup underneath changed.
-->
<script lang="ts">
  import { renderStore, X264_PRESETS } from "../../stores/render.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Modal from "../ui/Modal.svelte";
  import Panel from "../ui/Panel.svelte";
  import Select from "../ui/Select.svelte";
  import type { SelectOption } from "../ui/Select.svelte";
  import Button from "../ui/Button.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import LoadingState from "../ui/LoadingState.svelte";
  import NumberInput from "../ui/NumberInput.svelte";
  import RadioGroup from "../ui/RadioGroup.svelte";
  import SuccessBanner from "../ui/SuccessBanner.svelte";
  import type { AudioCodec, Container, EncoderBackend, VideoCodec } from "../../types/bindings";

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

  function codecLabel(codec: VideoCodec): string {
    return codec === "h264" ? "H.264" : codec === "h265" ? "H.265" : "VP9";
  }

  const containerSelectOptions: SelectOption[] = [
    { value: "mp_4", label: "MP4" },
    { value: "web_m", label: "WebM" },
  ];

  let videoCodecSelectOptions = $derived<SelectOption[]>(
    renderStore.videoCodecOptions.map((codec) => ({ value: codec, label: codecLabel(codec) })),
  );
  let x264PresetSelectOptions: SelectOption[] = X264_PRESETS.map((p) => ({ value: p, label: p }));
  let audioCodecSelectOptions = $derived<SelectOption[]>(
    renderStore.audioCodecOptions.map((codec) => ({ value: codec, label: codec.toUpperCase() })),
  );
  let fpsSelectOptions = $derived<SelectOption[]>(
    renderStore.fpsSelectOptions.map((opt) => ({ value: opt.key, label: opt.label })),
  );
  let hwEncoderSelectOptions = $derived<SelectOption[]>([
    { value: "auto", label: t("exportDialog.hwAuto") },
    { value: "software", label: t("exportDialog.hwSoftware") },
    ...renderStore.detectedWorkingEncoders
      .filter((enc) => enc.backend !== "software")
      .map((enc) => ({ value: enc.backend, label: encoderLabel(enc.backend) })),
  ]);
</script>

<Modal open={renderStore.open} title={t("exportDialog.title")} width={720} onClose={() => renderStore.close()}>
  <Panel title={t("exportDialog.presetSectionTitle")}>
    {#if renderStore.presetsError}
      <ErrorState message={renderStore.presetsError} />
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
    </div>
    {#if renderStore.presetsLoading}
      <LoadingState message={t("exportDialog.loadingPresets")} />
    {/if}
  </Panel>

  <Panel title={t("exportDialog.settingsSectionTitle")}>
    <div class="rd-row">
      <label class="rd-label" for="rd-width">{t("exportDialog.resolutionLabel")}</label>
      <div class="rd-number-wrap-sm">
        <NumberInput id="rd-width" min={2} step={2} bind:value={renderStore.width} />
      </div>
      <span class="rd-x muted-2">×</span>
      <div class="rd-number-wrap-sm">
        <NumberInput ariaLabel={t("exportDialog.heightLabel")} min={2} step={2} bind:value={renderStore.height} />
      </div>
    </div>

    <div class="rd-row">
      <label class="rd-label" for="rd-fps">{t("exportDialog.fpsLabel")}</label>
      <div class="rd-select-wrap">
        <Select
          id="rd-fps"
          value={renderStore.fpsSelectValue}
          options={fpsSelectOptions}
          onchange={(v) => renderStore.setFpsByKey(v)}
        />
      </div>
    </div>

    <div class="rd-row">
      <label class="rd-label" for="rd-container">{t("exportDialog.containerLabel")}</label>
      <div class="rd-select-wrap">
        <Select
          id="rd-container"
          value={renderStore.container}
          options={containerSelectOptions}
          onchange={(v) => renderStore.setContainer(v as Container)}
        />
      </div>
    </div>

    <div class="rd-row">
      <label class="rd-label" for="rd-video-codec">{t("exportDialog.videoCodecLabel")}</label>
      <div class="rd-select-wrap">
        <Select
          id="rd-video-codec"
          value={renderStore.videoCodec}
          options={videoCodecSelectOptions}
          onchange={(v) => (renderStore.videoCodec = v as VideoCodec)}
        />
      </div>
    </div>

    {#if renderStore.videoCodec === "h264" || renderStore.videoCodec === "h265"}
      <div class="rd-row">
        <label class="rd-label" for="rd-x264-preset">{t("exportDialog.encodeSpeedLabel")}</label>
        <div class="rd-select-wrap">
          <Select id="rd-x264-preset" bind:value={renderStore.x264Preset} options={x264PresetSelectOptions} />
        </div>
      </div>
    {/if}

    <div class="rd-row">
      <span class="rd-label">{t("exportDialog.qualityModeLabel")}</span>
      <RadioGroup
        name="rd-bitrate-mode"
        value={renderStore.bitrateMode}
        options={[
          { value: "crf", label: t("exportDialog.qualityModeCrf") },
          { value: "bitrate", label: t("exportDialog.qualityModeBitrate") },
        ]}
        onchange={(v) => (renderStore.bitrateMode = v as "crf" | "bitrate")}
      />
    </div>

    {#if renderStore.bitrateMode === "crf"}
      <div class="rd-row">
        <label class="rd-label" for="rd-crf">{t("exportDialog.crfLabel")}</label>
        <div class="rd-number-wrap-sm">
          <NumberInput id="rd-crf" min={0} max={51} bind:value={renderStore.crf} />
        </div>
        <span class="rd-hint muted-2">{t("exportDialog.crfHint")}</span>
      </div>
    {:else}
      <div class="rd-row">
        <label class="rd-label" for="rd-video-bitrate">{t("exportDialog.videoBitrateLabel")}</label>
        <div class="rd-number-wrap">
          <NumberInput id="rd-video-bitrate" min={1} bind:value={renderStore.videoBitrateKbps} />
        </div>
        <span class="rd-hint muted-2">kbps</span>
      </div>
    {/if}

    <div class="rd-row">
      <label class="rd-label" for="rd-audio-codec">{t("exportDialog.audioCodecLabel")}</label>
      <div class="rd-select-wrap">
        <Select
          id="rd-audio-codec"
          value={renderStore.audioCodec}
          options={audioCodecSelectOptions}
          onchange={(v) => (renderStore.audioCodec = v as AudioCodec)}
        />
      </div>
    </div>

    <div class="rd-row">
      <label class="rd-label" for="rd-audio-bitrate">{t("exportDialog.audioBitrateLabel")}</label>
      <div class="rd-number-wrap">
        <NumberInput id="rd-audio-bitrate" min={1} bind:value={renderStore.audioBitrateKbps} />
      </div>
      <span class="rd-hint muted-2">kbps</span>
    </div>
  </Panel>

  <Panel title={t("exportDialog.hwSectionTitle")}>
    {#if renderStore.hardwareLoading}
      <LoadingState message={t("exportDialog.hwDetecting")} />
    {:else if renderStore.hardwareError}
      <ErrorState message={renderStore.hardwareError} />
    {:else if renderStore.hardware}
      <p class="rd-hw-active">{t("exportDialog.hwActiveEncoder", { label: renderStore.hardware.active_encoder_label })}</p>
      <div class="rd-row">
        <label class="rd-label" for="rd-hw-encoder">{t("exportDialog.hwForceLabel")}</label>
        <div class="rd-select-wrap">
          <Select
            id="rd-hw-encoder"
            value={renderStore.hardwareEncoder ?? "auto"}
            options={hwEncoderSelectOptions}
            onchange={(v) => {
              renderStore.hardwareEncoder = v === "auto" ? null : (v as EncoderBackend);
            }}
          />
        </div>
      </div>
      {#if renderStore.hardware.encoders.length > 0}
        <ul class="rd-hw-list muted-2">
          {#each renderStore.hardware.encoders as enc (enc.backend)}
            <li>{encoderLabel(enc.backend)}: {enc.working ? t("exportDialog.hwWorking") : t("exportDialog.hwNotAvailable")}</li>
          {/each}
        </ul>
      {/if}
    {/if}
  </Panel>

  <Panel title={t("exportDialog.outputSectionTitle")}>
    <div class="rd-row">
      <Button size="sm" onclick={() => void renderStore.chooseOutputPath()}>{t("exportDialog.chooseOutputButton")}</Button>
      <span class="rd-output-path muted-2" title={renderStore.outputPath ?? undefined}>
        {renderStore.outputPath ? basename(renderStore.outputPath) : t("exportDialog.noOutputChosen")}
      </span>
    </div>
  </Panel>

  {#if renderStore.startError}
    <ErrorState message={renderStore.startError} />
  {/if}

  {#if renderStore.progress}
    <Panel title={t("exportDialog.progressSectionTitle")}>
      {#if renderStore.progress.error}
        <ErrorState message={t("exportDialog.renderFailed", { error: renderStore.progress.error })} />
      {:else if renderStore.progress.done}
        <SuccessBanner message={t("exportDialog.renderComplete", { path: renderStore.progress.output_path ?? "" })} />
      {:else}
        <ProgressBar
          value={renderStore.progress.fraction ?? 0}
          max={1}
          label={formatPercent(renderStore.progress.fraction) +
            (renderStore.progress.speed !== null
              ? ` · ${t("exportDialog.speedLabel", { speed: renderStore.progress.speed.toFixed(2) })}`
              : "")}
        />
      {/if}
    </Panel>
  {/if}

  {#snippet footer()}
    {#if renderStore.isRendering}
      <Button variant="danger" disabled={renderStore.cancelling} onclick={() => void renderStore.cancel()}>
        {renderStore.cancelling ? t("exportDialog.cancelling") : t("exportDialog.cancelButton")}
      </Button>
    {:else if renderStore.progress?.done}
      <Button onclick={() => renderStore.startNewExport()}>{t("exportDialog.newExportButton")}</Button>
    {:else}
      <Button disabled={!renderStore.canExport} onclick={() => void renderStore.startExport()}>
        {renderStore.starting ? t("exportDialog.starting") : t("exportDialog.exportButton")}
      </Button>
    {/if}
    <span class="rd-footer-spacer"></span>
    <Button variant="ghost" onclick={() => renderStore.close()}>{t("exportDialog.closeButton")}</Button>
  {/snippet}
</Modal>

<style>
  /* Design System retrofit (Phase D7a/D9, `STUDIO_PLAN.md`): the dialog
     shell, section headings, finite-option selects, buttons, error/loading
     text, the progress track, the numeric fields, the CRF/bitrate radio
     pair, and the render-complete message are all gone from here —
     `Modal`/`Panel`/`Select`/`Button`/`ErrorState`/`LoadingState`/
     `ProgressBar`/`NumberInput`/`RadioGroup`/`SuccessBanner` (Design System)
     own that chrome now. Only what has no Design System equivalent remains:
     the preset-card grid (needs a "selected" visual state Card.svelte
     doesn't expose) and small layout-only helpers (row/label/select-wrap/
     number-wrap sizing, the hw list). */
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
  .rd-preset-card:hover {
    border-color: var(--border-strong);
  }
  .rd-preset-card.selected {
    border-color: var(--accent);
    background: hsl(213 94% 68% / 0.08);
  }
  .rd-preset-name {
    font-size: 11.5px;
    font-weight: 600;
  }
  .rd-preset-desc {
    font-size: 10.5px;
    line-height: 1.4;
  }
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
  .rd-select-wrap {
    flex: 1;
    min-width: 0;
  }
  /* Fixed-width wrappers around NumberInput (Phase D9) — NumberInput's own
     `.ui-input` fills its container (width: 100%), so these small,
     layout-only wrapper divs reproduce the original literal `.rd-number`/
     `.rd-number-sm` field widths (100px/76px) without needing a size prop on
     the shared component itself. */
  .rd-number-wrap {
    width: 100px;
    flex-shrink: 0;
  }
  .rd-number-wrap-sm {
    width: 76px;
    flex-shrink: 0;
  }
  .rd-x {
    flex-shrink: 0;
  }
  .rd-hint {
    font-size: 10.5px;
  }
  .rd-hw-active {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
  }
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
  .rd-footer-spacer {
    flex: 1;
  }
</style>
