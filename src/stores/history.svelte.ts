// Svelte 5 runes-based store for the Video Processing History UI (upgrade
// spec §21, `UPGRADE_PLAN.md` Phase U3 — the History backend, already real,
// shipped in an earlier pass: `src-tauri/src/history/`, `commands/history.rs`).
//
// Structurally mirrors `stores/batch.svelte.ts`: a real, paginated
// (`list_history`'s own `LIMIT`/`OFFSET`, same "load more" shape
// `commands::media::search_media_library` already established) table of
// *finished* jobs, reusing the exact same table-row/status-badge visual
// language `BatchJobsDialog.svelte` already established — this is genuinely
// "a second table, this time of finished/past jobs instead of live ones."
//
// Every §21 action maps 1:1 onto the real backend surface
// (`commands::history`'s own doc comment covers exactly which of §21's
// actions map to a real command vs. an existing mechanism elsewhere):
// - **View**: expand a row in place — no dedicated dialog, no command.
// - **"Download output"**: honestly, a desktop app has no "download" concept
//   for a file that is already local — the row shows the real
//   `output_path` and a "Copy output path" action (Clipboard API, no new
//   Tauri plugin needed). A genuine "reveal in file explorer" action would
//   need new backend surface (a small Rust command, or the
//   `tauri-plugin-opener` crate registered on the Rust side, neither of
//   which exists yet in `src-tauri/Cargo.toml`/`lib.rs`) — out of scope for
//   this frontend-only pass; see `UPGRADE_PLAN.md`'s Phase U3 writeup for
//   this gap called out explicitly.
// - **Re-run / Re-run with another template**: `rerun_from_history`/
//   `rerun_from_history_with_template`, then handed to `batchStore`'s own
//   `adoptExternalBatch` so the new batch's jobs show up in the existing,
//   already-real Batch Jobs dialog — no second "jobs" UI built here. Neither
//   discards any state (they only ever *start* a brand-new batch — the
//   original history entry is never touched, per `history::build_rerun_config`'s
//   own doc comment) so, unlike `highlightDetection.svelte.ts`'s "Create new
//   project" arm/confirm (which really does discard an unsaved timeline),
//   these fire immediately with no confirmation step.
// - **Clone settings**: `clone_history_entry_settings`, stashed as
//   `pendingClone` for `StartBatchDialog` to consume the next time it opens
//   (see that component's own `applyClonedConfig`) — this command itself
//   starts nothing.
// - **View logs**: reuses the real, already-shipped Phase 12
//   `commands.openLogsFolder()` (`stores/systemInfo.svelte.ts`'s own
//   precedent) — no second logs UI.
// - **Delete**: `delete_history_entry`, behind a lightweight arm/confirm —
//   same "click to arm, click again to confirm" shape as
//   `stores/batch.svelte.ts`'s own `pendingCancelId`. Unlike every other
//   action here, deleting a durable history row is genuinely irreversible,
//   so it is the one action in this store that gets a confirmation step.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.

import { commands } from "../types/bindings";
import type { BatchPipelineConfig, HistoryEntry, Template } from "../types/bindings";
import { batchStore } from "./batch.svelte";

const PAGE_SIZE = 50;

class HistoryStore {
  dialogOpen = $state(false);

  entries = $state<HistoryEntry[]>([]);
  loading = $state(false);
  loadError = $state<string | null>(null);
  /** A page shorter than `PAGE_SIZE` means there is nothing more to load —
   * the same inference `stores/modelManager.svelte.ts`-style "load more"
   * lists use, no separate total-count command exists. */
  hasMore = $state(true);

  expandedId = $state<string | null>(null);

  /** Loaded lazily on first open — just for rendering a template's real name
   * next to its id, and for the "run with another template" picker's own
   * option list. */
  templates = $state<Template[]>([]);
  private templatesLoaded = false;

  /** Row-scoped transient state, same "keyed by id" shape as
   * `stores/batch.svelte.ts`'s own `actionErrorByJob`/`actionPendingByJob`. */
  actionPendingById = $state<Record<string, boolean>>({});
  actionErrorById = $state<Record<string, string | null>>({});
  /** Row id whose "Copy output path" just succeeded — cleared automatically
   * after a short, purely-cosmetic timeout. */
  copiedId = $state<string | null>(null);

  /** Arm/confirm delete — see module doc comment. */
  pendingDeleteId = $state<string | null>(null);

  /** Row id currently showing the inline "run with another template" picker,
   * and the template currently selected in it. */
  pickingTemplateForId = $state<string | null>(null);
  pickedTemplateId = $state<string | null>(null);

  /** Set by `cloneSettings()`, consumed exactly once by `StartBatchDialog`
   * the next time it opens (`consumeClone()`) — `clone_history_entry_settings`
   * itself starts nothing, so this is the only hand-off needed. */
  pendingClone = $state<BatchPipelineConfig | null>(null);

  // -------------------------------------------------------------------
  // Dialog lifecycle
  // -------------------------------------------------------------------

  openDialog(): void {
    this.dialogOpen = true;
    void this.ensureTemplatesLoaded();
    if (this.entries.length === 0 && !this.loading) void this.load(true);
  }

  closeDialog(): void {
    this.dialogOpen = false;
  }

  async ensureTemplatesLoaded(): Promise<void> {
    if (this.templatesLoaded) return;
    const result = await commands.listTemplates();
    if (result.status === "ok") {
      this.templates = [...result.data.built_in, ...result.data.custom];
      this.templatesLoaded = true;
    }
  }

  templateName(templateId: string | null): string | null {
    if (templateId === null) return null;
    return this.templates.find((tpl) => tpl.id === templateId)?.name ?? templateId;
  }

  // -------------------------------------------------------------------
  // Loading / pagination
  // -------------------------------------------------------------------

  private async load(reset: boolean): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.loadError = null;
    try {
      const offset = reset ? 0 : this.entries.length;
      const result = await commands.listHistory(PAGE_SIZE, offset);
      if (result.status === "ok") {
        this.entries = reset ? result.data : [...this.entries, ...result.data];
        this.hasMore = result.data.length === PAGE_SIZE;
      } else {
        this.loadError = result.error.message;
      }
    } catch (err) {
      this.loadError = String(err);
    } finally {
      this.loading = false;
    }
  }

  async refresh(): Promise<void> {
    await this.load(true);
  }

  async loadMore(): Promise<void> {
    if (!this.hasMore) return;
    await this.load(false);
  }

  // -------------------------------------------------------------------
  // View
  // -------------------------------------------------------------------

  toggleExpand(id: string): void {
    this.expandedId = this.expandedId === id ? null : id;
  }

  // -------------------------------------------------------------------
  // "Download output" — see module doc comment for why this is "copy the
  // real path", not a real reveal-in-folder action.
  // -------------------------------------------------------------------

  async copyOutputPath(entry: HistoryEntry): Promise<void> {
    if (!entry.output_path) return;
    try {
      await navigator.clipboard.writeText(entry.output_path);
      this.copiedId = entry.id;
      setTimeout(() => {
        if (this.copiedId === entry.id) this.copiedId = null;
      }, 1500);
    } catch (err) {
      this.actionErrorById[entry.id] = String(err);
    }
  }

  // -------------------------------------------------------------------
  // View logs (Phase 12's real mechanism, reused verbatim)
  // -------------------------------------------------------------------

  async viewLogs(): Promise<void> {
    const result = await commands.openLogsFolder();
    if (result.status === "error") {
      this.loadError = result.error.message;
    }
  }

  // -------------------------------------------------------------------
  // Re-run / Re-run with another template
  // -------------------------------------------------------------------

  async rerun(entry: HistoryEntry): Promise<void> {
    this.actionPendingById[entry.id] = true;
    this.actionErrorById[entry.id] = null;
    try {
      const result = await commands.rerunFromHistory(entry.id);
      if (result.status === "ok") {
        await batchStore.adoptExternalBatch(result.data.batch_id, result.data.job_ids);
      } else {
        this.actionErrorById[entry.id] = result.error.message;
      }
    } catch (err) {
      this.actionErrorById[entry.id] = String(err);
    } finally {
      this.actionPendingById[entry.id] = false;
    }
  }

  openTemplatePicker(entry: HistoryEntry): void {
    this.pickingTemplateForId = entry.id;
    this.pickedTemplateId = entry.template_id;
  }

  cancelTemplatePicker(): void {
    this.pickingTemplateForId = null;
    this.pickedTemplateId = null;
  }

  setPickedTemplateId(id: string): void {
    this.pickedTemplateId = id;
  }

  async confirmRerunWithTemplate(entry: HistoryEntry): Promise<void> {
    const newTemplateId = this.pickedTemplateId;
    if (!newTemplateId) return;
    this.pickingTemplateForId = null;
    this.pickedTemplateId = null;
    this.actionPendingById[entry.id] = true;
    this.actionErrorById[entry.id] = null;
    try {
      const result = await commands.rerunFromHistoryWithTemplate(entry.id, newTemplateId);
      if (result.status === "ok") {
        await batchStore.adoptExternalBatch(result.data.batch_id, result.data.job_ids);
      } else {
        this.actionErrorById[entry.id] = result.error.message;
      }
    } catch (err) {
      this.actionErrorById[entry.id] = String(err);
    } finally {
      this.actionPendingById[entry.id] = false;
    }
  }

  // -------------------------------------------------------------------
  // Clone settings
  // -------------------------------------------------------------------

  async cloneSettings(entry: HistoryEntry): Promise<void> {
    this.actionPendingById[entry.id] = true;
    this.actionErrorById[entry.id] = null;
    try {
      const result = await commands.cloneHistoryEntrySettings(entry.id);
      if (result.status === "ok") {
        this.pendingClone = result.data;
        batchStore.openStartDialog();
      } else {
        this.actionErrorById[entry.id] = result.error.message;
      }
    } catch (err) {
      this.actionErrorById[entry.id] = String(err);
    } finally {
      this.actionPendingById[entry.id] = false;
    }
  }

  /** Consumed exactly once by `StartBatchDialog` — clears itself immediately
   * so a later, unrelated dialog open never reapplies a stale config. */
  consumeClone(): BatchPipelineConfig | null {
    const config = this.pendingClone;
    this.pendingClone = null;
    return config;
  }

  // -------------------------------------------------------------------
  // Delete (arm/confirm — module doc comment)
  // -------------------------------------------------------------------

  requestDelete(id: string): void {
    this.pendingDeleteId = id;
  }

  cancelDelete(): void {
    this.pendingDeleteId = null;
  }

  async confirmDelete(id: string): Promise<void> {
    this.pendingDeleteId = null;
    this.actionPendingById[id] = true;
    this.actionErrorById[id] = null;
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status === "ok") {
        this.entries = this.entries.filter((e) => e.id !== id);
      } else {
        this.actionErrorById[id] = result.error.message;
      }
    } catch (err) {
      this.actionErrorById[id] = String(err);
    } finally {
      this.actionPendingById[id] = false;
    }
  }
}

export const historyStore = new HistoryStore();
