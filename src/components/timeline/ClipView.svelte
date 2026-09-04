<!--
  One timeline clip (master-prompt §10): click to select (ctrl/shift-click
  additive), drag to move (including across tracks), edge-drag to trim
  start/end, double-click to split at the playhead. Video clips show a
  thumbnail filmstrip (`generate_thumbnail_strip`, the other narrow command
  this pass added); audio clips show a waveform (`Waveform.svelte`).

  Drag/trim gestures update a purely-local, purely-visual offset while the
  pointer is down and only call the real backend command once on pointerup
  (master-prompt §50: don't recompute/re-invoke on every pixel of a drag).
-->
<script module lang="ts">
  import type { ThumbnailStripFrame } from "../../types/bindings";
  const thumbnailCache = new Map<string, Promise<ThumbnailStripFrame[]>>();
</script>

<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { timeline } from "../../stores/timeline.svelte";
  import { commands } from "../../types/bindings";
  import type { Clip, MediaItem, Track } from "../../types/bindings";
  import { clipEndUs, clipTimelineDurationUs, pxToUs, throttleRaf, TRACK_ROW_HEIGHT_PX, usToPx } from "../../timeline/algebra";
  import Waveform from "./Waveform.svelte";
  import { t } from "../../lib/i18n.svelte";

  let { clip, track }: { clip: Clip; track: Track } = $props();

  const MIN_WIDTH_FOR_HANDLES_PX = 20;
  const MAX_THUMBNAILS = 20;
  const THUMBNAIL_TARGET_WIDTH_PX = 96;

  let rowIndex = $derived(timeline.tracks.findIndex((tr) => tr.id === track.id));
  let media = $derived<MediaItem | null>((clip.media_id ? timeline.mediaById.get(clip.media_id) : undefined) ?? null);

  let durationUs = $derived(clipTimelineDurationUs(clip));
  let leftPx = $derived(usToPx(clip.position_us, timeline.pxPerSecond));
  let widthPx = $derived(Math.max(2, usToPx(durationUs, timeline.pxPerSecond)));
  let topPx = $derived(rowIndex >= 0 ? rowIndex * TRACK_ROW_HEIGHT_PX : 0);
  let selected = $derived(timeline.isSelected(clip.id));
  let showHandles = $derived(widthPx >= MIN_WIDTH_FOR_HANDLES_PX && !track.locked);

  // Local drag/trim visual state (see file doc comment).
  let dragMode = $state<"move" | "trim-start" | "trim-end" | null>(null);
  let dragDeltaPx = $state(0);
  let dragRowDelta = $state(0);

  let visualLeftPx = $derived(dragMode === "move" || dragMode === "trim-start" ? leftPx + dragDeltaPx : leftPx);
  let visualWidthPx = $derived(
    dragMode === "trim-start"
      ? Math.max(2, widthPx - dragDeltaPx)
      : dragMode === "trim-end"
        ? Math.max(2, widthPx + dragDeltaPx)
        : widthPx,
  );
  let visualTopPx = $derived(topPx + dragRowDelta * TRACK_ROW_HEIGHT_PX);

  function clampRowDelta(delta: number): number {
    if (rowIndex < 0) return 0;
    const target = Math.min(timeline.tracks.length - 1, Math.max(0, rowIndex + delta));
    return target - rowIndex;
  }

  function startDrag(e: PointerEvent, mode: "move" | "trim-start" | "trim-end"): void {
    if (e.button !== 0 || track.locked) return;
    e.stopPropagation();
    timeline.selectClip(clip.id, { additive: e.ctrlKey || e.metaKey || e.shiftKey });

    const startX = e.clientX;
    const startY = e.clientY;
    const pxPerSecond = timeline.pxPerSecond;
    const originalPosition = clip.position_us;
    const originalEnd = clipEndUs(clip);
    dragMode = mode;
    dragDeltaPx = 0;
    dragRowDelta = 0;

    const applyPreview = throttleRaf((clientX: number, clientY: number) => {
      dragDeltaPx = clientX - startX;
      if (mode === "move") {
        dragRowDelta = clampRowDelta(Math.round((clientY - startY) / TRACK_ROW_HEIGHT_PX));
      }
    });

    function onMove(ev: PointerEvent): void {
      applyPreview(ev.clientX, ev.clientY);
    }

    async function onUp(ev: PointerEvent): Promise<void> {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      const deltaPx = ev.clientX - startX;
      const deltaUs = pxToUs(deltaPx, pxPerSecond);
      const rowDelta = mode === "move" ? clampRowDelta(Math.round((ev.clientY - startY) / TRACK_ROW_HEIGHT_PX)) : 0;
      dragMode = null;
      dragDeltaPx = 0;
      dragRowDelta = 0;

      if (mode === "move") {
        if (deltaUs === 0 && rowDelta === 0) return;
        let newPosition = Math.max(0, originalPosition + deltaUs);
        newPosition = await timeline.snap(newPosition, clip.id);
        const targetTrack = timeline.tracks[rowIndex + rowDelta] ?? track;
        await timeline.moveClip(clip.id, targetTrack.id, newPosition);
      } else if (mode === "trim-start") {
        if (deltaUs === 0) return;
        const newStart = await timeline.snap(originalPosition + deltaUs, clip.id);
        await timeline.trimClipStart(clip.id, newStart);
      } else {
        if (deltaUs === 0) return;
        const newEnd = await timeline.snap(originalEnd + deltaUs, clip.id);
        await timeline.trimClipEnd(clip.id, newEnd);
      }
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  async function onDoubleClick(e: MouseEvent): Promise<void> {
    e.stopPropagation();
    if (track.locked) return;
    const at = timeline.playheadUs;
    if (at > clip.position_us && at < clipEndUs(clip)) {
      await timeline.splitClip(clip.id, at);
    }
  }

  function filenameOf(item: MediaItem): string {
    return item.source_path.split(/[\\/]/).pop() ?? item.source_path;
  }

  // Thumbnail filmstrip (video clips only): debounced against width so a
  // continuous trim-drag or zoom change doesn't fire one IPC call per pixel.
  let thumbnails = $state<ThumbnailStripFrame[]>([]);

  $effect(() => {
    const m = media;
    const width = visualWidthPx;
    if (!m || m.kind !== "video" || track.kind !== "video") {
      thumbnails = [];
      return;
    }
    const count = Math.min(MAX_THUMBNAILS, Math.max(1, Math.round(width / THUMBNAIL_TARGET_WIDTH_PX)));
    const timer = setTimeout(() => void loadThumbnails(m, count), 200);
    return () => clearTimeout(timer);
  });

  async function loadThumbnails(item: MediaItem, count: number): Promise<void> {
    const key = `${item.id}:${count}`;
    let promise = thumbnailCache.get(key);
    if (!promise) {
      promise = commands
        .generateThumbnailStrip(item.id, item.proxy_path ?? item.source_path, item.duration_us, count)
        .then((r) => (r.status === "ok" ? r.data : []));
      thumbnailCache.set(key, promise);
    }
    thumbnails = await promise;
  }
</script>

<div
  class="tl-clip kind-{track.kind}"
  class:selected
  class:disabled={!clip.enabled}
  class:locked={track.locked}
  style="left:{visualLeftPx}px; top:{visualTopPx}px; width:{visualWidthPx}px; height:{TRACK_ROW_HEIGHT_PX - 6}px;"
  onpointerdown={(e) => startDrag(e, "move")}
  ondblclick={onDoubleClick}
  title={media ? filenameOf(media) : clip.id}
  role="button"
  tabindex="0"
>
  {#if showHandles}
    <div
      class="tl-clip-handle left"
      onpointerdown={(e) => {
        e.stopPropagation();
        startDrag(e, "trim-start");
      }}
      role="slider"
      aria-label={t("timelinePanel.trimStartHandle")}
      aria-orientation="horizontal"
      aria-valuenow={clip.position_us}
      tabindex="-1"
    ></div>
  {/if}

  <div class="tl-clip-body">
    {#if track.kind === "audio" && media}
      <Waveform {media} widthPx={visualWidthPx} heightPx={TRACK_ROW_HEIGHT_PX - 20} />
    {:else if track.kind === "video" && media}
      <div class="tl-clip-thumbs">
        {#each thumbnails as frame (frame.timestamp_us)}
          <img src={convertFileSrc(frame.path)} alt="" draggable="false" />
        {/each}
      </div>
    {/if}
    <span class="tl-clip-label">{media ? filenameOf(media) : t("timelinePanel.clipEmptyLabel")}</span>
  </div>

  {#if showHandles}
    <div
      class="tl-clip-handle right"
      onpointerdown={(e) => {
        e.stopPropagation();
        startDrag(e, "trim-end");
      }}
      role="slider"
      aria-label={t("timelinePanel.trimEndHandle")}
      aria-orientation="horizontal"
      aria-valuenow={clipEndUs(clip)}
      tabindex="-1"
    ></div>
  {/if}
</div>

<style>
  .tl-clip {
    position: absolute;
    box-sizing: border-box;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    overflow: hidden;
    cursor: grab;
    pointer-events: auto;
    user-select: none;
  }
  .tl-clip.kind-video {
    background: hsl(213 94% 68% / 0.18);
  }
  .tl-clip.kind-audio {
    background: hsl(142 71% 55% / 0.14);
  }
  .tl-clip.kind-caption {
    background: hsl(38 92% 60% / 0.16);
  }
  .tl-clip.kind-image {
    background: hsl(280 80% 70% / 0.16);
  }
  .tl-clip.kind-overlay {
    background: hsl(320 80% 70% / 0.16);
  }
  .tl-clip.kind-effect {
    background: hsl(0 84% 65% / 0.14);
  }
  .tl-clip.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .tl-clip.disabled {
    opacity: 0.4;
  }
  .tl-clip.locked {
    cursor: not-allowed;
  }
  .tl-clip-body {
    position: relative;
    height: 100%;
    display: flex;
    align-items: flex-end;
    padding: 2px 4px;
  }
  .tl-clip-thumbs {
    position: absolute;
    inset: 0;
    display: flex;
    overflow: hidden;
  }
  .tl-clip-thumbs img {
    height: 100%;
    flex: 1 1 auto;
    min-width: 0;
    object-fit: cover;
    opacity: 0.85;
  }
  .tl-clip-label {
    position: relative;
    z-index: 1;
    font-size: 10px;
    color: var(--foreground);
    text-shadow: 0 1px 2px hsl(0 0% 0% / 0.8);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tl-clip-handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 7px;
    cursor: ew-resize;
    z-index: 2;
  }
  .tl-clip-handle.left {
    left: 0;
  }
  .tl-clip-handle.right {
    right: 0;
  }
  .tl-clip-handle:hover {
    background: hsl(0 0% 100% / 0.25);
  }
</style>
