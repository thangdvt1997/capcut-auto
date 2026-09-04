<!--
  Marker row (master-prompt §10 "Markers"): diamonds along a thin strip
  under the ruler. Click seeks the playhead there; double-click removes it.
  Markers themselves are frontend-only, session-local state — see
  `stores/timeline.svelte.ts`'s `TimelineMarker` doc comment for why
  (`ProjectV1` has no `markers` field yet; persisting them is a later-phase
  schema decision).
-->
<script lang="ts">
  import { usToPx, type Viewport } from "../../timeline/algebra";
  import type { TimelineMarker } from "../../stores/timeline.svelte";
  import { t } from "../../lib/i18n.svelte";

  let {
    markers,
    viewport,
    pxPerSecond,
    widthPx,
    onSeek,
    onRemove,
  }: {
    markers: TimelineMarker[];
    viewport: Viewport;
    pxPerSecond: number;
    widthPx: number;
    onSeek: (us: number) => void;
    onRemove: (id: string) => void;
  } = $props();

  const OVERSCAN_US = 2_000_000;
  let visible = $derived(
    markers.filter((m) => m.time_us >= viewport.startUs - OVERSCAN_US && m.time_us <= viewport.endUs + OVERSCAN_US),
  );
</script>

<div class="tl-markers" style="width:{widthPx}px">
  {#each visible as marker (marker.id)}
    <button
      class="tl-marker"
      style="left:{usToPx(marker.time_us, pxPerSecond)}px"
      title={marker.label || t("timelinePanel.markerTooltip")}
      onclick={() => onSeek(marker.time_us)}
      ondblclick={(e) => {
        e.stopPropagation();
        onRemove(marker.id);
      }}
      aria-label={t("timelinePanel.markerTooltip")}
    ></button>
  {/each}
</div>

<style>
  .tl-markers {
    position: relative;
    height: 100%;
    background: var(--surface);
  }
  .tl-marker {
    position: absolute;
    top: 2px;
    width: 10px;
    height: 10px;
    margin-left: -5px;
    background: var(--warn);
    border: none;
    transform: rotate(45deg);
    cursor: pointer;
    padding: 0;
  }
  .tl-marker:hover {
    outline: 1px solid var(--foreground);
  }
</style>
