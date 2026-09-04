<!--
  Media Library panel (master-prompt §7/§35): file picker, folder picker,
  and native drag & drop import, backed by the SQLite media index
  (`src-tauri/src/db`). Replaces the Phase 2 "Media — Phase 3" placeholder
  in LeftPanel's Media tab.

  Not built here (later phases): AI-generated tags, semantic search beyond
  filename/tag substring matching — see `mediaLibrary.svelte.ts`'s doc
  comment on `search_media_library`.
-->
<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount } from "svelte";
  import { mediaLibrary } from "../../stores/media.svelte";
  import type { MediaKind, MediaLibraryEntry } from "../../types/bindings";

  const SUPPORTED_EXTENSIONS = [
    "mp4", "mov", "mkv", "avi", "webm", "m4v",
    "mp3", "wav", "aac", "m4a", "flac",
    "png", "jpg", "jpeg", "webp",
  ];

  let dragActive = $state(false);

  onMount(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop") {
        dragActive = false;
        void mediaLibrary.importPaths(event.payload.paths);
      } else if (event.payload.type === "enter" || event.payload.type === "over") {
        dragActive = true;
      } else {
        dragActive = false;
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  });

  async function pickFiles() {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Media", extensions: SUPPORTED_EXTENSIONS }],
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      await mediaLibrary.importPaths(paths);
    }
  }

  async function pickFolder() {
    const selected = await open({ directory: true });
    if (selected && typeof selected === "string") {
      await mediaLibrary.importFolder(selected);
    }
  }

  function formatDuration(us: number): string {
    if (us <= 0) return "--:--";
    const totalSeconds = Math.round(us / 1_000_000);
    const m = Math.floor(totalSeconds / 60);
    const s = totalSeconds % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function kindIcon(kind: MediaKind): string {
    return kind === "video" ? "▶" : kind === "audio" ? "♫" : "▦";
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  function onSearchInput() {
    // Debounce (master prompt §50) — every keystroke would otherwise issue
    // its own IPC round trip + SQLite query.
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void mediaLibrary.refresh(), 200);
  }

  function proxyLabel(entry: MediaLibraryEntry): string | null {
    const progress = mediaLibrary.proxyProgress[entry.id];
    if (progress && !progress.done) {
      const pct = progress.fraction !== null ? `${Math.round(progress.fraction * 100)}%` : "…";
      return `Proxy ${pct}`;
    }
    if (progress?.error) return "Proxy failed";
    if (entry.proxy_path) return "Proxy ready";
    return null;
  }
</script>

<div class="media-library" class:drag-active={dragActive}>
  <div class="ml-toolbar">
    <button class="btn" onclick={pickFiles}>Import Files</button>
    <button class="btn" onclick={pickFolder}>Import Folder</button>
    <select class="ml-select" bind:value={mediaLibrary.proxyMode} title="Proxy media mode (master prompt §8)">
      <option value="off">Proxy: Off</option>
      <option value="auto">Proxy: Auto</option>
      <option value="always">Proxy: Always</option>
    </select>
  </div>

  <div class="ml-search">
    <input
      class="ml-search-input"
      type="text"
      placeholder="Search filename or tag…"
      bind:value={mediaLibrary.searchQuery}
      oninput={onSearchInput}
    />
    <select
      class="ml-select"
      bind:value={mediaLibrary.kindFilter}
      onchange={() => void mediaLibrary.refresh()}
    >
      <option value={null}>All</option>
      <option value="video">Video</option>
      <option value="audio">Audio</option>
      <option value="image">Image</option>
    </select>
  </div>

  {#if mediaLibrary.lastError}
    <div class="ml-error">{mediaLibrary.lastError}</div>
  {/if}
  {#if mediaLibrary.importing}
    <div class="ml-status muted-2">Importing…</div>
  {/if}

  <div class="ml-grid">
    {#if mediaLibrary.entries.length === 0 && !mediaLibrary.loading}
      <div class="ml-empty muted-2">
        No media yet. Drag & drop files here, or use Import Files / Import Folder.
        <br />Supported: MP4/MOV/MKV/AVI/WEBM/M4V, MP3/WAV/AAC/M4A/FLAC, PNG/JPG/JPEG/WEBP.
      </div>
    {/if}
    {#each mediaLibrary.entries as entry (entry.id)}
      <button
        class="ml-card"
        class:selected={entry.id === mediaLibrary.selectedId}
        onclick={() => mediaLibrary.select(entry.id)}
        ondblclick={() => void mediaLibrary.regenerateProxy(entry)}
        title={entry.path}
      >
        <div class="ml-thumb">
          {#if entry.thumbnail_path}
            <img src={mediaLibrary.assetUrl(entry.thumbnail_path)} alt={entry.filename} loading="lazy" />
          {:else}
            <span class="ml-thumb-fallback">{kindIcon(entry.kind)}</span>
          {/if}
          {#if entry.kind !== "image"}
            <span class="ml-duration mono">{formatDuration(entry.duration_us)}</span>
          {/if}
        </div>
        <div class="ml-meta">
          <span class="ml-filename">{entry.filename}</span>
          {#if entry.width > 0 && entry.height > 0}
            <span class="ml-res muted-2 mono">{entry.width}×{entry.height}</span>
          {/if}
          {#if proxyLabel(entry)}
            <span class="ml-proxy muted-2">{proxyLabel(entry)}</span>
          {/if}
        </div>
        <span
          class="ml-remove"
          role="button"
          tabindex="0"
          onclick={(e) => {
            e.stopPropagation();
            void mediaLibrary.remove(entry.id);
          }}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.stopPropagation();
              void mediaLibrary.remove(entry.id);
            }
          }}
          title="Remove from library"
        >
          ×
        </span>
      </button>
    {/each}
  </div>

  {#if dragActive}
    <div class="ml-drop-overlay">Drop to import</div>
  {/if}
</div>

<style>
  .media-library {
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
    min-height: 0;
  }
  .ml-toolbar {
    display: flex;
    gap: 6px;
    padding: 8px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ml-search {
    display: flex;
    gap: 6px;
    padding: 8px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ml-search-input {
    flex: 1;
    height: 26px;
    padding: 0 8px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
  }
  .ml-select {
    height: 26px;
    background: var(--input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--foreground);
    font-size: 11px;
  }
  .ml-error {
    margin: 0 8px;
    padding: 6px 8px;
    font-size: 11px;
    color: var(--neg);
    background: hsl(0 84% 65% / 0.08);
    border: 1px solid hsl(0 84% 65% / 0.3);
    border-radius: var(--radius-sm);
  }
  .ml-status {
    padding: 4px 8px;
    font-size: 11px;
  }
  .ml-grid {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: 8px;
    padding: 8px;
    align-content: start;
  }
  .ml-empty {
    grid-column: 1 / -1;
    text-align: center;
    padding: 24px 8px;
    font-size: 11.5px;
    line-height: 1.6;
  }
  .ml-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
    text-align: left;
    position: relative;
    color: inherit;
    font: inherit;
  }
  .ml-card:hover { border-color: var(--border-strong); }
  .ml-card.selected { border-color: var(--accent); background: hsl(213 94% 68% / 0.08); }
  .ml-thumb {
    position: relative;
    aspect-ratio: 16 / 9;
    background: var(--elevated);
    border-radius: var(--radius-sm);
    overflow: hidden;
    display: grid;
    place-items: center;
  }
  .ml-thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ml-thumb-fallback { font-size: 20px; color: var(--muted-2); }
  .ml-duration {
    position: absolute;
    right: 3px;
    bottom: 3px;
    font-size: 10px;
    padding: 1px 4px;
    background: hsl(0 0% 0% / 0.65);
    color: hsl(0 0% 100%);
    border-radius: 3px;
  }
  .ml-meta { display: flex; flex-direction: column; gap: 1px; }
  .ml-filename {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ml-res, .ml-proxy { font-size: 10px; }
  .ml-remove {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: hsl(0 0% 0% / 0.5);
    color: hsl(0 0% 100%);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .ml-remove:hover { background: var(--neg); }
  .ml-drop-overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: hsl(213 94% 68% / 0.15);
    border: 2px dashed var(--accent);
    font-size: 13px;
    font-weight: 600;
    pointer-events: none;
  }
  .media-library.drag-active { outline: 2px dashed var(--accent); outline-offset: -2px; }
</style>
