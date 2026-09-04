<!--
  Video preview panel (master-prompt §9): a real HTML5 <video> element
  served through Tauri's `asset:` protocol, not a mock. Backed directly by
  the Media Library selection for now — "preview should follow timeline
  edits" (the master prompt's own closing line for this section) is Phase 4
  scope, once a timeline/playhead exists to follow; this phase wires
  play/pause/stop/seek/frame-step/speed/volume/mute/fullscreen/canvas-ratio
  against whatever media is selected in the library.

  UNVERIFIED: this file type-checks and builds, but nothing in this build
  environment can render a webview or a display (headless Linux build
  server, no GPU/X session) — actual play/pause/seek/rendering behavior has
  not been visually confirmed. See IMPLEMENTATION_PLAN.md Phase 3.
-->
<script lang="ts">
  import { commands } from "../../types/bindings";
  import type { MediaItem, MediaLibraryEntry } from "../../types/bindings";
  import { mediaLibrary } from "../../stores/media.svelte";

  const SPEEDS = [0.25, 0.5, 1, 1.5, 2];
  const RATIOS: { id: string; label: string; value: number | null }[] = [
    { id: "16:9", label: "16:9", value: 16 / 9 },
    { id: "9:16", label: "9:16", value: 9 / 16 },
    { id: "1:1", label: "1:1", value: 1 },
    { id: "4:5", label: "4:5", value: 4 / 5 },
    { id: "custom", label: "Source", value: null },
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

  const FALLBACK_RATIO = RATIOS[RATIOS.length - 1]!;
  let selected = $derived(mediaLibrary.selected);
  let ratio = $derived(RATIOS.find((r) => r.id === ratioId) ?? FALLBACK_RATIO);
  let fps = $derived(probed !== null && probed.fps.num > 0 ? probed.fps.num / probed.fps.den : 30);
  let frameDurationSec = $derived(1 / fps);

  // Re-probe (for exact fps, used by frame-stepping) whenever the selected
  // media changes. A plain `MediaLibraryEntry` intentionally doesn't carry
  // `fps` (master prompt §35's index field list doesn't call for it) — this
  // is the one place the preview panel needs it.
  $effect(() => {
    const entry: MediaLibraryEntry | null = selected;
    probed = null;
    probeError = null;
    playing = false;
    currentTimeSec = 0;
    if (!entry) return;
    commands.probeMediaFile(entry.path).then((result) => {
      if (result.status === "ok") {
        probed = result.data;
      } else {
        probeError = result.error.message;
      }
    });
  });

  function videoSrc(entry: MediaLibraryEntry): string | null {
    // Editing uses the proxy when available (master prompt §8); final
    // render (Phase 6) always reads the original.
    return mediaLibrary.assetUrl(entry.proxy_path ?? entry.path);
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
    <div class="preview-empty muted-2">Select media from the Media Library to preview it.</div>
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
      </div>
    </div>

    {#if probeError}
      <div class="preview-error">{probeError}</div>
    {/if}

    <div class="preview-controls">
      <button class="btn btn-ghost" onclick={stop} title="Stop">⏹</button>
      <button class="btn btn-ghost" onclick={() => stepFrame(-1)} title="Frame back">⏮</button>
      <button class="btn" onclick={playing ? pause : play} title={playing ? "Pause" : "Play"}>
        {playing ? "⏸" : "▶"}
      </button>
      <button class="btn btn-ghost" onclick={() => stepFrame(1)} title="Frame forward">⏭</button>

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

      <button class="btn btn-ghost" onclick={toggleMute} title={muted ? "Unmute" : "Mute"}>
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
          <option value={r.id}>{r.label}</option>
        {/each}
      </select>

      <button class="btn btn-ghost" onclick={enterFullscreen} title="Fullscreen">⛶</button>
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
