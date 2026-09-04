// Svelte 5 runes-based media library store (pattern informed by autocut's
// `store.svelte.ts` — a plain class using `$state` fields directly, audit
// §2 — reimplemented, not copied). `.svelte.ts` (not `.ts`) is required for
// rune syntax to work outside a `.svelte` component file.
//
// This store owns the Media Library panel's client-side state: the
// currently-known library entries (backed by the SQLite index in
// `src-tauri/src/db`, master prompt §35), the selection driving the preview
// panel, and live proxy-generation progress fed by the `media:proxy-progress`
// Tauri event (`src-tauri/src/commands/media.rs`).

import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "../types/bindings";
import type { ImportResult, MediaKind, MediaLibraryEntry, ProxyMode } from "../types/bindings";

/**
 * Payload of the `media:proxy-progress` Tauri event. Hand-written rather than
 * specta-generated: this project's `tauri-specta` `Builder` only registers
 * *commands* (`specta_builder()` in `src-tauri/src/lib.rs`), not typed
 * events — adding typed-event registration for this one small, stable event
 * wasn't worth the extra macro surface this late in Phase 3. Keep this in
 * sync with `commands::media::ProxyProgressEvent` in
 * `src-tauri/src/commands/media.rs` by hand; it's the one payload in this
 * codebase that isn't generated (autocut's whole IPC surface worked this
 * way — audit §2 risk #10 — this is a narrow, deliberate exception, not a
 * reversion to that pattern generally).
 */
export interface ProxyProgressEvent {
  media_id: string;
  fraction: number | null;
  done: boolean;
  proxy_path: string | null;
  error: string | null;
}

const PROXY_PROGRESS_EVENT = "media:proxy-progress";
const LIBRARY_LIMIT = 500;

class MediaLibraryStore {
  entries = $state<MediaLibraryEntry[]>([]);
  selectedId = $state<string | null>(null);
  importing = $state(false);
  loading = $state(false);
  lastError = $state<string | null>(null);
  searchQuery = $state("");
  kindFilter = $state<MediaKind | null>(null);
  proxyMode = $state<ProxyMode>("auto");
  /** Keyed by media id — most recent progress event seen for that job. */
  proxyProgress = $state<Record<string, ProxyProgressEvent>>({});

  selected = $derived(this.entries.find((e) => e.id === this.selectedId) ?? null);

  constructor() {
    // Fire-and-forget: a session-guard (autocut's `sessionId` pattern) isn't
    // needed here the way it is for video-load races, since every event
    // carries its own `media_id` and this store just indexes by it.
    void listen<ProxyProgressEvent>(PROXY_PROGRESS_EVENT, (event) => {
      const payload = event.payload;
      this.proxyProgress[payload.media_id] = payload;
      if (payload.done && payload.proxy_path) {
        const entry = this.entries.find((e) => e.id === payload.media_id);
        if (entry) {
          entry.proxy_path = payload.proxy_path;
        }
      }
    });
    void this.refresh();
  }

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      const result = await commands.searchMediaLibrary(
        this.searchQuery.trim().length > 0 ? this.searchQuery.trim() : null,
        this.kindFilter,
        LIBRARY_LIMIT,
      );
      if (result.status === "ok") {
        this.entries = result.data;
        this.lastError = null;
      } else {
        this.lastError = result.error.message;
      }
    } finally {
      this.loading = false;
    }
  }

  async importPaths(paths: string[]): Promise<void> {
    if (paths.length === 0) return;
    this.importing = true;
    try {
      const result = await commands.importMediaPaths(paths, this.proxyMode);
      if (result.status === "ok") {
        await this.handleImportResults(result.data);
      } else {
        this.lastError = result.error.message;
      }
    } finally {
      this.importing = false;
    }
  }

  async importFolder(folder: string): Promise<void> {
    this.importing = true;
    try {
      const result = await commands.importMediaFolder(folder, this.proxyMode);
      if (result.status === "ok") {
        await this.handleImportResults(result.data);
      } else {
        this.lastError = result.error.message;
      }
    } finally {
      this.importing = false;
    }
  }

  private async handleImportResults(results: ImportResult[]): Promise<void> {
    const failures = results.filter((r) => r.error !== null);
    if (failures.length > 0) {
      this.lastError = `${failures.length} of ${results.length} file(s) failed to import: ${failures
        .map((f) => `${f.source_path} (${f.error?.message ?? "unknown error"})`)
        .join("; ")}`;
    } else {
      this.lastError = null;
    }
    await this.refresh();
    const firstSucceeded = results.find((r) => r.media !== null);
    if (firstSucceeded?.media && this.selectedId === null) {
      this.selectedId = firstSucceeded.media.id;
    }
  }

  async remove(id: string): Promise<void> {
    await commands.removeMediaFromLibrary(id);
    if (this.selectedId === id) this.selectedId = null;
    await this.refresh();
  }

  async regenerateProxy(entry: MediaLibraryEntry): Promise<void> {
    const result = await commands.generateMediaProxy(entry.id, entry.path, this.proxyMode);
    if (result.status === "error") {
      this.lastError = result.error.message;
    }
  }

  select(id: string | null): void {
    this.selectedId = id;
  }

  /** `null` in, `null` out — lets callers pass an `Option<string>`-shaped
   * field straight through without an extra guard at every call site. */
  assetUrl(path: string | null | undefined): string | null {
    return path ? convertFileSrc(path) : null;
  }
}

export const mediaLibrary = new MediaLibraryStore();
