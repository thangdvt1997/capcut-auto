// Svelte 5 runes-based store for the Phase D13 Status Bar's real, live
// CPU%/RAM% reading (`STUDIO_PLAN.md` "Dashboard Header + Status Bar",
// promt.md §14 "STATUS BAR"). Backed by the new `get_live_system_stats`
// Tauri command (`src-tauri/src/commands/diagnostics.rs::LiveSystemStats`) —
// see that command's own module doc comment for the full "why a persistent
// backend `System` + frontend polling, not a backend-pushed event" writeup.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.
//
// Mirrors `WorkerPoolWidget.svelte`'s own polling contract closely: this
// store only exposes `stats`/`error` + a plain `refresh()` method, with no
// timer of its own (a plain store class has no lifecycle to hook a
// `setInterval` into — see `stores/autoZoom.svelte.ts`'s own doc comment for
// this exact precedent) — `StatusBar.svelte` (the one real mount point for
// this store, a permanent shell row per the task brief) owns the actual
// interval, starting it on mount and clearing it on unmount.

import { commands } from "../types/bindings";
import type { LiveSystemStats } from "../types/bindings";

class LiveSystemStatsStore {
  /** `null` until the first successful fetch — never fabricated as zeroes
   * (same "null means not yet known" discipline as `batchStore.workerPoolStatus`). */
  stats = $state<LiveSystemStats | null>(null);
  error = $state<string | null>(null);

  /** `get_live_system_stats` is not a `Result` on the Rust side (nothing
   * fallible once the managed `System` state exists — see that command's own
   * doc comment), so only a real IPC/transport failure throws here. */
  async refresh(): Promise<void> {
    try {
      this.stats = await commands.getLiveSystemStats();
      this.error = null;
    } catch (err) {
      this.error = String(err);
    }
  }
}

export const liveSystemStatsStore = new LiveSystemStatsStore();
