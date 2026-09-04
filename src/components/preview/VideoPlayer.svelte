<!--
  Video preview panel (master-prompt §9): a real HTML5 <video> element
  served through Tauri's `asset:` protocol, not a mock.

  Phase 3 backed this directly by the Media Library selection; Phase 4 adds
  "preview follows timeline edits" (this section's own closing line, and
  IMPLEMENTATION_PLAN.md Phase 4's brief): when the timeline has a project
  loaded and its playhead sits over a clip on a non-hidden, non-locked video
  track, that clip's source media takes priority over the Media Library
  selection, and moving the playhead seeks the underlying <video> element to
  the corresponding source time. This is **single-clip scrubbing only** —
  real multi-track *compositing* at the playhead needs the render engine
  (Phase 6 `RenderGraph`) and is not attempted here. With no project loaded
  (or no clip under the playhead), this falls back to the Phase 3 behavior
  of following the Media Library selection.

  UNVERIFIED: this file type-checks and builds, but nothing in this build
  environment can render a webview or a display (headless Linux build
  server, no GPU/X session) — actual play/pause/seek/rendering behavior has
  not been visually confirmed. See IMPLEMENTATION_PLAN.md Phase 3.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { commands } from "../../types/bindings";
  import type { MediaItem, MediaLibraryEntry } from "../../types/bindings";
  import { mediaLibrary } from "../../stores/media.svelte";
  import { timeline } from "../../stores/timeline.svelte";
  import { usToSec } from "../../timeline/algebra";
  import { t } from "../../lib/i18n.svelte";
  import CaptionOverlay from "./CaptionOverlay.svelte";

  const SPEEDS = [0.25, 0.5, 1, 1.5, 2];
  // Ratio labels are numeric aspect ratios (16:9, 9:16, …) except "custom",
  // whose display label comes from the locale-driven `ratioLabel()` below
  // (t("videoPlayer.ratioSource")) instead of this static table.
  const RATIOS: { id: string; label: string; value: number | null }[] = [
    { id: "16:9", label: "16:9", value: 16 / 9 },
    { id: "9:16", label: "9:16", value: 9 / 16 },
    { id: "1:1", label: "1:1", value: 1 },
    { id: "4:5", label: "4:5", value: 4 / 5 },
    { id: "custom", label: "", value: null },
  ];

  let videoEl: HTMLVideoElement | undefined = $state();
  let containerEl: HTMLDivElement | undefined = $state();
  let probed = $state<MediaItem | null>(null);
  let playing = $state(false);
  let currentTimeSec = $state(0);
  let durationSec = $state(0);
  let speed = $state(1);
  let volume = $state(1);
  let muted = $state(false);
  let ratioId = $state("custom");
  let probeError = $state<string | null>(null);

  /** The subset of fields the preview actually needs, shared by both
   * sources it can be driven from — see `selected` below. */
  interface PreviewSource {
    id: string;
    kind: "video" | "audio" | "image";
    filename: string;
    path: string;
    proxy_path: string | null;
    width: number;
    height: number;
  }

  function filenameFromPath(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function fromMediaLibraryEntry(entry: MediaLibraryEntry): PreviewSource {
    return {
      id: entry.id,
      kind: entry.kind,
      filename: entry.filename,
      path: entry.path,
      proxy_path: entry.proxy_path,
      width: entry.width,
      height: entry.height,
    };
  }

  function fromTimelineMedia(media: MediaItem): PreviewSource {
    return {
      id: media.id,
      kind: media.kind,
      filename: filenameFromPath(media.source_path),
      path: media.source_path,
      proxy_path: media.proxy_path,
      width: media.width,
      height: media.height,
    };
  }

  const FALLBACK_RATIO = RATIOS[RATIOS.length - 1]!;
  // Preview-follows-timeline (see file doc comment): the timeline's active
  // clip-under-playhead wins over the Media Library selection whenever one
  // exists.
  let timelineTarget = $derived(timeline.activeVideoTarget);
  let selected = $derived<PreviewSource | null>(
    timelineTarget ? fromTimelineMedia(timelineTarget.media) : mediaLibrary.selected ? fromMediaLibraryEntry(mediaLibrary.selected) : null,
  );
  let ratio = $derived(RATIOS.find((r) => r.id === ratioId) ?? FALLBACK_RATIO);
  let fps = $derived(probed !== null && probed.fps.num > 0 ? probed.fps.num / probed.fps.den : 30);
  let frameDurationSec = $derived(1 / fps);
  // A stable primitive to key the "media identity changed" effect below on.
  // `selected` itself is a freshly-constructed object every time
  // `timelineTarget` recomputes (i.e. on every playhead tick, since
  // `activeVideoTarget` is a new `{ media, sourceTimeUs }` each time) even
  // when the underlying media hasn't changed — depending on the *object*
  // would re-run the probe/reset effect on every scrub, not just on an
  // actual source change.
  let selectedId = $derived(selected?.id ?? null);

  function seekToSourceUs(us: number): void {
    if (!videoEl) return;
    const sec = Math.max(0, usToSec(us));
    videoEl.currentTime = durationSec > 0 ? Math.min(sec, durationSec) : sec;
  }

  // Re-probe (for exact fps, used by frame-stepping) whenever the active
  // *media identity* changes — not on every playhead tick. Reads beyond
  // `selectedId` go through `untrack` so they don't themselves become
  // dependencies (see `selectedId`'s doc comment). A plain
  // `MediaLibraryEntry` intentionally doesn't carry `fps` (master prompt
  // §35's index field list doesn't call for it) — this is the one place the
  // preview panel needs it.
  $effect(() => {
    const id = selectedId;
    probed = null;
    probeError = null;
    playing = false;
    currentTimeSec = 0;
    if (!id) return;
    const source = untrack(() => selected);
    if (!source) return;
    commands.probeMediaFile(source.path).then((result) => {
      if (result.status === "ok") {
        probed = result.data;
        // If the timeline (not the library) drove this source change, land
        // on the clip's current source time immediately rather than 0.
        const target = untrack(() => timelineTarget);
        if (target) seekToSourceUs(target.sourceTimeUs);
      } else {
        probeError = result.error.message;
      }
    });
  });

  // Playhead-driven scrubbing within the *same* source media: only fires
  // once metadata is loaded (durationSec > 0) so it doesn't race the
  // source-switch effect above when both the media id and the playhead
  // change together (e.g. clicking a different clip's span).
  $effect(() => {
    const target = timelineTarget;
    if (!target || durationSec <= 0) return;
    seekToSourceUs(target.sourceTimeUs);
  });

  onMount(() => {
    timeline.previewApi.togglePlayPause = () => (playing ? pause() : play());
    return () => {
      if (timeline.previewApi.togglePlayPause) timeline.previewApi.togglePlayPause = undefined;
    };
  });

  function videoSrc(source: PreviewSource): string | null {
    // Editing uses the proxy when available (master prompt §8); final
    // render (Phase 6) always reads the original. `convertFileSrc` directly
    // (not `mediaLibrary.assetUrl`) since `source` may be timeline-driven,
    // not a Media Library entry.
    const path = source.proxy_path ?? source.path;
    return path ? convertFileSrc(path) : null;
  }

  function play() {
    void videoEl?.play();
  }
  function pause() {
    videoEl?.pause();
  }
  function stop() {
    if (!videoEl) return;
    videoEl.pause();
    videoEl.currentTime = 0;
  }
  function seek(sec: number) {
    if (!videoEl) return;
    videoEl.currentTime = Math.max(0, Math.min(durationSec, sec));
  }
  function stepFrame(direction: 1 | -1) {
    if (!videoEl) return;
    videoEl.pause();
    seek(videoEl.currentTime + direction * frameDurationSec);
  }
  function setSpeed(next: number) {
    speed = next;
    if (videoEl) videoEl.playbackRate = next;
  }
  function toggleMute() {
    muted = !muted;
    if (videoEl) videoEl.muted = muted;
  }
  function onVolumeInput(value: number) {
    volume = value;
    if (videoEl) videoEl.volume = value;
    if (value > 0 && muted) toggleMute();
  }
  function enterFullscreen() {
    void containerEl?.requestFullscreen?.();
  }
  function ratioLabel(r: { id: string; label: string }): string {
    return r.id === "custom" ? t("videoPlayer.ratioSource") : r.label;
  }
  function formatTime(sec: number): string {
    if (!Number.isFinite(sec) || sec < 0) sec = 0;
    const m = Math.floor(sec / 60);
    const s = Math.floor(sec % 60);
    const ms = Math.floor((sec - Math.floor(sec)) * 100);
    return `${m}:${s.toString().padStart(2, "0")}.${ms.toString().padStart(2, "0")}`;
  }
</script>

<div class="preview">
  {#if !selected}
    <div class="preview-empty muted-2">{t("videoPlayer.selectMediaPrompt")}</div>
  {:else}
    {@const src = videoSrc(selected)}
    <div class="preview-stage">
      <div
        class="canvas-frame"
        bind:this={containerEl}
        style:aspect-ratio={ratio.value ? `${ratio.value}` : selected.width && selected.height ? `${selected.width}/${selected.height}` : "16/9"}
      >
        {#if selected.kind === "video" && src}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            bind:this={videoEl}
            {src}
            onloadedmetadata={() => {
              if (videoEl) durationSec = videoEl.duration || 0;
            }}
            ontimeupdate={() => {
              if (videoEl) currentTimeSec = videoEl.currentTime;
            }}
            onplay={() => (playing = true)}
            onpause={() => (playing = false)}
            onended={() => (playing = false)}
          ></video>
        {:else if selected.kind === "image" && src}
          <img {src} alt={selected.filename} />
        {:else if selected.kind === "audio"}
          <div class="audio-placeholder">♫ {selected.filename}</div>
        {/if}
        <!-- Karaoke/active-word caption overlay (master prompt §27) — sits
             on top of whatever preview content this frame currently shows,
             driven by the shared timeline playhead, not this panel's own
             media selection (see CaptionOverlay.svelte doc comment). -->
        <CaptionOverlay />
      </div>
    </div>

    {#if probeError}
      <div class="preview-error">{probeError}</div>
    {/if}

    <div class="preview-controls">
      <button class="btn btn-ghost" onclick={stop} title={t("videoPlayer.stop")}>⏹</button>
      <button class="btn btn-ghost" onclick={() => stepFrame(-1)} title={t("videoPlayer.frameBack")}>⏮</button>
      <button class="btn" onclick={playing ? pause : play} title={playing ? t("videoPlayer.pause") : t("videoPlayer.play")}>
        {playing ? "⏸" : "▶"}
      </button>
      <button class="btn btn-ghost" onclick={() => stepFrame(1)} title={t("videoPlayer.frameForward")}>⏭</button>

      <input
        class="seek"
        type="range"
        min="0"
        max={durationSec || 0}
        step="0.01"
        value={currentTimeSec}
        oninput={(e) => seek(Number(e.currentTarget.value))}
      />
      <span class="time mono muted-2">{formatTime(currentTimeSec)} / {formatTime(durationSec)}</span>

      <select class="ml-select" value={speed} onchange={(e) => setSpeed(Number(e.currentTarget.value))}>
        {#each SPEEDS as s (s)}
          <option value={s}>{s}x</option>
        {/each}
      </select>

      <button class="btn btn-ghost" onclick={toggleMute} title={muted ? t("videoPlayer.unmute") : t("videoPlayer.mute")}>
        {muted || volume === 0 ? "🔇" : "🔊"}
      </button>
      <input
        class="volume"
        type="range"
        min="0"
        max="1"
        step="0.01"
        value={volume}
        oninput={(e) => onVolumeInput(Number(e.currentTarget.value))}
      />

      <select class="ml-select" bind:value={ratioId}>
        {#each RATIOS as r (r.id)}
          <option value={r.id}>{ratioLabel(r)}</option>
        {/each}
      </select>

      <button class="btn btn-ghost" onclick={enterFullscreen} title={t("videoPlayer.fullscreen")}>⛶</button>
    </div>
  {/if}
</div>

<style>
  .preview {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--background);
  }
  .preview-empty {
    flex: 1;
    display: grid;
    place-items: center;
    font-size: 12px;
    text-align: center;
    padding: 24px;
  }
  .preview-stage {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    padding: 12px;
    overflow: hidden;
  }
  .canvas-frame {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    height: 100%;
    background: black;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: var(--radius-sm);
  }
  .canvas-frame video, .canvas-frame img {
    max-width: 100%;
    max-height: 100%;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .audio-placeholder {
    color: var(--muted);
    font-size: 13px;
    padding: 24px;
  }
  .preview-error {
    padding: 4px 10px;
    font-size: 11px;
    color: var(--neg);
  }
  .preview-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .seek { flex: 1; min-width: 100px; }
  .volume { width: 70px; }
  .time { font-size: 11px; white-space: nowrap; }
  .ml-select {
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
  }
</style>
