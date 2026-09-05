// Svelte 5 runes-based store for Phase 12's auto-update settings (master
// prompt §62): the three `UpdateCheckMode` options (Automatically check /
// Notify only / Disabled) plus a real "Check for Updates Now" action against
// `commands::checkForUpdate`, and a real "Install & Restart" action against
// `commands::installAvailableUpdate` once an update is actually available.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.
//
// ## Where `mode` is persisted, and why
//
// Same as every other non-project, app-level setting in this codebase
// (`stores/aiSettings.svelte.ts`/`stores/capcut.svelte.ts`'s own doc
// comments): there is no backend settings-persistence surface yet, so this
// is `localStorage`-only, keyed `ave:update:mode`. The backend's own
// `UpdateCheckMode` enum (`src-tauri/src/update/mod.rs`) exists purely so
// this value still round-trips through `checkForUpdate`/
// `installAvailableUpdate` with real specta-typed correctness, and so the
// backend can independently refuse to check/install when `"disabled"` is
// selected even if this store ever got its own gating wrong — this store
// still mirrors that gating locally (see `checkNow`/`installNow` below) so
// the UI never even attempts a network round trip while disabled.
//
// ## What each mode actually does (documented here — the master prompt only
// names the three options, not their exact behavior)
//
// - `"automatically_check"`: this store calls `checkNow()` once, right when
//   the app starts (this module's own construction), in addition to the
//   manual button.
// - `"notify_only"`: never checks on its own — only the manual "Check for
//   Updates Now" button checks, and only ever *notifies* via the status
//   display.
// - `"disabled"`: no automatic or manual check ever reaches the backend;
//   `checkNow()` short-circuits to a local `{status: "disabled"}` outcome.
//
// No mode ever auto-installs without the user explicitly clicking "Install &
// Restart" — and that action itself only ever appears once `checkNow()`
// reports `"available"` (never while `"deferred"` — a render/batch job is
// running, master prompt §62: "Never update while rendering").

import { commands } from "../types/bindings";
import type { UpdateCheckMode, UpdateCheckOutcome } from "../types/bindings";

export const UPDATE_CHECK_MODES: readonly UpdateCheckMode[] = [
  "automatically_check",
  "notify_only",
  "disabled",
];

const MODE_STORAGE_KEY = "ave:update:mode";

function isUpdateCheckMode(value: unknown): value is UpdateCheckMode {
  return (UPDATE_CHECK_MODES as readonly string[]).includes(value as string);
}

function loadMode(): UpdateCheckMode {
  try {
    const raw = localStorage.getItem(MODE_STORAGE_KEY);
    return isUpdateCheckMode(raw) ? raw : "automatically_check";
  } catch {
    // localStorage may be unavailable (private browsing, disabled storage).
    return "automatically_check";
  }
}

function saveMode(mode: UpdateCheckMode): void {
  try {
    localStorage.setItem(MODE_STORAGE_KEY, mode);
  } catch {
    /* storage may be disabled — mode simply won't survive a restart */
  }
}

class UpdateSettingsStore {
  open = $state(false);

  mode = $state<UpdateCheckMode>(loadMode());

  checking = $state(false);
  installing = $state(false);
  lastOutcome = $state<UpdateCheckOutcome | null>(null);
  lastError = $state<string | null>(null);

  constructor() {
    // "Automatically check" (module doc comment): one real check on app
    // startup, independent of whether this dialog is ever opened.
    if (this.mode === "automatically_check") {
      void this.checkNow();
    }
  }

  openDialog(): void {
    this.open = true;
  }

  close(): void {
    this.open = false;
  }

  setMode(next: UpdateCheckMode): void {
    this.mode = next;
    saveMode(next);
    if (next === "disabled") {
      // Nothing left to show once checking is turned off — matches the
      // backend's own `UpdateCheckOutcome::Disabled` for a check that never
      // ran, rather than leaving a stale "available"/"up to date" result
      // on screen after the user just disabled checking.
      this.lastOutcome = { status: "disabled" };
      this.lastError = null;
    }
  }

  async checkNow(): Promise<void> {
    if (this.checking || this.mode === "disabled") {
      if (this.mode === "disabled") {
        this.lastOutcome = { status: "disabled" };
      }
      return;
    }
    this.checking = true;
    this.lastError = null;
    try {
      const result = await commands.checkForUpdate(this.mode);
      if (result.status === "ok") {
        this.lastOutcome = result.data;
      } else {
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.checking = false;
    }
  }

  /** Only meaningful once `lastOutcome.status === "available"` — the
   * dialog only renders this action in that state, but this store's own
   * guard here is the same defense-in-depth precedent as `checkNow()`'s
   * disabled-mode guard above. */
  async installNow(): Promise<void> {
    if (this.installing || this.mode === "disabled" || this.lastOutcome?.status !== "available") {
      return;
    }
    this.installing = true;
    this.lastError = null;
    try {
      const result = await commands.installAvailableUpdate(this.mode);
      if (result.status === "ok") {
        this.lastOutcome = result.data;
      } else {
        this.lastError = result.error.message;
      }
    } catch (err) {
      this.lastError = String(err);
    } finally {
      this.installing = false;
    }
  }
}

export const updateSettingsStore = new UpdateSettingsStore();
