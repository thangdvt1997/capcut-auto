<!--
  Single-row "Preview" visualization for the Filler Word Detector (master
  prompt §16's "Preview" step) — a simpler cousin of
  `components/silence/SpeechRegionStrip.svelte` (same `usToPx` time<->pixel
  scale so it reads consistently with the rest of the app), reduced to one
  row since filler-word candidates don't have a separate "detected raw
  signal" band the way VAD scores do: every candidate already *is* the thing
  being previewed. Checked candidates render as "will be removed"; unchecked
  ones render as a lighter "kept" tick so the user can still see where they
  were detected without them being mistaken for an active removal.
-->
<script lang="ts">
  import { usToPx, type PxPerSecond, type Us } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import type { Cut } from "../../types/bindings";

  let {
    durationUs,
    candidates,
    checked,
  }: { durationUs: Us; candidates: Cut[]; checked: Record<string, boolean> } = $props();

  let containerWidthPx = $state(0);

  let pxPerSecond = $derived<PxPerSecond>(
    durationUs > 0 && containerWidthPx > 0 ? (containerWidthPx / durationUs) * 1_000_000 : 1,
  );

  function toPx(us: Us): number {
    return usToPx(us, pxPerSecond);
  }
</script>

<div class="strip" bind:clientWidth={containerWidthPx}>
  <div class="track">
    {#each candidates as cut (cut.id)}
      {@const isChecked = checked[cut.id] ?? false}
      <div
        class="band"
        class:checked={isChecked}
        style="left:{toPx(cut.start_us)}px; width:{Math.max(2, toPx(cut.end_us) - toPx(cut.start_us))}px;"
        title="{isChecked ? t('fillerWordDetector.stripWillRemove') : t('fillerWordDetector.stripKept')} ({(cut.start_us / 1_000_000).toFixed(2)}s – {(cut.end_us / 1_000_000).toFixed(2)}s)"
      ></div>
    {/each}
  </div>
</div>

<style>
  .strip {
    width: 100%;
    min-width: 0;
  }
  .track {
    position: relative;
    width: 100%;
    height: 22px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .band {
    position: absolute;
    top: 0;
    bottom: 0;
    background: hsl(210 20% 60% / 0.35);
  }
  .band.checked {
    background: repeating-linear-gradient(
      45deg,
      hsl(0 84% 65% / 0.55),
      hsl(0 84% 65% / 0.55) 4px,
      hsl(0 84% 65% / 0.35) 4px,
      hsl(0 84% 65% / 0.35) 8px
    );
  }
</style>
