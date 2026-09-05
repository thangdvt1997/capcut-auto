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
-->
<script lang="ts">
  import { systemInfoStore } from "../../stores/systemInfo.svelte";
  import { t } from "../../lib/i18n.svelte";

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      systemInfoStore.close();
    }
  }

  const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
    const value = bytes / 1024 ** exp;
    return `${exp === 0 ? value.toFixed(0) : value.toFixed(1)} ${BYTE_UNITS[exp]}`;
  }
</script>

{#if systemInfoStore.open}
  <div class="si-backdrop" role="presentation" onclick={() => systemInfoStore.close()}>
    <div
      class="si-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={t("systemInfo.title")}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onKeydown}
    >
      <div class="si-header">
        <span class="si-title">{t("systemInfo.title")}</span>
        <button class="btn btn-ghost" onclick={() => systemInfoStore.close()} title={t("systemInfo.close")}>×</button>
      </div>

      <div class="si-body">
        <p class="si-explainer muted-2">{t("systemInfo.explainer")}</p>

        {#if systemInfoStore.loadError}
          <div class="si-error">{t("systemInfo.loadFailed", { error: systemInfoStore.loadError })}</div>
        {/if}

        {#if systemInfoStore.loading && !systemInfoStore.data}
          <p class="si-empty muted-2">{t("systemInfo.loading")}</p>
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
          <div class="si-error">{t("systemInfo.openLogsFailed", { error: systemInfoStore.logsFolderError })}</div>
        {/if}
        {#if systemInfoStore.copyError}
          <div class="si-error">{t("systemInfo.copyFailed", { error: systemInfoStore.copyError })}</div>
        {/if}
      </div>

      <div class="si-footer">
        <button class="btn btn-ghost" disabled={systemInfoStore.loading} onclick={() => void systemInfoStore.refresh()}>
          {t("systemInfo.refreshButton")}
        </button>
        <button
          class="btn btn-ghost"
          disabled={systemInfoStore.openingLogsFolder}
          onclick={() => void systemInfoStore.openLogsFolder()}
        >
          {t("systemInfo.openLogsButton")}
        </button>
        <span class="si-footer-spacer"></span>
        {#if systemInfoStore.copyDone}
          <span class="si-copy-done">{t("systemInfo.copyDone")}</span>
        {/if}
        <button class="btn" disabled={!systemInfoStore.data} onclick={() => void systemInfoStore.copyToClipboard()}>
          {t("systemInfo.copyButton")}
        </button>
        <button class="btn btn-ghost" onclick={() => systemInfoStore.close()}>{t("systemInfo.close")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .si-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: grid;
    place-items: center;
    z-index: 100;
  }
  .si-dialog {
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
  .si-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .si-title {
    font-size: 13px;
    font-weight: 600;
  }
  .si-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .si-explainer {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .si-empty {
    margin: 0;
    font-size: 11.5px;
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
  .si-error {
    padding: 8px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .si-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .si-footer-spacer {
    flex: 1;
  }
  .si-copy-done {
    font-size: 11px;
    color: var(--pos, #3fb950);
  }
</style>
