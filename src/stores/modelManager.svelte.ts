// Svelte 5 runes-based store for the Model Manager (master prompt §14/§60,
// Phase 7's Model Manager *UI* half — the backend command surface
// (`src-tauri/src/commands/transcription.rs` + `transcription/models.rs`)
// already shipped in an earlier Phase 7 pass). Structurally mirrors
// `stores/render.svelte.ts` (the freshest "list options, kick off a
// background job, listen for a named progress event, allow cancel" store in
// this codebase): list catalog entries -> Download kicks off a
// fire-and-forget backend job -> progress arrives via a named Tauri event,
// keyed by `model_id` (not assumed singular — five models can in principle
// download concurrently) -> Cancel while downloading, Delete (with
// confirmation) once installed.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { listen } from "@tauri-apps/api/event";
import { commands } from "../types/bindings";
import type { AvailableModel, InstalledModel, ModelCatalogEntry, ModelId } from "../types/bindings";

/**
 * Payload of the `models:download-progress` Tauri event
 * (`src-tauri/src/commands/transcription.rs::ModelDownloadProgressEvent`).
 * Hand-written rather than specta-generated — this struct is only ever
 * `emit()`-ted, never returned from a `#[tauri::command]`, so
 * `tauri-specta`'s `Builder` (which only registers commands) never sees it
 * and it doesn't appear in `bindings.ts`. Same precedent as
 * `stores/render.svelte.ts`'s `RenderProgressEvent` / `stores/media.svelte.ts`'s
 * `ProxyProgressEvent` — keep this in sync with the Rust struct by hand.
 */
export interface ModelDownloadProgressEvent {
  model_id: string;
  filename: string;
  size: number;
  downloaded: number;
  speed_bytes_per_sec: number;
  eta_secs: number | null;
  done: boolean;
  error: string | null;
}

const MODEL_DOWNLOAD_PROGRESS_EVENT = "models:download-progress";

/** Merged view of one catalog entry for the dialog: static metadata +
 * install state + live download progress (if any) + this dialog's own
 * two-step delete-confirmation state. Assembled by `modelsView` below rather
 * than stored directly, so it's always derived fresh from the three
 * independent state sources instead of risking them drifting out of sync. */
export interface ModelView {
  entry: ModelCatalogEntry;
  installed: boolean;
  /** Real on-disk size once installed (`InstalledModel::size_bytes`) — falls
   * back to the catalog's `approx_size_bytes` estimate before that's known. */
  installedSizeBytes: number | null;
  downloading: boolean;
  progress: ModelDownloadProgressEvent | null;
  pendingDelete: boolean;
}

class ModelManagerStore {
  open = $state(false);

  available = $state<AvailableModel[]>([]);
  installed = $state<InstalledModel[]>([]);
  loading = $state(false);
  loadError = $state<string | null>(null);

  /** Keyed by `model_id`, not just "the current download" — several models
   * could in principle download at once, and a stale event for one model
   * must never clobber another's progress. */
  progressByModel = $state<Record<string, ModelDownloadProgressEvent>>({});
  startErrorByModel = $state<Record<string, string | null>>({});
  cancellingByModel = $state<Record<string, boolean>>({});
  deletingByModel = $state<Record<string, boolean>>({});

  /** Two-step delete confirmation (task brief: "deleting a multi-hundred-MB
   * model shouldn't be a single accidental click") — no existing
   * confirmation precedent elsewhere in this codebase (`MediaLibrary`'s
   * remove-from-library and `SilenceDetector`'s destructive actions both
   * apply immediately on a single click), so this is a fresh, simple
   * "click Delete once to arm it, click the now-relabeled button again
   * within the same dialog session to actually delete" pattern rather than
   * a native `confirm()` popup (keeps it in-dialog and themeable). Only one
   * model can be armed at a time; opening/closing the dialog or starting a
   * different model's delete clears it. */
  pendingDeleteId = $state<string | null>(null);

  constructor() {
    // Fire-and-forget, matching `stores/render.svelte.ts`'s
    // `RenderProgressEvent` listener pattern exactly.
    void listen<ModelDownloadProgressEvent>(MODEL_DOWNLOAD_PROGRESS_EVENT, (event) => {
      this.progressByModel[event.payload.model_id] = event.payload;
      // A `done` event (success or error) means the backend's job map no
      // longer holds this model_id — proactively refresh `available` so
      // `download_in_progress`/`installed` reflect that without waiting for
      // the user to close/reopen the dialog.
      if (event.payload.done) {
        void this.refresh();
      }
    });
  }

  // -------------------------------------------------------------------
  // Derived
  // -------------------------------------------------------------------

  modelsView = $derived.by((): ModelView[] => {
    return this.available.map((a): ModelView => {
      const progress = this.progressByModel[a.entry.id] ?? null;
      const installedRow = this.installed.find((i) => i.id === a.entry.id) ?? null;
      return {
        entry: a.entry,
        installed: a.installed,
        installedSizeBytes: installedRow?.size_bytes ?? (a.installed ? a.entry.approx_size_bytes : null),
        // A download is "in progress" per the live progress event (most
        // current) if one has arrived yet, else per the backend's own
        // `download_in_progress` flag from the last `refresh()` (covers the
        // window right after opening the dialog, before any progress event
        // for an already-running job has arrived).
        downloading: progress ? !progress.done : a.download_in_progress,
        progress,
        pendingDelete: this.pendingDeleteId === a.entry.id,
      };
    });
  });

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  /** Opens the dialog; lazily (re)loads the catalog on every open, matching
   * `renderStore.openDialog()`'s "lazily on first relevant interaction"
   * precedent — cheap enough here to just always refresh rather than cache
   * across opens, since install state can change from outside this dialog
   * (e.g. a future first-run wizard also downloading a model). */
  openDialog(): void {
    this.open = true;
    this.pendingDeleteId = null;
    void this.refresh();
  }

  close(): void {
    this.open = false;
    this.pendingDeleteId = null;
  }

  async refresh(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.loadError = null;
    try {
      const [availableResult, installedResult] = await Promise.all([
        commands.listAvailableModels(),
        commands.listInstalledModels(),
      ]);
      if (availableResult.status === "ok") {
        this.available = availableResult.data;
      } else {
        this.loadError = availableResult.error.message;
      }
      if (installedResult.status === "ok") {
        this.installed = installedResult.data;
      } else if (!this.loadError) {
        this.loadError = installedResult.error.message;
      }
    } finally {
      this.loading = false;
    }
  }

  // -------------------------------------------------------------------
  // Download / cancel (master prompt §60 — real resumable download +
  // progress, no client-side simulation; `download_model` is documented as
  // safe to call again on an already-in-progress download, so this never
  // needs to guard against a double-click the way `render`'s start does)
  // -------------------------------------------------------------------

  async download(id: ModelId): Promise<void> {
    this.startErrorByModel[id] = null;
    // Seed an initial "starting" progress row immediately so the UI shows a
    // progress bar right away rather than waiting for the first real event
    // (which, for a large model, can be a second or two after the request).
    const entry = this.available.find((a) => a.entry.id === id)?.entry;
    if (entry && !this.progressByModel[id]) {
      this.progressByModel[id] = {
        model_id: id,
        filename: entry.filename,
        size: entry.approx_size_bytes,
        downloaded: 0,
        speed_bytes_per_sec: 0,
        eta_secs: null,
        done: false,
        error: null,
      };
    }
    const result = await commands.downloadModel(id);
    if (result.status === "error") {
      this.startErrorByModel[id] = result.error.message;
      // Roll back the optimistic seed row if the request itself failed
      // (e.g. an unknown model id) — a real progress event never arrived to
      // supersede it.
      delete this.progressByModel[id];
    }
  }

  async cancelDownload(id: ModelId): Promise<void> {
    if (this.cancellingByModel[id]) return;
    this.cancellingByModel[id] = true;
    try {
      const result = await commands.cancelModelDownload(id);
      if (result.status === "error") {
        this.startErrorByModel[id] = result.error.message;
      }
    } finally {
      this.cancellingByModel[id] = false;
    }
  }

  /** Clears a finished (done, success or error) download's progress row so
   * the model's card goes back to showing a plain Download button. */
  dismissProgress(id: ModelId): void {
    delete this.progressByModel[id];
    delete this.startErrorByModel[id];
  }

  // -------------------------------------------------------------------
  // Delete (two-step confirmation — see `pendingDeleteId` doc comment)
  // -------------------------------------------------------------------

  requestDelete(id: ModelId): void {
    this.pendingDeleteId = id;
  }

  cancelDeleteRequest(): void {
    this.pendingDeleteId = null;
  }

  async confirmDelete(id: ModelId): Promise<void> {
    if (this.deletingByModel[id]) return;
    this.deletingByModel[id] = true;
    try {
      const result = await commands.deleteModel(id);
      if (result.status === "error") {
        this.startErrorByModel[id] = result.error.message;
      } else {
        this.pendingDeleteId = null;
        await this.refresh();
      }
    } finally {
      this.deletingByModel[id] = false;
    }
  }
}

export const modelManagerStore = new ModelManagerStore();

/**
 * Convenience entry point for other components to open the Model Manager
 * dialog without importing the store class shape — e.g. the Transcript
 * Editor (built concurrently in this same phase, in a different pass) can
 * call this from a "no model installed — open Model Manager" prompt without
 * needing to know anything else about this store.
 */
export function openModelManager(): void {
  modelManagerStore.openDialog();
}
