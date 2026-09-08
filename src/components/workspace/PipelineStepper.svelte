<!--
  Phase D2+D3 (`STUDIO_PLAN.md`): promt.md §3.3/§28's Process Pipeline
  stepper. Phase D19 expanded this from that pass's own deliberately-smaller
  5-step subset to promt.md §3.3's full 10-step vision (Extract Subtitle,
  Speech Recognition, Translate, Rewrite Script, Generate Voice, Sync
  Timeline, Video Processing, Subtitle Burn-in, Render, Export), plus a real
  per-step enable/disable toggle and the SKIPPED state promt.md's own
  vocabulary names.

  **Real data sources only — no fabricated progress, per this component's own
  established discipline (unchanged this pass).** Each step's state comes
  from something that already, genuinely exists:

  - **Extract Subtitle** (was "Subtitle"): `stores/captions.svelte.ts`'s real
    `generating`/`generateError`/`captions` — unchanged logic, just relabeled
    to promt.md's own more precise name for what `generate_captions` actually
    does (groups an already-recognized transcript into displayed subtitle/
    caption entries with word-level timing — genuinely "extracting the
    subtitle track" from recognized speech, not the speech recognition itself
    — see the next step).
  - **Speech Recognition** (new): the real, *separate* ASR backend this
    codebase already has — `commands::transcription::transcribe_media`,
    fronted by `stores/transcriptEditor.svelte.ts` (`TranscriptEditor.svelte`,
    Advanced mode's Transcript tab) — confirmed distinct from caption
    generation above: `transcribe_media` produces `project.transcript`
    entries from raw audio; `generate_captions` only ever *consumes* that
    already-existing transcript (see `stores/captions.svelte.ts`'s own class
    doc comment) and cannot run before this step has produced something for
    it to read. `running`/`failed` come from `transcriptEditor`'s own
    per-job state (`transcribing`, `startError`, `progress?.error`) — real,
    but scoped to whichever clip's media a transcription job was last started
    for (`transcriptEditor`'s own "anchored to the selected clip" scope,
    documented there), not a whole-project aggregate, the same honesty tier
    already established for the Voice step below. `success` instead reads
    `captionsStore.hasTranscript` (whole-project `project.transcript.length
    > 0`) so it reflects "this project has *some* recognized speech on file"
    rather than only the most recently anchored clip.
  - **Translate**: unchanged from Phase D2+D3 — see below.
  - **Rewrite Script** (new): always `pending` — no backend of any kind
    exists anywhere in this codebase for AI script rewriting (confirmed via
    grep before this pass), so this honestly never leaves Waiting, exactly
    like Voice/Sync below. Shown with the same disabled-looking tooltip
    treatment.
  - **Generate Voice** (was "Voice"): unchanged reasoning from Phase D16's
    re-confirmation — see below.
  - **Sync Timeline** (was "Sync"): unchanged — see below.
  - **Video Processing** (new): the real `stores/batch.svelte.ts` batch
    pipeline's own `"editing"` `BatchJobStatus` (silence/filler-word removal
    — `src-tauri/src/batch/pipeline.rs`'s real `"Removing silence"`/`"Editing
    complete"` stage labels), read off the same `batchStore.latestJob` the
    existing Render step already uses. Deliberately **never reports
    `failed`**: `batch::manager::process_job`/`run_job_with_events` (Rust)
    unconditionally overwrite a failed job's `stage` field to the generic
    literal `"Failed"` on any terminal error, regardless of which named stage
    (`BatchError::StageFailed{stage,..}`) actually raised it — confirmed by
    reading that function directly. There is no reliable, typed signal left
    at that point for which of Video Processing/Render was actually in
    flight when a job failed; guessing would be exactly the kind of
    fabrication this component's own discipline forbids. So only Render
    (unchanged, pre-existing logic) reports the pipeline's own terminal
    `failed` outcome — matching this codebase's one existing precedent for
    that signal — while Video Processing stays at `pending`/`running`/
    `success` only, all unambiguous transitions of the typed `status` enum.
    No percent label for the same reason: `BatchJob.progress` is a
    cross-stage *weighted overall* fraction (`batch/pipeline.rs`'s
    `ProgressTracker::overall`), not stage-local — showing it next to "Video
    Processing" specifically would misleadingly imply stage-local precision
    it doesn't have.
  - **Subtitle Burn-in** (new): mirrors the Render step's own state exactly
    (not an independent derivation) — `src-tauri/src/render/captions.rs`
    confirms caption burn-in is real and genuinely happens, but as an
    inseparable part of the single `"rendering"` stage/status this codebase
    reports, with no separately observable phase of its own to read distinct
    state from. Shown with an explanatory tooltip rather than silently
    duplicating Render with no comment, so it's clear this isn't a copy-paste
    mistake.
  - **Render**: unchanged from Phase D2+D3/D13 — the real
    `stores/batch.svelte.ts` job state, specifically `batchStore.latestJob`
    (the single job across every batch this session has seen a
    `batch:progress` event for with the latest real `started_at` timestamp —
    a defined, honest "most likely relevant to what's being worked on right
    now" rule, not a random pick). `running` shows that job's own real
    `progress` percentage; `success`/`failed` mirror its real terminal
    `BatchJobStatus`. No batch job at all (nothing started this session)
    correctly shows `pending`, not a fabricated running animation.
  - **Export** (new): the real, *separate* `stores/render.svelte.ts` single-
    project Export dialog job lifecycle (master prompt §32/§43/§44's
    `start_render_job`/`render:progress`) — a genuinely different real
    feature from the Batch pipeline's own Render stage above (one exports
    *this* project directly from the Export dialog; the other runs an
    independent batch pipeline over arbitrary media files). `renderStore.
    jobId === null` (no export ever started this session) is `pending`;
    `isRendering` is `running` (with a real `progress?.fraction` percent,
    unlike Video Processing above — `RenderProgressEvent.fraction` genuinely
    is this one job's own real fraction, not a cross-stage composite);
    `progress?.done && !progress?.error` is `success`; `startError` or
    `progress?.error` is `failed` — all real, typed, unambiguous fields.

  **Enable/disable toggle + SKIPPED** (new, per promt.md §3.3's own "Cho
  phép enable/disable từng step" + WAITING/RUNNING/SUCCESS/FAILED/SKIPPED
  vocabulary): `enabledSteps` below is a plain, real, working `$state`
  toggle per step — checking/unchecking it for real changes what this
  component renders (`skipped` overrides whatever the step's own derived
  state would otherwise be, exactly like a disabled step in promt.md's own
  example). It is deliberately **session-only** (not persisted to
  `localStorage`), documented rather than silently chosen: no other part of
  this codebase reads these flags yet (there is no real "run the full
  10-step pipeline end-to-end" orchestrator command to gate — confirmed via
  grep — matching this task's own explicit allowance that the toggle "doesn't
  need to actually gate any real pipeline execution yet if nothing consumes
  it"), so persisting a flag nothing durable depends on yet would be a
  false promise of continuity rather than a real feature; this can graduate
  to persisted, orchestrator-consumed state once such a command exists.
-->
<script lang="ts">
  import { captionsStore } from "../../stores/captions.svelte";
  import { batchStore } from "../../stores/batch.svelte";
  import { translationReviewStore } from "../../stores/translationReview.svelte";
  import { transcriptEditor } from "../../stores/transcriptEditor.svelte";
  import { renderStore } from "../../stores/render.svelte";
  import { t } from "../../lib/i18n.svelte";
  import Tooltip from "../ui/Tooltip.svelte";

  type StepState = "pending" | "running" | "success" | "failed";
  type DisplayState = StepState | "skipped";

  type StepId =
    | "extractSubtitle"
    | "speechRecognition"
    | "translate"
    | "rewriteScript"
    | "generateVoice"
    | "syncTimeline"
    | "videoProcessing"
    | "subtitleBurnIn"
    | "render"
    | "export";

  interface StepDescriptor {
    id: StepId;
    labelKey: string;
    state: DisplayState;
    percent: string | null;
    tooltipKey: string | null;
  }

  const ICONS: Record<DisplayState, string> = {
    pending: "○",
    running: "●",
    success: "✓",
    failed: "✗",
    skipped: "⊘",
  };

  /** Session-local only — see file doc comment's "Enable/disable toggle"
   * section for why this isn't persisted. Every step defaults enabled. */
  let enabledSteps = $state<Record<StepId, boolean>>({
    extractSubtitle: true,
    speechRecognition: true,
    translate: true,
    rewriteScript: true,
    generateVoice: true,
    syncTimeline: true,
    videoProcessing: true,
    subtitleBurnIn: true,
    render: true,
    export: true,
  });

  function toggleStep(id: StepId): void {
    enabledSteps[id] = !enabledSteps[id];
  }

  // -------------------------------------------------------------------
  // Real per-step state — see file doc comment for each one's own source.
  // -------------------------------------------------------------------

  let extractSubtitleState = $derived.by((): StepState => {
    if (captionsStore.generateError) return "failed";
    if (captionsStore.generating) return "running";
    return captionsStore.captions.length > 0 ? "success" : "pending";
  });

  let speechRecognitionState = $derived.by((): StepState => {
    if (transcriptEditor.startError || transcriptEditor.progress?.error) return "failed";
    if (transcriptEditor.transcribing) return "running";
    return captionsStore.hasTranscript ? "success" : "pending";
  });

  let translateState = $derived.by((): StepState => {
    if (translationReviewStore.translateError || translationReviewStore.applyError) return "failed";
    if (translationReviewStore.translating || translationReviewStore.applying) return "running";
    return translationReviewStore.appliedThisSession ? "success" : "pending";
  });

  /** The job with the latest real `started_at` across every job this
   * session knows about — see file doc comment and `batchStore.latestJob`'s
   * own doc comment. Shared by Video Processing and Render below: the same
   * one real job progressing through its own real stages. */
  let latestJob = $derived(batchStore.latestJob);

  let videoProcessingState = $derived.by((): StepState => {
    if (!latestJob) return "pending";
    if (latestJob.status === "editing") return "running";
    if (latestJob.status === "rendering" || latestJob.status === "completed") return "success";
    return "pending"; // see file doc comment: `failed` is never attributed here
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

  /** See file doc comment — deliberately the same derived value as
   * `renderState`, not an independent one. */
  let subtitleBurnInState = $derived(renderState);

  let exportState = $derived.by((): StepState => {
    if (renderStore.startError || renderStore.progress?.error) return "failed";
    if (renderStore.isRendering) return "running";
    return renderStore.progress?.done ? "success" : "pending";
  });

  let exportProgressLabel = $derived(
    renderStore.isRendering && renderStore.progress?.fraction != null
      ? `${Math.round(renderStore.progress.fraction * 100)}%`
      : null,
  );

  function display(id: StepId, base: StepState): DisplayState {
    return enabledSteps[id] ? base : "skipped";
  }

  let steps = $derived.by(
    (): StepDescriptor[] => [
      {
        id: "extractSubtitle",
        // Reuses the pre-existing `stepper.subtitle` key (its *value* is
        // updated to "Extract Subtitle" this pass — see file doc comment)
        // rather than adding a new key + orphaning the old one.
        labelKey: "workspaceTab.stepper.subtitle",
        state: display("extractSubtitle", extractSubtitleState),
        percent: null,
        tooltipKey: null,
      },
      {
        id: "speechRecognition",
        labelKey: "workspaceTab.stepper.speechRecognition",
        state: display("speechRecognition", speechRecognitionState),
        percent: null,
        tooltipKey: null,
      },
      {
        id: "translate",
        labelKey: "workspaceTab.stepper.translate",
        state: display("translate", translateState),
        percent: null,
        tooltipKey: null,
      },
      {
        id: "rewriteScript",
        labelKey: "workspaceTab.stepper.rewriteScript",
        state: display("rewriteScript", "pending"),
        percent: null,
        tooltipKey: "workspaceTab.script.rewriteScriptNotWiredTooltip",
      },
      {
        id: "generateVoice",
        labelKey: "workspaceTab.stepper.voice",
        state: display("generateVoice", "pending"),
        percent: null,
        tooltipKey: "workspaceTab.script.voiceNotWiredTooltip",
      },
      {
        id: "syncTimeline",
        labelKey: "workspaceTab.stepper.sync",
        state: display("syncTimeline", "pending"),
        percent: null,
        tooltipKey: "workspaceTab.script.syncNotWiredTooltip",
      },
      {
        id: "videoProcessing",
        labelKey: "workspaceTab.stepper.videoProcessing",
        state: display("videoProcessing", videoProcessingState),
        percent: null,
        tooltipKey: null,
      },
      {
        id: "subtitleBurnIn",
        labelKey: "workspaceTab.stepper.subtitleBurnIn",
        state: display("subtitleBurnIn", subtitleBurnInState),
        percent: null,
        tooltipKey: "workspaceTab.stepper.subtitleBurnInTooltip",
      },
      {
        id: "render",
        labelKey: "workspaceTab.stepper.render",
        state: display("render", renderState),
        percent: renderProgressLabel,
        tooltipKey: null,
      },
      {
        id: "export",
        labelKey: "workspaceTab.stepper.export",
        state: display("export", exportState),
        percent: exportProgressLabel,
        tooltipKey: null,
      },
    ],
  );
</script>

{#snippet stepBody(step: StepDescriptor)}
  <div class="step" class:step-skipped={step.state === "skipped"} data-state={step.state} role="listitem">
    <input
      type="checkbox"
      class="enable-checkbox"
      checked={enabledSteps[step.id]}
      onchange={() => toggleStep(step.id)}
      title={t("workspaceTab.stepper.enableToggleTitle")}
      aria-label={`${t("workspaceTab.stepper.enableToggleTitle")}: ${t(step.labelKey)}`}
    />
    <span class="icon" aria-hidden="true">{ICONS[step.state]}</span>
    <span class="label">{t(step.labelKey)}</span>
    {#if step.percent}<span class="pct mono">{step.percent}</span>{/if}
  </div>
{/snippet}

<div class="stepper" role="list" aria-label={t("workspaceTab.stepper.ariaLabel")}>
  {#each steps as step, i (step.id)}
    {#if step.tooltipKey}
      <Tooltip text={t(step.tooltipKey)}>{@render stepBody(step)}</Tooltip>
    {:else}
      {@render stepBody(step)}
    {/if}
    {#if i < steps.length - 1}
      <span class="arrow" aria-hidden="true">→</span>
    {/if}
  {/each}
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
  .step-skipped {
    opacity: 0.5;
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
  .enable-checkbox {
    width: 11px;
    height: 11px;
    margin: 0;
    cursor: pointer;
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
