<!--
  Preview / Dry Run result panel (upgrade spec §18, `UPGRADE_PLAN.md` Phase
  U3). Purely presentational — renders a real `DryRunResult` (`commands.
  dryRunBatchJob`, `src-tauri/src/batch/dry_run.rs`) exactly as returned, no
  fabricated fields. Mounted inside `StartBatchDialog.svelte`'s own flow
  (its "Preview (Dry Run)" button), since a dry run previews the config that
  dialog is already building for the first selected file — not a separate
  standalone dialog.

  Field-by-field honesty (see `batch::dry_run` module doc comment for the
  full "which analysis steps run for real" writeup this panel must not
  misrepresent):
  - Input: real probed facts (`media::probe::probe`).
  - Resolved template: `None` renders as "no template resolved", never
    fabricated.
  - AI Decision: only ever present when the caller had no template chosen
    AND real AI settings were sent — shown as a proposal, never as something
    already applied (this panel does not offer an "Accept" button; picking a
    template from the dialog's own dropdown above is how a user would act on
    it).
  - Editing Plan: `predicted_removed_us`/`predicted_duration_us` render as
    "not computed" (never `0` or blank) when the backend didn't actually
    compute them (no audio track, or silence removal disabled) — a real
    "unknown", not a fabricated estimate.
  - Expected Output: the real resolved export preset/output path — no render
    ever happens, so this path is a prediction, not a promise a file exists.
-->
<script lang="ts">
  import { t } from "../../lib/i18n.svelte";
  import { formatTimecode } from "../../timeline/algebra";
  import type { Container, DryRunResult, VideoCodec } from "../../types/bindings";

  let { result }: { result: DryRunResult } = $props();

  function containerLabel(container: Container): string {
    switch (container) {
      case "mp_4":
        return "MP4";
      case "web_m":
        return "WebM";
    }
  }

  function codecLabel(codec: VideoCodec): string {
    switch (codec) {
      case "h264":
        return "H.264";
      case "h265":
        return "H.265";
      case "vp_9":
        return "VP9";
    }
  }

  function msFromUs(us: number): number {
    return Math.round(us / 1000);
  }
</script>

<div class="dr-panel">
  <section class="dr-section">
    <h4 class="dr-section-title">{t("dryRunPanel.inputTitle")}</h4>
    <p class="dr-line">
      {t("dryRunPanel.inputSummary", {
        duration: formatTimecode(result.input.duration_us),
        width: result.input.width,
        height: result.input.height,
        audio: result.input.has_audio ? t("dryRunPanel.hasAudio") : t("dryRunPanel.noAudio"),
        video: result.input.has_video ? t("dryRunPanel.hasVideo") : t("dryRunPanel.noVideo"),
      })}
    </p>
  </section>

  <section class="dr-section">
    <h4 class="dr-section-title">{t("dryRunPanel.templateTitle")}</h4>
    {#if result.resolved_template}
      <p class="dr-line">{result.resolved_template.name}</p>
    {:else}
      <p class="dr-line muted-2">{t("dryRunPanel.noTemplate")}</p>
    {/if}
  </section>

  <section class="dr-section">
    <h4 class="dr-section-title">{t("dryRunPanel.aiTitle")}</h4>
    {#if result.ai_decision}
      <p class="dr-line">
        {t("dryRunPanel.aiRecommendation", {
          name: result.ai_decision.template_name,
          reason: result.ai_decision.reason,
          confidence: Math.round(result.ai_decision.confidence * 100),
        })}
      </p>
      <p class="dr-line muted-2">{t("dryRunPanel.aiProposalNote", { name: result.ai_decision.template_name })}</p>
    {:else}
      <p class="dr-line muted-2">{t("dryRunPanel.aiNone")}</p>
    {/if}
  </section>

  <section class="dr-section">
    <h4 class="dr-section-title">{t("dryRunPanel.editingPlanTitle")}</h4>
    {#if result.editing_plan.silence_removal.enabled}
      <p class="dr-line">
        {t("dryRunPanel.silenceEnabled", {
          source:
            result.editing_plan.silence_removal.source === "explicit"
              ? t("dryRunPanel.sourceExplicit")
              : t("dryRunPanel.sourceTemplate"),
        })}
      </p>
      {#if result.editing_plan.silence_removal.params}
        <p class="dr-line muted-2">
          {t("dryRunPanel.silenceParams", {
            before: msFromUs(result.editing_plan.silence_removal.params.padding_before_us),
            after: msFromUs(result.editing_plan.silence_removal.params.padding_after_us),
            gap: msFromUs(result.editing_plan.silence_removal.params.merge_gap_us),
          })}
        </p>
      {/if}
      <p class="dr-line muted-2">
        {result.editing_plan.silence_removal.predicted_removed_us !== null
          ? t("dryRunPanel.predictedRemoved", {
              time: formatTimecode(result.editing_plan.silence_removal.predicted_removed_us),
            })
          : t("dryRunPanel.notComputed")}
      </p>
    {:else}
      <p class="dr-line muted-2">{t("dryRunPanel.silenceDisabled")}</p>
    {/if}

    {#if result.editing_plan.captions.enabled}
      <p class="dr-line">
        {t("dryRunPanel.captionsEnabled", { model: result.editing_plan.captions.transcription_model_id ?? "—" })}
      </p>
    {:else}
      <p class="dr-line muted-2">{t("dryRunPanel.captionsDisabled")}</p>
    {/if}
  </section>

  <section class="dr-section">
    <h4 class="dr-section-title">{t("dryRunPanel.expectedOutputTitle")}</h4>
    <p class="dr-line">
      <span class="dr-label">{t("dryRunPanel.outputPath")}:</span>
      <span class="dr-value" title={result.expected_output.output_path}>{result.expected_output.output_path}</span>
    </p>
    <p class="dr-line">
      <span class="dr-label">{t("dryRunPanel.predictedDuration")}:</span>
      <span class="dr-value">
        {result.expected_output.predicted_duration_us !== null
          ? formatTimecode(result.expected_output.predicted_duration_us)
          : t("dryRunPanel.notComputed")}
      </span>
    </p>
    <p class="dr-line">
      <span class="dr-label">{t("dryRunPanel.resolution")}:</span>
      <span class="dr-value">{result.expected_output.width}×{result.expected_output.height}</span>
    </p>
    <p class="dr-line">
      <span class="dr-label">{t("dryRunPanel.containerCodec")}:</span>
      <span class="dr-value">{containerLabel(result.expected_output.container)} / {codecLabel(result.expected_output.video_codec)}</span>
    </p>
  </section>
</div>

<style>
  .dr-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 10px 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .dr-section {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .dr-section-title {
    margin: 0;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .dr-line {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .dr-label {
    color: var(--muted);
  }
  .dr-value {
    font-family: inherit;
  }
</style>
