<!--
  Region visualization for the Silence Detector (master prompt §12's own
  example: alternating labeled SPEECH/SILENCE bands). Two rows sharing one
  time<->pixel scale (reusing `src/timeline/algebra.ts`'s `usToPx`, the same
  conversion the main `Timeline.svelte` ruler uses, so this reads
  consistently with the rest of the app even though it's a standalone bar
  inside this dialog, not embedded in the main timeline):

    - "Detected" — raw VAD output: each `SpeechSegment` is a SPEECH band,
      every gap between segments (and before/after them) is SILENCE.
    - "Preview" — the proposed cutlist after padding/merge: every `Cut`
      (kind `Remove`) is a band that will be removed on Apply, the
      complement is what's kept.

  Pure presentational: takes `durationUs`/`segments`/`cuts` and lays them out
  at whatever pixel width its container measures.
-->
<script lang="ts">
  import { usToPx, type PxPerSecond, type Us } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import type { Cut, SpeechSegment } from "../../types/bindings";

  let { durationUs, segments, cuts }: { durationUs: Us; segments: SpeechSegment[]; cuts: Cut[] } = $props();

  let containerWidthPx = $state(0);

  let pxPerSecond = $derived<PxPerSecond>(
    durationUs > 0 && containerWidthPx > 0 ? (containerWidthPx / durationUs) * 1_000_000 : 1,
  );

  function toPx(us: Us): number {
    return usToPx(us, pxPerSecond);
  }

  interface Band {
    startUs: Us;
    endUs: Us;
    kind: "speech" | "silence" | "keep" | "remove";
  }

  /** Detected speech bands plus the silence gaps between/around them,
   * covering the full `[0, durationUs]` span. */
  let detectedBands = $derived.by((): Band[] => {
    if (durationUs <= 0) return [];
    const sorted = [...segments].sort((a, b) => a.start_us - b.start_us);
    const bands: Band[] = [];
    let cursor = 0;
    for (const seg of sorted) {
      if (seg.start_us > cursor) bands.push({ startUs: cursor, endUs: seg.start_us, kind: "silence" });
      bands.push({ startUs: Math.max(cursor, seg.start_us), endUs: Math.max(seg.start_us, seg.end_us), kind: "speech" });
      cursor = Math.max(cursor, seg.end_us);
    }
    if (cursor < durationUs) bands.push({ startUs: cursor, endUs: durationUs, kind: "silence" });
    return bands;
  });

  /** Proposed Remove cuts plus the kept gaps between/around them. `cuts` is
   * already just the `Remove` intervals (`build_silence_cutlist`'s
   * contract) — the "keep" regions are whatever isn't covered. */
  let previewBands = $derived.by((): Band[] => {
    if (durationUs <= 0) return [];
    const sorted = [...cuts].sort((a, b) => a.start_us - b.start_us);
    const bands: Band[] = [];
    let cursor = 0;
    for (const cut of sorted) {
      if (cut.start_us > cursor) bands.push({ startUs: cursor, endUs: cut.start_us, kind: "keep" });
      bands.push({ startUs: Math.max(cursor, cut.start_us), endUs: Math.max(cut.start_us, cut.end_us), kind: "remove" });
      cursor = Math.max(cursor, cut.end_us);
    }
    if (cursor < durationUs) bands.push({ startUs: cursor, endUs: durationUs, kind: "keep" });
    return bands;
  });

  function bandLabel(kind: Band["kind"]): string {
    switch (kind) {
      case "speech":
        return t("silenceDetector.bandSpeech");
      case "silence":
        return t("silenceDetector.bandSilence");
      case "keep":
        return t("silenceDetector.bandKeep");
      case "remove":
        return t("silenceDetector.bandRemove");
    }
  }
</script>

<div class="region-strip" bind:clientWidth={containerWidthPx}>
  <div class="rs-row">
    <span class="rs-row-label muted-2">{t("silenceDetector.detectedRowLabel")}</span>
    <div class="rs-track">
      {#each detectedBands as band, i (i)}
        <div
          class="rs-band kind-{band.kind}"
          style="left:{toPx(band.startUs)}px; width:{Math.max(1, toPx(band.endUs) - toPx(band.startUs))}px;"
          title="{bandLabel(band.kind)} ({(band.startUs / 1_000_000).toFixed(2)}s – {(band.endUs / 1_000_000).toFixed(2)}s)"
        >
          {#if toPx(band.endUs) - toPx(band.startUs) > 34}
            <span class="rs-band-label">{bandLabel(band.kind)}</span>
          {/if}
        </div>
      {/each}
    </div>
  </div>
  <div class="rs-row">
    <span class="rs-row-label muted-2">{t("silenceDetector.previewRowLabel")}</span>
    <div class="rs-track">
      {#each previewBands as band, i (i)}
        <div
          class="rs-band kind-{band.kind}"
          style="left:{toPx(band.startUs)}px; width:{Math.max(1, toPx(band.endUs) - toPx(band.startUs))}px;"
          title="{bandLabel(band.kind)} ({(band.startUs / 1_000_000).toFixed(2)}s – {(band.endUs / 1_000_000).toFixed(2)}s)"
        >
          {#if toPx(band.endUs) - toPx(band.startUs) > 34}
            <span class="rs-band-label">{bandLabel(band.kind)}</span>
          {/if}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .region-strip {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    min-width: 0;
  }
  .rs-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .rs-row-label {
    width: 64px;
    flex-shrink: 0;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .rs-track {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 28px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .rs-band {
    position: absolute;
    top: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border-right: 1px solid var(--surface-2);
  }
  .rs-band-label {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: hsl(0 0% 100% / 0.85);
    white-space: nowrap;
  }
  .rs-band.kind-speech { background: hsl(142 71% 55% / 0.55); }
  .rs-band.kind-silence { background: hsl(240 4% 22% / 0.7); }
  .rs-band.kind-keep { background: hsl(142 71% 55% / 0.35); }
  .rs-band.kind-remove {
    background: repeating-linear-gradient(
      45deg,
      hsl(0 84% 65% / 0.55),
      hsl(0 84% 65% / 0.55) 4px,
      hsl(0 84% 65% / 0.35) 4px,
      hsl(0 84% 65% / 0.35) 8px
    );
  }
</style>
