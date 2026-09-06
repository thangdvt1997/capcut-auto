// Svelte 5 runes-based store for the Asset Library (upgrade spec §17,
// `src-tauri/src/assets/{mod,io,error}.rs` + `src-tauri/src/commands/assets.rs`).
// Structurally mirrors `stores/modelManager.svelte.ts`'s "list a catalog,
// add via a real backend command, remove with a two-step confirm" shape,
// adapted for a small user-curated catalog (no fixed entry list, no
// download progress) instead of a fixed 5-model list.
//
// This store is the single shared source both consumers read from:
//   - `components/assets/AssetLibraryDialog.svelte` (this module's own
//     management UI: list/add/remove).
//   - `stores/templates.svelte.ts`'s intro/outro/watermark/background-music
//     pickers (via `byKinds`) — registering an asset here makes it
//     immediately selectable there without a separate fetch.
//
// `.svelte.ts` (not `.ts`) is required for `$state`/`$derived` to work
// outside a `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type { Asset, AssetKind } from "../types/bindings";

/** Upgrade spec §17's exact 11-kind catalog, in the same order the backend's
 * own `AssetKind` enum lists them (`assets::mod`). */
export const ASSET_KINDS: AssetKind[] = [
  "intro",
  "outro",
  "logo",
  "watermark",
  "music",
  "sound_effect",
  "overlay",
  "font",
  "subtitle_style",
  "transition_preset",
  "background",
];

/** Kinds that actually plug into a real `Template` field today
 * (`Template::intro`/`outro`/`watermark`/`background_music`) — mirrors
 * `assets::mod`'s own module doc comment ("which `AssetKind`s are really
 * consumed vs. structural-only") exactly, so this UI's own "used by
 * templates today" / "not consumed yet" split never drifts from the
 * backend's own documented judgment call. */
export const CONSUMED_ASSET_KINDS: ReadonlySet<AssetKind> = new Set<AssetKind>([
  "intro",
  "outro",
  "logo",
  "watermark",
  "music",
]);

/** A light file-picker filter per kind — not validated server-side (the
 * backend only checks the path is a real file, never its content, per
 * `assets::new_asset`'s own doc comment), just a convenience so the native
 * picker defaults to a sensible extension list. `null` means "no filter" for
 * kinds with no obvious single file type (e.g. a subtitle-style or
 * transition-preset file format isn't defined anywhere in this codebase
 * yet). */
function extensionsForKind(kind: AssetKind): string[] | null {
  switch (kind) {
    case "intro":
    case "outro":
      return ["mp4", "mov", "mkv", "avi", "webm", "m4v"];
    case "logo":
    case "watermark":
    case "overlay":
    case "background":
      return ["png", "jpg", "jpeg", "webp"];
    case "music":
    case "sound_effect":
      return ["mp3", "wav", "aac", "m4a", "flac"];
    default:
      return null;
  }
}

class AssetsStore {
  open = $state(false);

  assets = $state<Asset[]>([]);
  loading = $state(false);
  loadError = $state<string | null>(null);
  private loaded = false;

  // ---- Add Asset form -----------------------------------------------------

  addKind = $state<AssetKind>("intro");
  addName = $state("");
  addFilePath = $state<string | null>(null);
  adding = $state(false);
  addError = $state<string | null>(null);

  canSubmitAdd = $derived(this.addName.trim().length > 0 && this.addFilePath !== null && !this.adding);

  // ---- Remove (two-step confirm, same arm/cancel/confirm shape
  //      `TemplatesPanel.svelte`'s own custom-template delete already uses) --

  pendingRemoveId = $state<string | null>(null);
  removingId = $state<string | null>(null);
  removeError = $state<string | null>(null);

  // -------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.open = true;
    this.pendingRemoveId = null;
    void this.ensureLoaded();
  }

  close(): void {
    this.open = false;
    this.pendingRemoveId = null;
  }

  /** Lazily loads the catalog once — cheap enough to just call this from
   * every consumer (the management dialog on open, the Template editor's
   * save/edit form on open) rather than tracking per-consumer freshness. */
  async ensureLoaded(): Promise<void> {
    if (this.loaded || this.loading) return;
    await this.refresh();
  }

  async refresh(): Promise<void> {
    this.loading = true;
    this.loadError = null;
    try {
      const result = await commands.listAssets(null);
      if (result.status === "ok") {
        this.assets = result.data;
        this.loaded = true;
      } else {
        this.loadError = result.error.message;
      }
    } catch (err) {
      this.loadError = String(err);
    } finally {
      this.loading = false;
    }
  }

  /** Assets whose kind is one of `kinds` — used by
   * `stores/templates.svelte.ts`'s intro/outro/watermark/background-music
   * pickers to filter the catalog down to the relevant kind(s). */
  byKinds(kinds: AssetKind[]): Asset[] {
    return this.assets.filter((a) => kinds.includes(a.kind));
  }

  // -------------------------------------------------------------------
  // Add
  // -------------------------------------------------------------------

  resetAddForm(): void {
    this.addKind = "intro";
    this.addName = "";
    this.addFilePath = null;
    this.addError = null;
  }

  /** Real native file picker (`@tauri-apps/plugin-dialog`), the exact same
   * pattern `MediaLibrary.svelte`'s own `pickFiles` already uses — never a
   * raw text path field. */
  async pickFile(): Promise<void> {
    const extensions = extensionsForKind(this.addKind);
    const selected = await open({
      multiple: false,
      filters: extensions ? [{ name: "Asset", extensions }] : undefined,
    });
    if (selected && typeof selected === "string") {
      this.addFilePath = selected;
    }
  }

  async submitAdd(): Promise<void> {
    if (!this.canSubmitAdd || !this.addFilePath) return;
    this.adding = true;
    this.addError = null;
    try {
      const result = await commands.addAsset(this.addKind, this.addName.trim(), this.addFilePath);
      if (result.status === "ok") {
        this.resetAddForm();
        await this.refresh();
      } else {
        this.addError = result.error.message;
      }
    } catch (err) {
      this.addError = String(err);
    } finally {
      this.adding = false;
    }
  }

  // -------------------------------------------------------------------
  // Remove
  // -------------------------------------------------------------------

  armRemove(assetId: string): void {
    this.pendingRemoveId = assetId;
  }

  cancelRemove(): void {
    this.pendingRemoveId = null;
  }

  async confirmRemove(assetId: string): Promise<void> {
    if (this.pendingRemoveId !== assetId || this.removingId !== null) return;
    this.removingId = assetId;
    this.removeError = null;
    try {
      const result = await commands.removeAsset(assetId);
      if (result.status === "ok") {
        this.pendingRemoveId = null;
        await this.refresh();
      } else {
        this.removeError = result.error.message;
      }
    } catch (err) {
      this.removeError = String(err);
    } finally {
      this.removingId = null;
    }
  }
}

export const assetsStore = new AssetsStore();
