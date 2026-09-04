<!--
  Timeline root (master-prompt §10/§48): ruler + marker row + track rows +
  playhead + selection-region marquee, horizontal (native) scroll, zoom
  controls, and the master-prompt §49 keyboard shortcuts. Mounted by
  `layout/TimelinePanel.svelte`, which used to be a static V2/V1/A1/CC
  mockup — this is the real thing it always said it would become.

  Layout: one native-scrolling container. Each track row is a flex pair of
  a sticky-left header cell (`TrackHeader`) and a lane cell; the ruler and
  marker rows use the same sticky-corner pattern so the header column stays
  pinned while the timeline content scrolls horizontally underneath it.
  Clips are rendered in one absolutely-positioned overlay layer spanning
  every row (not nested per-lane) so a cross-track drag is just a `top`
  change, and only clips intersecting the visible viewport (+ overscan) are
  mounted at all (master-prompt §50 virtualization).
-->
<script lang="ts">
  import { timeline } from "../../stores/timeline.svelte";
  import {
    clipsInSelectionRange,
    pxToUs,
    throttleRaf,
    TRACK_ROW_HEIGHT_PX,
    usToPx,
    viewportFromScroll,
    visibleClips,
  } from "../../timeline/algebra";
  import type { Clip, Track } from "../../types/bindings";
  import TrackHeader from "./TrackHeader.svelte";
  import ClipView from "./ClipView.svelte";
  import Ruler from "./Ruler.svelte";
  import Markers from "./Markers.svelte";
  import SyncGroupDialog from "./SyncGroupDialog.svelte";
  import SilenceDetector from "../silence/SilenceDetector.svelte";
  import { silenceDetector } from "../../stores/silenceDetector.svelte";
  import { t } from "../../lib/i18n.svelte";

  // Phase 5 (master prompt §12/§39): both new dialogs are toolbar-triggered
  // from here, next to the rest of the multi-select actions, since that's
  // where the user already has clips selected/visible. See
  // `SilenceDetector.svelte`/`SyncGroupDialog.svelte`'s own doc comments for
  // the full placement rationale.
  let syncDialogOpen = $state(false);

  const HEADER_WIDTH_PX = 168;
  const RULER_HEIGHT_PX = 26;
  const MARKERS_HEIGHT_PX = 14;
  const TRAILING_PADDING_PX = 240;
  const OVERSCAN_PX = 400;

  let scrollEl: HTMLDivElement | undefined = $state();
  let overlayEl: HTMLDivElement | undefined = $state();
  let rootEl: HTMLDivElement | undefined = $state();
  let scrollViewportWidthPx = $state(0);

  let laneViewportWidthPx = $derived(Math.max(0, scrollViewportWidthPx - HEADER_WIDTH_PX));
  let viewport = $derived(viewportFromScroll(timeline.scrollLeftPx, laneViewportWidthPx, timeline.pxPerSecond));
  let contentWidthPx = $derived(
    Math.max(laneViewportWidthPx, usToPx(timeline.durationUs, timeline.pxPerSecond) + TRAILING_PADDING_PX),
  );
  let lanesHeightPx = $derived(Math.max(TRACK_ROW_HEIGHT_PX, timeline.tracks.length * TRACK_ROW_HEIGHT_PX));
  let innerWidthPx = $derived(HEADER_WIDTH_PX + contentWidthPx);
  let innerHeightPx = $derived(RULER_HEIGHT_PX + MARKERS_HEIGHT_PX + lanesHeightPx);
  let playheadLeftPx = $derived(HEADER_WIDTH_PX + usToPx(timeline.playheadUs, timeline.pxPerSecond));

  let visibleEntries = $derived.by(() => {
    const overscanUs = pxToUs(OVERSCAN_PX, timeline.pxPerSecond);
    const out: { clip: Clip; track: Track }[] = [];
    for (const track of timeline.tracks) {
      const clips = timeline.clipsByTrack.get(track.id) ?? [];
      for (const clip of visibleClips(clips, viewport, overscanUs)) {
        out.push({ clip, track });
      }
    }
    return out;
  });

  const onScroll = throttleRaf(() => {
    if (scrollEl) timeline.setScrollLeft(scrollEl.scrollLeft);
  });

  // Keep the playhead in view when it moves off-screen (arrow-key seek,
  // split/trim landing outside the current scroll position, etc.) without
  // fighting a scroll the user is actively performing themselves.
  $effect(() => {
    if (!scrollEl) return;
    const playheadPx = usToPx(timeline.playheadUs, timeline.pxPerSecond);
    const viewStart = scrollEl.scrollLeft;
    const viewEnd = viewStart + laneViewportWidthPx;
    if (playheadPx < viewStart || playheadPx > viewEnd) {
      scrollEl.scrollLeft = Math.max(0, playheadPx - laneViewportWidthPx / 2);
    }
  });

  function seekTo(us: number): void {
    timeline.setPlayhead(us);
  }

  // -------------------------------------------------------------------
  // Selection-region marquee (empty-lane drag-select)
  // -------------------------------------------------------------------

  let marquee = $state<{ startUs: number; startRow: number; curUs: number; curRow: number } | null>(null);
  let marqueeMoved = $state(false);

  function clampRow(row: number): number {
    return Math.min(Math.max(0, row), Math.max(0, timeline.tracks.length - 1));
  }

  function pointToDomain(clientX: number, clientY: number): { us: number; row: number } {
    if (!overlayEl) return { us: 0, row: 0 };
    const rect = overlayEl.getBoundingClientRect();
    return {
      us: Math.max(0, pxToUs(clientX - rect.left, timeline.pxPerSecond)),
      row: clampRow(Math.floor((clientY - rect.top) / TRACK_ROW_HEIGHT_PX)),
    };
  }

  function onLaneBgPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    const additive = e.ctrlKey || e.metaKey || e.shiftKey;
    const start = pointToDomain(e.clientX, e.clientY);
    marquee = { startUs: start.us, startRow: start.row, curUs: start.us, curRow: start.row };
    marqueeMoved = false;

    const updatePreview = throttleRaf((clientX: number, clientY: number) => {
      const cur = pointToDomain(clientX, clientY);
      marqueeMoved = true;
      marquee = marquee && { ...marquee, curUs: cur.us, curRow: cur.row };
    });

    function onMove(ev: PointerEvent): void {
      updatePreview(ev.clientX, ev.clientY);
    }

    function onUp(): void {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      if (marquee) {
        if (marqueeMoved) {
          const range = {
            minTrackIndex: Math.min(marquee.startRow, marquee.curRow),
            maxTrackIndex: Math.max(marquee.startRow, marquee.curRow),
            minUs: Math.min(marquee.startUs, marquee.curUs),
            maxUs: Math.max(marquee.startUs, marquee.curUs),
          };
          const ids = clipsInSelectionRange(timeline.clips, timeline.tracks, range);
          if (additive) {
            const next = new Set(timeline.selectedClipIds);
            ids.forEach((id) => next.add(id));
            timeline.setSelection(Array.from(next));
          } else {
            timeline.setSelection(ids);
          }
        } else if (!additive) {
          timeline.clearSelection();
        }
      }
      marquee = null;
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  // -------------------------------------------------------------------
  // Keyboard shortcuts (master prompt §49), scoped to this component
  // having focus so typing elsewhere in the app is never hijacked.
  // -------------------------------------------------------------------

  function isTypingTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable;
  }

  function frameDurationUs(): number {
    const fps = timeline.project?.canvas.fps;
    if (!fps || fps.num <= 0) return 33_333;
    return Math.round((fps.den / fps.num) * 1_000_000);
  }

  const LARGE_SEEK_US = 1_000_000;

  function onKeyDown(e: KeyboardEvent): void {
    if (isTypingTarget(e.target)) return;
    const ctrl = e.ctrlKey || e.metaKey;

    if (ctrl && e.shiftKey && e.key.toLowerCase() === "z") {
      e.preventDefault();
      void timeline.redo();
      return;
    }
    if (ctrl && e.key.toLowerCase() === "z") {
      e.preventDefault();
      void timeline.undo();
      return;
    }
    if (ctrl && e.key.toLowerCase() === "c") {
      e.preventDefault();
      void timeline.copySelected();
      return;
    }
    if (ctrl && e.key.toLowerCase() === "v") {
      e.preventDefault();
      void timeline.paste();
      return;
    }
    if (ctrl && (e.key === "+" || e.key === "=")) {
      e.preventDefault();
      timeline.zoomIn();
      return;
    }
    if (ctrl && e.key === "-") {
      e.preventDefault();
      timeline.zoomOut();
      return;
    }
    // Ctrl+S (save) is intentionally NOT handled here: no project-save
    // command exists yet (only `new_project`/`load_timeline_project`), and
    // this task's brief says to skip wiring it rather than invent one.
    if (ctrl) return;

    if (e.key === "Delete") {
      e.preventDefault();
      void timeline.deleteSelected();
      return;
    }
    if (!e.shiftKey && e.key.toLowerCase() === "s") {
      e.preventDefault();
      void timeline.splitAtPlayhead();
      return;
    }
    if (e.key === " ") {
      e.preventDefault();
      timeline.previewApi.togglePlayPause?.();
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      timeline.seekBy(e.shiftKey ? -LARGE_SEEK_US : -frameDurationUs());
      return;
    }
    if (e.key === "ArrowRight") {
      e.preventDefault();
      timeline.seekBy(e.shiftKey ? LARGE_SEEK_US : frameDurationUs());
      return;
    }
  }

  function focusRoot(): void {
    rootEl?.focus();
  }
</script>

<!--
  This root panel is a custom keyboard-driven widget (master-prompt §49
  shortcuts) with no single native ARIA role that fits "a whole editing
  surface with its own keymap" — `role="application"` is a landmark role,
  not one eslint-plugin-svelte's a11y checks treat as interactive, so
  there's no role that would satisfy both compiler checks below without
  making the semantics worse. Suppressed deliberately, same pattern as the
  `a11y_media_has_caption` suppression already in `VideoPlayer.svelte`.
-->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="timeline-panel"
  bind:this={rootEl}
  tabindex="0"
  onkeydown={onKeyDown}
  onpointerdowncapture={focusRoot}
  role="application"
  aria-label={t("timelinePanel.title")}
>
  <div class="tl-toolbar">
    <span class="tl-title">{t("timelinePanel.title")}</span>
    <span class="tl-toolbar-group">
      <button class="btn btn-ghost" onclick={() => void timeline.undo()} title={t("timelinePanel.undo")}>↶</button>
      <button class="btn btn-ghost" onclick={() => void timeline.redo()} title={t("timelinePanel.redo")}>↷</button>
    </span>
    <span class="tl-toolbar-group">
      <button
        class="btn btn-ghost"
        disabled={timeline.selectedClipIds.size === 0}
        onclick={() => void timeline.copySelected()}
        title={t("timelinePanel.copy")}
      >⧉</button>
      <button
        class="btn btn-ghost"
        disabled={!timeline.hasClipboardContent}
        onclick={() => void timeline.paste()}
        title={t("timelinePanel.paste")}
      >📋</button>
      <button
        class="btn btn-ghost"
        disabled={timeline.selectedClipIds.size === 0}
        onclick={() => void timeline.deleteSelected()}
        title={t("timelinePanel.deleteSelected")}
      >🗑</button>
      <button class="btn btn-ghost" onclick={() => void timeline.splitAtPlayhead()} title={t("timelinePanel.splitAtPlayhead")}>✂</button>
      <button class="btn btn-ghost" onclick={() => timeline.addMarker(timeline.playheadUs)} title={t("timelinePanel.addMarker")}>◆+</button>
    </span>
    <span class="tl-toolbar-group">
      <button class="btn btn-ghost" onclick={() => silenceDetector.openFor()} title={t("mediaLibrary.detectSilence")}>
        {t("timelinePanel.silenceDetectorButton")}
      </button>
      <button
        class="btn btn-ghost"
        disabled={timeline.selectedClipIds.size < 2}
        onclick={() => (syncDialogOpen = true)}
        title={timeline.selectedClipIds.size < 2 ? t("timelinePanel.syncGroupNeedsTwo") : t("timelinePanel.syncGroupButton")}
      >
        {t("timelinePanel.syncGroupButton")}
      </button>
    </span>
    <span class="tl-toolbar-spacer"></span>
    <span class="tl-toolbar-group">
      <button class="btn btn-ghost" onclick={() => timeline.zoomOut()} title={t("timelinePanel.zoomOut")}>−</button>
      <span class="tl-zoom-label mono muted-2">{Math.round(timeline.pxPerSecond)}px/s</span>
      <button class="btn btn-ghost" onclick={() => timeline.zoomIn()} title={t("timelinePanel.zoomIn")}>+</button>
    </span>
  </div>

  {#if timeline.lastError}
    <div class="tl-error">{timeline.lastError}</div>
  {/if}

  {#if !timeline.project}
    <div class="tl-empty muted-2">{t("timelinePanel.noProject")}</div>
  {:else}
    <div
      class="tl-scroll"
      bind:this={scrollEl}
      bind:clientWidth={scrollViewportWidthPx}
      onscroll={onScroll}
    >
      <div class="tl-inner" style="width:{innerWidthPx}px; height:{innerHeightPx}px;">
        <div class="tl-row sticky-row" style="top:0px; height:{RULER_HEIGHT_PX}px;">
          <div class="tl-corner" style="width:{HEADER_WIDTH_PX}px;"></div>
          <Ruler {viewport} pxPerSecond={timeline.pxPerSecond} widthPx={contentWidthPx} onSeek={seekTo} />
        </div>
        <div class="tl-row sticky-row" style="top:{RULER_HEIGHT_PX}px; height:{MARKERS_HEIGHT_PX}px;">
          <div class="tl-corner" style="width:{HEADER_WIDTH_PX}px;"></div>
          <Markers
            markers={timeline.markers}
            {viewport}
            pxPerSecond={timeline.pxPerSecond}
            widthPx={contentWidthPx}
            onSeek={seekTo}
            onRemove={(id) => timeline.removeMarker(id)}
          />
        </div>

        {#each timeline.tracks as track (track.id)}
          <div class="tl-row" style="height:{TRACK_ROW_HEIGHT_PX}px;">
            <div class="tl-header-cell" style="width:{HEADER_WIDTH_PX}px;">
              <TrackHeader {track} />
            </div>
            <div
              class="tl-lane-bg kind-{track.kind}"
              style="width:{contentWidthPx}px;"
              onpointerdown={onLaneBgPointerDown}
              role="button"
              tabindex="-1"
              aria-label={t("timelinePanel.lanesRegion")}
            ></div>
          </div>
        {/each}

        <div
          bind:this={overlayEl}
          class="tl-clips-layer"
          style="left:{HEADER_WIDTH_PX}px; top:{RULER_HEIGHT_PX + MARKERS_HEIGHT_PX}px; width:{contentWidthPx}px; height:{lanesHeightPx}px;"
        >
          {#each visibleEntries as entry (entry.clip.id)}
            <ClipView clip={entry.clip} track={entry.track} />
          {/each}

          {#if marquee && marqueeMoved}
            {@const x1 = usToPx(Math.min(marquee.startUs, marquee.curUs), timeline.pxPerSecond)}
            {@const x2 = usToPx(Math.max(marquee.startUs, marquee.curUs), timeline.pxPerSecond)}
            {@const y1 = Math.min(marquee.startRow, marquee.curRow) * TRACK_ROW_HEIGHT_PX}
            {@const y2 = (Math.max(marquee.startRow, marquee.curRow) + 1) * TRACK_ROW_HEIGHT_PX}
            <div class="tl-marquee" style="left:{x1}px; top:{y1}px; width:{x2 - x1}px; height:{y2 - y1}px;"></div>
          {/if}
        </div>

        <div class="tl-playhead" style="left:{playheadLeftPx}px; height:{innerHeightPx}px;"></div>
      </div>
    </div>
  {/if}

  <SyncGroupDialog bind:open={syncDialogOpen} />
  <SilenceDetector />
</div>

<style>
  .timeline-panel {
    /* `width: 100%` + `min-width: 0` (not just `min-height: 0`) matters
       here: this panel sits in a CSS Grid cell (`ResizableSplit.svelte`'s
       `.pane`) whose own `overflow: hidden` stops IT from growing, but
       without an explicit min-width reset on *this* element too, a wide
       descendant (`.tl-inner` below, sized to the full timeline content
       width in px, easily thousands of pixels for a real project) still
       bubbles up as this flex column's own intrinsic/preferred width and
       pushes later flex children (the zoom controls) out past the visible
       area instead of `.tl-scroll`'s own `overflow: auto` containing it. */
    width: 100%;
    min-width: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    outline: none;
  }
  .tl-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    min-width: 0;
  }
  .tl-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    margin-right: 4px;
  }
  .tl-toolbar-group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .tl-toolbar-group .btn {
    height: 22px;
    padding: 0 6px;
    font-size: 12px;
  }
  .tl-toolbar-spacer {
    flex: 1;
  }
  .tl-zoom-label {
    font-size: 10px;
    min-width: 48px;
    text-align: center;
  }
  .tl-error {
    padding: 4px 10px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border-bottom: 1px solid hsl(0 84% 65% / 0.3);
    flex-shrink: 0;
  }
  .tl-empty {
    flex: 1;
    display: grid;
    place-items: center;
    font-size: 12px;
  }
  .tl-scroll {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    position: relative;
  }
  .tl-inner {
    position: relative;
  }
  .tl-row {
    display: flex;
    align-items: stretch;
  }
  .sticky-row {
    position: sticky;
    z-index: 4;
    background: var(--surface);
  }
  .tl-corner {
    flex-shrink: 0;
    background: var(--surface-2);
    border-right: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  .tl-header-cell {
    position: sticky;
    left: 0;
    z-index: 3;
    flex-shrink: 0;
  }
  .tl-lane-bg {
    flex-shrink: 0;
    position: relative;
    background-image: repeating-linear-gradient(
      90deg,
      transparent,
      transparent 79px,
      var(--border) 79px,
      var(--border) 80px
    );
    border-bottom: 1px solid var(--surface-2);
  }
  .tl-lane-bg.kind-video { background-color: hsl(213 94% 68% / 0.03); }
  .tl-lane-bg.kind-audio { background-color: hsl(142 71% 55% / 0.03); }
  .tl-lane-bg.kind-caption { background-color: hsl(38 92% 60% / 0.03); }
  .tl-clips-layer {
    position: absolute;
    pointer-events: none;
  }
  .tl-marquee {
    position: absolute;
    border: 1px solid var(--accent);
    background: hsl(213 94% 68% / 0.12);
    pointer-events: none;
  }
  .tl-playhead {
    position: absolute;
    top: 0;
    width: 2px;
    background: var(--neg);
    z-index: 5;
    pointer-events: none;
  }
</style>
