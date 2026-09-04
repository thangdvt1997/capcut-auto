<!--
  Timeline ruler (master-prompt §10 "timeline ruler"): time markings that
  adapt to zoom level (`algebra.rulerTicks` picks a "nice" interval so tick
  spacing stays legible at any zoom), click-to-seek, and drag-to-scrub the
  playhead. Ticks are computed only for the visible viewport (virtualization,
  master prompt §50) — this component never renders a tick for the whole
  timeline length at once.
-->
<script lang="ts">
  import { rulerTicks, pxToUs, usToPx, type Viewport } from "../../timeline/algebra";

  let {
    viewport,
    pxPerSecond,
    widthPx,
    onSeek,
  }: {
    viewport: Viewport;
    pxPerSecond: number;
    widthPx: number;
    onSeek: (us: number) => void;
  } = $props();

  let ticks = $derived(rulerTicks(viewport, pxPerSecond));
  let el: HTMLDivElement | undefined = $state();

  function seekFromClientX(clientX: number): void {
    if (!el) return;
    const rect = el.getBoundingClientRect();
    onSeek(pxToUs(clientX - rect.left, pxPerSecond));
  }

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    seekFromClientX(e.clientX);
    function onMove(ev: PointerEvent): void {
      seekFromClientX(ev.clientX);
    }
    function onUp(): void {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }
</script>

<div
  bind:this={el}
  class="tl-ruler"
  style="width:{widthPx}px"
  onpointerdown={onPointerDown}
  role="slider"
  aria-label="timeline ruler"
  aria-valuenow={viewport.startUs}
  tabindex="-1"
>
  {#each ticks as tick (tick.us)}
    <div class="tl-tick" class:major={tick.major} style="left:{usToPx(tick.us, pxPerSecond)}px">
      {#if tick.major}<span class="tl-tick-label mono">{tick.label}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .tl-ruler {
    position: relative;
    height: 100%;
    background: var(--surface-2);
    cursor: pointer;
    user-select: none;
    overflow: hidden;
  }
  .tl-tick {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--border);
  }
  .tl-tick.major {
    background: var(--border-strong);
  }
  .tl-tick-label {
    position: absolute;
    top: 3px;
    left: 4px;
    font-size: 10px;
    color: var(--muted-2);
    white-space: nowrap;
  }
</style>
