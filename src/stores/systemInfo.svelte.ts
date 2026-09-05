// Svelte 5 runes-based store for the Phase 12 System Information panel
// (master prompt §78): loads the real `get_system_information` aggregate,
// offers a real "Copy System Information" action (formats exactly the
// fields the dialog displays as plain text, via the standard Web Clipboard
// API — already the simplest option available inside a Tauri webview, no
// new `@tauri-apps/api` clipboard dependency needed), and a real
// "Open Logs Folder" button against `commands.openLogsFolder`.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { commands } from "../types/bindings";
import type { SystemInformation } from "../types/bindings";

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
  const value = bytes / 1024 ** exp;
  return `${exp === 0 ? value.toFixed(0) : value.toFixed(1)} ${BYTE_UNITS[exp]}`;
}

/**
 * Formats exactly the master prompt §78 field list as plain text, in the
 * same order the dialog displays them — "Copy System Information" copies
 * what's on screen, nothing more, nothing hidden. `project_directory` is
 * always `null` (see `SystemInformation` doc comment in `bindings.ts` / the
 * real Rust struct) — rendered honestly as "not applicable yet" rather than
 * a fabricated path, since no Project Manager / default project directory
 * concept exists anywhere in this codebase yet.
 */
export function formatSystemInformation(info: SystemInformation): string {
  const cpu = info.cpu_brand ? `${info.cpu_brand} (${info.cpu_core_count} cores)` : `${info.cpu_core_count} cores`;
  const ram = `${formatBytes(info.used_memory_bytes)} used / ${formatBytes(info.total_memory_bytes)} total`;
  const encoderNames = info.hardware_encoders.map((e) => `${e.label}${e.working ? "" : " (not working)"}`);
  const gpu = info.active_encoder_label;
  const hardwareEncoders = encoderNames.length > 0 ? encoderNames.join(", ") : "none detected";
  const capcutDetected = info.capcut_installations.length > 0;
  const capcutVersion = "not tracked (no version-reading detector exists for CapCut/Jianying)";
  const capcutPath = capcutDetected
    ? info.capcut_installations.map((i) => i.draft_root).join("; ")
    : "not detected";
  const installedModels =
    info.installed_transcription_models.length > 0
      ? info.installed_transcription_models.map((m) => `${m.id} (${formatBytes(m.size_bytes)})`).join(", ")
      : "none installed";
  const projectDirectory = info.project_directory ?? "not applicable yet (no Project Manager exists in this app)";

  const lines = [
    "AI Video Editor — System Information",
    `Application version: ${info.app_version} (Tauri ${info.tauri_version})`,
    `Windows version: ${info.os_version ?? info.os}`,
    `CPU: ${cpu}`,
    `RAM: ${ram}`,
    `GPU: ${gpu}`,
    `FFmpeg version: ${info.ffmpeg_version}`,
    `FFprobe version: ${info.ffprobe_version}`,
    `Hardware encoders: ${hardwareEncoders}`,
    `CapCut detected version: ${capcutVersion}`,
    `CapCut path: ${capcutPath}`,
    `Transcription backend: ${info.transcription_backend}`,
    `Installed models: ${installedModels}`,
    `Cache directory: ${info.media_cache_dir}`,
    `Project directory: ${projectDirectory}`,
  ];
  return lines.join("\n");
}

class SystemInfoStore {
  open = $state(false);

  data = $state<SystemInformation | null>(null);
  loading = $state(false);
  loadError = $state<string | null>(null);

  copyDone = $state(false);
  copyError = $state<string | null>(null);

  logsFolderError = $state<string | null>(null);
  openingLogsFolder = $state(false);

  openDialog(): void {
    this.open = true;
    void this.refresh();
  }

  close(): void {
    this.open = false;
  }

  async refresh(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.loadError = null;
    this.copyDone = false;
    this.copyError = null;
    try {
      const result = await commands.getSystemInformation();
      if (result.status === "ok") {
        this.data = result.data;
      } else {
        this.loadError = result.error.message;
      }
    } catch (err) {
      this.loadError = String(err);
    } finally {
      this.loading = false;
    }
  }

  async copyToClipboard(): Promise<void> {
    if (!this.data) return;
    this.copyError = null;
    this.copyDone = false;
    try {
      await navigator.clipboard.writeText(formatSystemInformation(this.data));
      this.copyDone = true;
    } catch (err) {
      this.copyError = String(err);
    }
  }

  async openLogsFolder(): Promise<void> {
    if (this.openingLogsFolder) return;
    this.openingLogsFolder = true;
    this.logsFolderError = null;
    try {
      const result = await commands.openLogsFolder();
      if (result.status === "error") {
        this.logsFolderError = result.error.message;
      }
    } catch (err) {
      this.logsFolderError = String(err);
    } finally {
      this.openingLogsFolder = false;
    }
  }
}

export const systemInfoStore = new SystemInfoStore();
