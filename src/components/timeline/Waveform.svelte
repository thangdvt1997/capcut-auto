<!--
  Audio waveform for one clip (master-prompt §10 "waveform"). Calls the real
  `compute_media_waveform` command (Phase 3's `audio::waveform`, exposed
  over IPC — already wired before this pass, see `src-tauri/src/commands/media.rs`)
  against the clip's source/proxy path; not synthesized/faked data.

  Bin count tracks the clip's on-screen width so the waveform stays legible
  across zoom levels without asking the backend to decode/downsample more
  detail than can actually be drawn. Results are cached per `path:bins` at
  module scope (shared across every `Waveform` instance) and the request is
  debounced 150ms behind width changes so a drag/zoom in progress doesn't
  fire one IPC call per pixel (master prompt §50).
-->
<script module lang="ts">
  import type { WaveformResult } from "../../types/bindings";
  const waveformCache = new Map<string, Promise<WaveformResult | null>>();
</script>

<script lang="ts">
  import { commands } from "../../types/bindings";
  import type { MediaItem } from "../../types/bindings";

  let { media, widthPx, heightPx }: { media: MediaItem; widthPx: number; heightPx: number } = $props();

  let peaks = $state<number[]>([]);

  $effect(() => {
    const path = media.proxy_path ?? media.source_path;
    const bins = Math.max(8, Math.min(400, Math.round(widthPx / 3)));
    const timer = setTimeout(() => void load(path, bins), 150);
    return () => clearTimeout(timer);
  });

  async function load(path: string, bins: number): Promise<void> {
    const key = `${path}:${bins}`;
    let promise = waveformCache.get(key);
    if (!promise) {
      promise = commands.computeMediaWaveform(path, bins).then((r) => (r.status === "ok" ? r.data : null));
      waveformCache.set(key, promise);
    }
    const result = await promise;
    peaks = result?.peaks ?? [];
  }
</script>

<svg class="tl-waveform" width={widthPx} height={heightPx} viewBox="0 0 {widthPx} {heightPx}" preserveAspectRatio="none">
  {#if peaks.length > 0}
    {@const barWidth = widthPx / peaks.length}
    {#each peaks as peak, i (i)}
      {@const h = Math.max(1, peak * heightPx)}
      <rect x={i * barWidth} y={(heightPx - h) / 2} width={Math.max(0.6, barWidth - 0.5)} height={h} rx="0.5" />
    {/each}
  {/if}
</svg>

<style>
  .tl-waveform {
    display: block;
    pointer-events: none;
  }
  .tl-waveform rect {
    fill: hsl(142 71% 55% / 0.6);
  }
</style>
