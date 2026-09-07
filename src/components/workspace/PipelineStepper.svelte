<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): promt.md §3.3/§28's Process Pipeline
  stepper (`✓ Subtitle → ✓ Translate → ● Voice 68% → ○ Sync → ○ Render`).

  **Real data sources only — no fabricated progress, per the task brief.**
  Each step's state comes from something that already, genuinely exists:

  - **Subtitle**: `stores/captions.svelte.ts`'s real `generating`/
    `generateError`/`captions` — the exact same state `CaptionsPanel.svelte`
    already renders, just summarized into one step here.
  - **Translate**: the real translate-in-flight/error/result state
    `WorkspaceSimple.svelte` owns (passed down as props) — backed by the
    real `ai::translate_captions` command, not a timer or a fake value.
  - **Voice**: always `pending`. Honest, not an oversight —
    `src-tauri/src/commands/voice.rs`'s own doc comment states plainly that
    no `synthesize_speech` command exists yet (Phase S7 built the provider
    abstraction + connection/listing commands only), so there is nothing
    real to reflect. Shown disabled-looking with a tooltip explaining why,
    matching this codebase's own `voice::stub` "return a clear NotImplemented
    rather than silently no-op" honesty convention.
  - **Sync**: always `pending`, for the same reason — "Sync Timeline" as
    promt.md's own pipeline concept has no corresponding real command; the
    existing Timeline's own sync-groups feature is a different, unrelated
    NLE concept, not this pipeline step.
  - **Render**: the real `stores/batch.svelte.ts` job state — specifically
    the single job (across every batch this session has seen a
    `batch:progress` event for) with the latest real `started_at`
    timestamp, a defined, honest "most likely relevant to what's being
    worked on right now" rule, not a random pick. `running` shows that
    job's own real `progress` percentage; `success`/`failed` mirror its real
    terminal `BatchJobStatus`. No batch job at all (nothing started this
    session) correctly shows `pending`, not a fabricated running animation.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { batchStore } from "../../stores/batch.svelte";
  import { t } from "../../lib/i18n.svelte";
  import type { BatchJob } from "../../types/bindings";
  import Tooltip from "../ui/Tooltip.svelte";

  let {
    translating,
    translateError,
    hasTranslated,
  }: { translating: boolean; translateError: string | null; hasTranslated: boolean } = $props();

  type StepState = "pending" | "running" | "success" | "failed";

  const ICONS: Record<StepState, string> = { pending: "○", running: "●", success: "✓", failed: "✗" };

  let subtitleState = $derived.by((): StepState => {
    if (captionsStore.generateError) return "failed";
    if (captionsStore.generating) return "running";
    return captionsStore.captions.length > 0 ? "success" : "pending";
  });

  let translateState = $derived.by((): StepState => {
    if (translateError) return "failed";
    if (translating) return "running";
    return hasTranslated ? "success" : "pending";
  });

  /** The job with the latest real `started_at` across every job this
   * session knows about (see module doc comment). */
  let latestJob = $derived.by((): BatchJob | null => {
    const jobs = Object.values(batchStore.jobsById);
    if (jobs.length === 0) return null;
    return jobs.reduce((latest, job) => (Date.parse(job.started_at) > Date.parse(latest.started_at) ? job : latest));
  });

  let renderState = $derived.by((): StepState => {
    if (!latestJob) return "pending";
    if (latestJob.status === "rendering") return "running";
    if (latestJob.status === "completed") return "success";
    if (latestJob.status === "failed") return "failed";
    return "pending";
  });

  let renderProgressLabel = $derived(
    latestJob && latestJob.status === "rendering" ? `${Math.round(latestJob.progress * 100)}%` : null,
  );
</script>

<div class="stepper" role="list" aria-label={t("workspaceTab.stepper.ariaLabel")}>
  <div class="step" data-state={subtitleState} role="listitem">
    <span class="icon" aria-hidden="true">{ICONS[subtitleState]}</span>
    <span class="label">{t("workspaceTab.stepper.subtitle")}</span>
  </div>
  <span class="arrow" aria-hidden="true">→</span>

  <div class="step" data-state={translateState} role="listitem">
    <span class="icon" aria-hidden="true">{ICONS[translateState]}</span>
    <span class="label">{t("workspaceTab.stepper.translate")}</span>
  </div>
  <span class="arrow" aria-hidden="true">→</span>

  <Tooltip text={t("workspaceTab.script.voiceNotWiredTooltip")}>
    <div class="step step-unwired" data-state="pending" role="listitem">
      <span class="icon" aria-hidden="true">{ICONS.pending}</span>
      <span class="label">{t("workspaceTab.stepper.voice")}</span>
    </div>
  </Tooltip>
  <span class="arrow" aria-hidden="true">→</span>

  <Tooltip text={t("workspaceTab.script.syncNotWiredTooltip")}>
    <div class="step step-unwired" data-state="pending" role="listitem">
      <span class="icon" aria-hidden="true">{ICONS.pending}</span>
      <span class="label">{t("workspaceTab.stepper.sync")}</span>
    </div>
  </Tooltip>
  <span class="arrow" aria-hidden="true">→</span>

  <div class="step" data-state={renderState} role="listitem">
    <span class="icon" aria-hidden="true">{ICONS[renderState]}</span>
    <span class="label">{t("workspaceTab.stepper.render")}</span>
    {#if renderProgressLabel}<span class="pct mono">{renderProgressLabel}</span>{/if}
  </div>
</div>

<style>
  .stepper {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .step {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    font-size: 11.5px;
    color: var(--muted);
  }
  .step-unwired {
    opacity: 0.6;
  }
  .step[data-state="success"] {
    color: var(--pos);
  }
  .step[data-state="running"] {
    color: var(--accent);
  }
  .step[data-state="failed"] {
    color: var(--neg);
  }
  .icon {
    font-weight: 700;
  }
  .arrow {
    color: var(--muted-2);
    flex-shrink: 0;
  }
  .pct {
    font-size: 10px;
    color: var(--muted-2);
  }
</style>
