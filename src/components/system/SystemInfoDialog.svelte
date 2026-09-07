<!--
  System Information dialog (Phase 12, master prompt §78): exactly the
  master-prompt-listed field set (Application version / Windows version /
  CPU / RAM / GPU / FFmpeg version / FFprobe version / Hardware encoders /
  CapCut detected version / CapCut path / Transcription backend / Installed
  models / Cache directory / Project directory), sourced from the real
  `get_system_information` command, plus a real "Copy System Information"
  button and a real "Open Logs Folder" button (master prompt §54/§86).

  Honest treatment of two fields with no real backing data yet (documented
  here + `IMPLEMENTATION_PLAN.md`):
    - "CapCut detected version": this app's CapCut/Jianying detector
      (`capcut::detect`) has never read a version — filesystem-marker-based
      detection only. Shown as "not tracked", not a fabricated value.
    - "Project directory": always empty — no Project Manager / default
      project directory concept exists anywhere in this codebase yet (see
      `SystemInformation::project_directory`'s own Rust doc comment). Shown
      as "not applicable yet", not a fabricated path.

  Placement decision (documented here + `IMPLEMENTATION_PLAN.md`): mirrors
  every other Phase 7/9/10/11/12 standalone-dialog precedent exactly (no
  master prompt §46 Settings surface exists yet to host this as a section) —
  a standalone dialog, mounted once in `App.svelte`, reachable from a
  "System Info…" button in `TopBar.svelte`.

  Pure UI over `stores/systemInfo.svelte.ts`.

  **Phase D7a Design System retrofit (`STUDIO_PLAN.md`):** the hand-rolled
  backdrop/dialog/header/close-button shell is now `Modal.svelte` (Phase D1),
  every error banner (`loadError`/`logsFolderError`/`copyError`) is
  `ErrorState.svelte`, the initial "loading…" one-liner is
  `LoadingState.svelte`, and every footer button is `Button.svelte`. Every
  real behavior — every store call, every `disabled` condition — is
  unchanged, only the markup underneath it. Left bespoke: the explainer
  paragraph and the field key/value list (`.si-list`/`.si-row`, a `<dl>` of
  ~13 label/value pairs) — matches `FirstRunWizard.svelte`'s own `.frw-kv`
  precedent, since no Design System primitive covers a plain label/value
  detail list, and the "copy done" success text (a plain inline confirmation,
  not a pill — same honest gap `ExportDialog.svelte`/`ModelManagerDialog.svelte`
  document for their own success messages).
-->
<script lang="ts">
  import { systemInfoStore } from "../../stores/systemInfo.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Modal from "../ui/Modal.svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import LoadingState from "../ui/LoadingState.svelte";

  const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
    const value = bytes / 1024 ** exp;
    return `${exp === 0 ? value.toFixed(0) : value.toFixed(1)} ${BYTE_UNITS[exp]}`;
  }
</script>

<Modal open={systemInfoStore.open} title={t("systemInfo.title")} width={620} onClose={() => systemInfoStore.close()}>
  <p class="si-explainer muted-2">{t("systemInfo.explainer")}</p>

  {#if systemInfoStore.loadError}
    <ErrorState message={t("systemInfo.loadFailed", { error: systemInfoStore.loadError })} />
  {/if}

  {#if systemInfoStore.loading && !systemInfoStore.data}
    <LoadingState message={t("systemInfo.loading")} />
  {/if}

  {#if systemInfoStore.data}
    {@const info = systemInfoStore.data}
    <dl class="si-list">
      <div class="si-row">
        <dt>{t("systemInfo.appVersionLabel")}</dt>
        <dd>{info.app_version} <span class="muted-2">(Tauri {info.tauri_version})</span></dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.windowsVersionLabel")}</dt>
        <dd>{info.os_version ?? info.os}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.cpuLabel")}</dt>
        <dd>{info.cpu_brand ?? t("systemInfo.unknownValue")} <span class="muted-2">({t("systemInfo.coreCount", { count: info.cpu_core_count })})</span></dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.ramLabel")}</dt>
        <dd>{formatBytes(info.used_memory_bytes)} / {formatBytes(info.total_memory_bytes)}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.gpuLabel")}</dt>
        <dd>{info.active_encoder_label}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.ffmpegVersionLabel")}</dt>
        <dd>{info.ffmpeg_version}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.ffprobeVersionLabel")}</dt>
        <dd>{info.ffprobe_version}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.hardwareEncodersLabel")}</dt>
        <dd>
          {#if info.hardware_encoders.length === 0}
            {t("systemInfo.noneDetected")}
          {:else}
            {info.hardware_encoders.map((e) => `${e.label}${e.working ? "" : ` ${t("systemInfo.notWorkingSuffix")}`}`).join(", ")}
          {/if}
        </dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.capcutVersionLabel")}</dt>
        <dd class="muted-2">{t("systemInfo.capcutVersionNotTracked")}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.capcutPathLabel")}</dt>
        <dd class="mono">
          {#if info.capcut_installations.length === 0}
            {t("systemInfo.noneDetected")}
          {:else}
            {#each info.capcut_installations as inst (inst.draft_root)}
              <div>{inst.draft_root}</div>
            {/each}
          {/if}
        </dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.transcriptionBackendLabel")}</dt>
        <dd>{info.transcription_backend}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.installedModelsLabel")}</dt>
        <dd>
          {#if info.installed_transcription_models.length === 0}
            {t("systemInfo.noneInstalled")}
          {:else}
            {info.installed_transcription_models.map((m) => `${m.id} (${formatBytes(m.size_bytes)})`).join(", ")}
          {/if}
        </dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.cacheDirectoryLabel")}</dt>
        <dd class="mono">{info.media_cache_dir}</dd>
      </div>
      <div class="si-row">
        <dt>{t("systemInfo.projectDirectoryLabel")}</dt>
        <dd class="muted-2">{info.project_directory ?? t("systemInfo.projectDirectoryNotApplicable")}</dd>
      </div>
    </dl>
  {/if}

  {#if systemInfoStore.logsFolderError}
    <ErrorState message={t("systemInfo.openLogsFailed", { error: systemInfoStore.logsFolderError })} />
  {/if}
  {#if systemInfoStore.copyError}
    <ErrorState message={t("systemInfo.copyFailed", { error: systemInfoStore.copyError })} />
  {/if}

  {#snippet footer()}
    <Button variant="ghost" disabled={systemInfoStore.loading} onclick={() => void systemInfoStore.refresh()}>
      {t("systemInfo.refreshButton")}
    </Button>
    <Button variant="ghost" disabled={systemInfoStore.openingLogsFolder} onclick={() => void systemInfoStore.openLogsFolder()}>
      {t("systemInfo.openLogsButton")}
    </Button>
    <span class="si-footer-spacer"></span>
    {#if systemInfoStore.copyDone}
      <span class="si-copy-done">{t("systemInfo.copyDone")}</span>
    {/if}
    <Button disabled={!systemInfoStore.data} onclick={() => void systemInfoStore.copyToClipboard()}>
      {t("systemInfo.copyButton")}
    </Button>
    <Button variant="ghost" onclick={() => systemInfoStore.close()}>{t("systemInfo.close")}</Button>
  {/snippet}
</Modal>

<style>
  /* Design System retrofit (Phase D7a, `STUDIO_PLAN.md`): the dialog shell,
     error banners, loading message, and every button are all gone from here —
     `Modal`/`ErrorState`/`LoadingState`/`Button` (Design System) own that
     chrome now. Only what has no Design System equivalent remains: the
     explainer paragraph, the field key/value list, and the inline
     "copy done" confirmation text. */
  .si-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .si-list {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .si-row {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 10px;
    padding: 7px 0;
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }
  .si-row:last-child {
    border-bottom: none;
  }
  .si-row dt {
    color: var(--muted);
    font-size: 11px;
  }
  .si-row dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .si-row dd.mono {
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
  }
  .si-footer-spacer {
    flex: 1;
  }
  .si-copy-done {
    font-size: 11px;
    color: var(--pos, #3fb950);
  }
</style>
