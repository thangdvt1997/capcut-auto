<!--
  Bottom Status Bar (Phase D13, `STUDIO_PLAN.md` "Dashboard Header + Status
  Bar", promt.md §14 "STATUS BAR"): a real, always-visible bottom strip —
  "CPU 42% | RAM 5.2/16GB | Workers 3/3 | Queue 18" plus a "Current:
  <stage> – <name> – <percent>%" line — matching promt.md's own worked
  example in spirit, never a fabricated number.

  **Real data sources only, per this codebase's own discipline:**
  - **CPU%/RAM%**: the new `get_live_system_stats` Tauri command
    (`src-tauri/src/commands/diagnostics.rs`), via
    `stores/liveSystemStats.svelte.ts` — see that command's own module doc
    comment for the full "why a persistent backend `System` + frontend
    polling" architecture decision. Polled here on a 2.5s interval (task
    brief: "keep the polling interval reasonable, e.g. every 2-3 seconds"),
    started on mount and cleared on unmount — this bar is a permanent shell
    row (mounted unconditionally in `App.svelte`, like `TopBar`/
    `ActivityLogPanel`), so "poll while mounted" already matches the "don't
    waste resources when idle" discipline `WorkerPoolWidget.svelte`
    establishes for its own narrower "only while a job is active" case: a
    permanently-visible host-metrics bar has a permanent reason to poll,
    same as promt.md's own "always visible bottom status bar" concept.
  - **FFmpeg**: the real, already-existing `ffmpeg_diagnostics` command
    (`commands::media`) — `Result::ok` means a real, resolved ffmpeg/ffprobe
    binary; `Result::error` means genuinely not found on this machine.
    Fetched once on mount (ffmpeg's presence on disk essentially never
    changes mid-session, unlike CPU/RAM), not on the CPU/RAM interval.
  - **CapCut**: `stores/capcut.svelte.ts`'s real `installations` (from the
    real `detect_windows_installations` scan) — `ensureDetected()` is called
    here too (idempotent: it's a no-op if a scan already ran, e.g. from
    opening CapCut Settings/Export), so this bar shows a real detected state
    even if the user never opened either of those dialogs this session,
    without duplicating the detection logic itself.
  - **AI API / Voice API**: the exact same `aiSettingsStore.testResult` /
    `voiceSettingsStore.testResult` real connection-test state
    `DashboardHeader.svelte` already reads — see that component's own doc
    comment for the "never fabricate Connected" rule, applied identically
    here.
  - **Current: <job>**: `stores/batch.svelte.ts`'s real `latestJob` derived
    (Phase D13 — moved out of `PipelineStepper.svelte`'s own local
    computation so this exact "job with the latest real started_at" rule is
    reused, not re-derived — see that store's own doc comment). Shows
    `stage – name – percent%`, matching promt.md's own worked example
    ("Generating voice – video_004.mp4 – 72%") field order exactly. No job
    at all (nothing started this session) shows an honest "no active job"
    state, never a fabricated line.

  Placement: mounted once in `App.svelte`, as the very last shell row
  (below `ActivityLogPanel`) — a permanent, app-level bottom bar, the same
  "cross-cutting, not tab-specific" reasoning `ActivityLogPanel.svelte`'s own
  doc comment already established.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { commands } from "../../types/bindings";
  import type { FfmpegDiagnostics } from "../../types/bindings";
  import { liveSystemStatsStore } from "../../stores/liveSystemStats.svelte";
  import { batchStore } from "../../stores/batch.svelte";
  import { capcutStore } from "../../stores/capcut.svelte";
  import { aiSettingsStore } from "../../stores/aiSettings.svelte";
  import { voiceSettingsStore } from "../../stores/voiceSettings.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Badge from "../ui/Badge.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";

  const POLL_INTERVAL_MS = 2500;

  type ConnStatus = "connected" | "failed" | "notTested";

  function connStatusOf(testResult: { success: boolean } | null): ConnStatus {
    if (testResult === null) return "notTested";
    return testResult.success ? "connected" : "failed";
  }

  function badgeVariantOf(status: ConnStatus): "pos" | "neg" | "neutral" {
    if (status === "connected") return "pos";
    if (status === "failed") return "neg";
    return "neutral";
  }

  function connLabelOf(status: ConnStatus): string {
    if (status === "connected") return t("dashboardHeader.connected");
    if (status === "failed") return t("dashboardHeader.notConnected");
    return t("dashboardHeader.notTested");
  }

  // One immediate fetch, then a 2.5s poll for as long as this permanent
  // shell row is mounted — see doc comment for why "always poll while
  // mounted" is the right call here, unlike WorkerPoolWidget's own
  // narrower "only while a job is active" gate.
  $effect(() => {
    void liveSystemStatsStore.refresh();
    const id = setInterval(() => void liveSystemStatsStore.refresh(), POLL_INTERVAL_MS);
    return () => clearInterval(id);
  });

  let ffmpeg = $state<FfmpegDiagnostics | null>(null);
  let ffmpegChecked = $state(false);
  let ffmpegError = $state<string | null>(null);

  onMount(() => {
    void capcutStore.ensureDetected();
    commands
      .ffmpegDiagnostics()
      .then((result) => {
        ffmpegChecked = true;
        if (result.status === "ok") {
          ffmpeg = result.data;
        } else {
          ffmpegError = result.error.message;
        }
      })
      .catch((err: unknown) => {
        ffmpegChecked = true;
        ffmpegError = String(err);
      });
  });

  let stats = $derived(liveSystemStatsStore.stats);
  let cpuLabel = $derived(stats ? t("statusBar.cpu", { percent: Math.round(stats.cpu_usage_percent) }) : null);
  let ramLabel = $derived(
    stats
      ? t("statusBar.ram", {
          usedGb: (stats.used_memory_bytes / 1024 ** 3).toFixed(1),
          totalGb: (stats.total_memory_bytes / 1024 ** 3).toFixed(1),
        })
      : null,
  );

  let capcutAvailable = $derived(capcutStore.installations.length > 0);

  let aiStatus = $derived(connStatusOf(aiSettingsStore.testResult));
  let voiceStatus = $derived(connStatusOf(voiceSettingsStore.testResult));

  let latestJob = $derived(batchStore.latestJob);
  let currentJobLabel = $derived(
    latestJob ? `${latestJob.stage} – ${latestJob.name}` : null,
  );
  let currentJobPercent = $derived(latestJob ? Math.round(latestJob.progress * 100) : 0);
</script>

<div class="sb-shell" role="status" aria-label={t("statusBar.ariaLabel")}>
  <span class="sb-metrics">
    {#if liveSystemStatsStore.error}
      <span class="sb-item muted-2" title={liveSystemStatsStore.error}>{t("statusBar.statsUnavailable")}</span>
    {:else if cpuLabel && ramLabel}
      <span class="sb-item mono">{cpuLabel}</span>
      <span class="sb-item mono">{ramLabel}</span>
    {:else}
      <span class="sb-item muted-2">{t("statusBar.loading")}</span>
    {/if}

    <span class="sb-item">
      <span class="sb-label muted-2">{t("statusBar.ffmpeg")}</span>
      {#if !ffmpegChecked}
        <Badge variant="neutral">{t("statusBar.checking")}</Badge>
      {:else if ffmpeg}
        <span title={ffmpeg.ffmpeg_version}><Badge variant="pos">{t("statusBar.available")}</Badge></span>
      {:else}
        <span title={ffmpegError ?? undefined}><Badge variant="neg">{t("statusBar.unavailable")}</Badge></span>
      {/if}
    </span>

    <span class="sb-item">
      <span class="sb-label muted-2">{t("statusBar.capcut")}</span>
      {#if capcutStore.detectLoading && !capcutStore.detectedOnce}
        <Badge variant="neutral">{t("statusBar.checking")}</Badge>
      {:else if capcutAvailable}
        <Badge variant="pos">{t("statusBar.available")}</Badge>
      {:else}
        <Badge variant="neg">{t("statusBar.unavailable")}</Badge>
      {/if}
    </span>

    <span class="sb-item">
      <span class="sb-label muted-2">{t("statusBar.ai")}</span>
      <Badge variant={badgeVariantOf(aiStatus)}>{connLabelOf(aiStatus)}</Badge>
    </span>

    <span class="sb-item">
      <span class="sb-label muted-2">{t("statusBar.voice")}</span>
      <Badge variant={badgeVariantOf(voiceStatus)}>{connLabelOf(voiceStatus)}</Badge>
    </span>
  </span>

  <span class="sb-current">
    {#if latestJob && currentJobLabel}
      <span class="sb-label muted-2">{t("statusBar.currentLabel")}</span>
      <span class="sb-item">{currentJobLabel}</span>
      <span class="sb-current-bar">
        <ProgressBar value={latestJob.progress} label={`${currentJobPercent}%`} />
      </span>
    {:else}
      <span class="sb-item muted-2">{t("statusBar.currentNone")}</span>
    {/if}
  </span>
</div>

<style>
  .sb-shell {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-3);
    background: var(--surface-1);
    border-top: 1px solid var(--border);
    font-size: 11px;
  }
  .sb-metrics {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3);
    min-width: 0;
  }
  .sb-item {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    white-space: nowrap;
  }
  .sb-label {
    font-size: 10.5px;
  }
  .sb-current {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    flex: 1;
    justify-content: flex-end;
  }
  .sb-current-bar {
    width: 120px;
    flex-shrink: 0;
  }
</style>
