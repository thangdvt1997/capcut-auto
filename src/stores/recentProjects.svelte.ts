// Svelte 5 runes-based store for "Recent Projects" (`STUDIO_PLAN.md` Phase
// D14 — closes the audit's own named gap: "Recent projects — no list/store/
// command anywhere").
//
// ## Why this only became buildable this same phase
//
// Before this phase, this codebase had no real file-backed "open"/"save" at
// all: `commands::project::new_project` only ever builds a brand-new,
// in-memory `ProjectV1` with no path, and `project::io::ProjectV1::
// save_atomic`/`load` (the real, already-tested atomic-write/load
// primitives) were never wired to a Tauri command or called from any
// frontend code — confirmed via grep before writing this store (see
// `commands::diagnostics::SystemInformation::project_directory`'s own doc
// comment: "no default project folder / recent-projects concept exists
// anywhere on the backend"). A "recent projects" list needs a real file
// path to be honest, so this phase's own real, minimal addition —
// `commands::project::save_project_as`/`open_project`, thin IPC wrappers
// around those same already-tested primitives — is the necessary
// prerequisite this store builds on. Every entry recorded here comes from a
// real, successful round trip through one of those two commands; never
// fabricated.
//
// ## Persistence
//
// `localStorage`, matching `stores/aiSettings.svelte.ts`/
// `stores/projectFolder.svelte.ts`'s own established convention: a JSON blob
// under a namespaced key, best-effort try/catch since storage can be
// unavailable (private browsing, disabled storage) — this list simply won't
// survive a restart in that case, never a thrown error.

import { open, save } from "@tauri-apps/plugin-dialog";
import { timeline } from "./timeline.svelte";
import { projectFolderStore, defaultSavePath } from "./projectFolder.svelte";

const STORAGE_KEY = "ave:recentProjects";

/** Capped at a reasonable number (task brief's own suggestion) — this is a
 * "jump back in" convenience list, not a full project history (that's
 * `stores/history.svelte.ts`'s own, unrelated, job-history concern). */
const MAX_ENTRIES = 10;

export interface RecentProjectEntry {
  /** The real, absolute path this project was actually saved to or opened
   * from — never fabricated. */
  path: string;
  /** The real `ProjectV1.project.name` at the moment of that real save/open,
   * not re-derived from the filename (a project's display name and its
   * filename can differ). */
  name: string;
  /** ISO 8601 timestamp (`Date.prototype.toISOString`) of that real
   * save/open moment. */
  lastOpenedAt: string;
}

function isRecentProjectEntry(value: unknown): value is RecentProjectEntry {
  if (!value || typeof value !== "object") return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.path === "string" &&
    typeof v.name === "string" &&
    typeof v.lastOpenedAt === "string"
  );
}

function loadEntries(): RecentProjectEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isRecentProjectEntry);
  } catch {
    // localStorage may be unavailable, or hold a malformed value from an
    // older/foreign build — fall back to an empty list rather than throw.
    return [];
  }
}

function saveEntries(entries: RecentProjectEntry[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch {
    /* storage may be disabled — the list simply won't survive a restart */
  }
}

/** Windows paths are case-insensitive (this app is Windows-only in scope,
 * matching `project::io`'s own test doc comments) — two paths differing
 * only in case are the same real file, so deduping must compare this way to
 * avoid ever showing the same project twice. */
function samePath(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}

class RecentProjectsStore {
  entries = $state<RecentProjectEntry[]>(loadEntries());

  /**
   * Records a real, just-opened-or-saved project file: moves it to the
   * front if a matching path is already present (deduped, never
   * duplicated), otherwise inserts it as the newest entry, then caps the
   * list at `MAX_ENTRIES` (dropping the oldest). Only ever called after a
   * real, successful `open_project`/`save_project_as` round trip (see
   * `open`/`TopBar.svelte`'s save flow) — never with synthesized data.
   */
  record(path: string, name: string): void {
    const withoutExisting = this.entries.filter((e) => !samePath(e.path, path));
    const next = [
      { path, name, lastOpenedAt: new Date().toISOString() },
      ...withoutExisting,
    ].slice(0, MAX_ENTRIES);
    this.entries = next;
    saveEntries(next);
  }

  remove(path: string): void {
    const next = this.entries.filter((e) => !samePath(e.path, path));
    this.entries = next;
    saveEntries(next);
  }

  clear(): void {
    this.entries = [];
    saveEntries([]);
  }

  /**
   * Opens `path` through the real `open_project` Tauri command — via
   * `timeline.openFromPath`, the exact same backend call and store method
   * the File menu's "Open Project…" flow (and this store's own
   * `browseAndOpen` below) already use — then records it here on success.
   * This is the single shared entry point every "reopen a recent project"
   * click (`TopBar.svelte`'s Recent list, `ScriptEditor.svelte`'s no-project
   * empty state) calls, so reopening never duplicates the real open-project
   * logic. Returns `true` on success; `timeline.lastError` is already set on
   * failure (same convention `openFromPath` itself documents).
   */
  async open(path: string): Promise<boolean> {
    const project = await timeline.openFromPath(path);
    if (project) {
      this.record(path, project.project.name);
      return true;
    }
    return false;
  }

  /**
   * Shows the native "Open" file picker (filtered to `.json` project files,
   * starting from the First-Run Wizard's own default save-browsing location
   * when one was chosen — `stores/projectFolder.svelte.ts`) and, if the user
   * picks a file, opens it via `open` above. Factored out here (rather than
   * duplicated in both `TopBar.svelte` and `ScriptEditor.svelte`) so both of
   * this phase's two real UI entry points share one real dialog+open flow.
   */
  async browseAndOpen(): Promise<boolean> {
    const selected = await open({
      filters: [{ name: "Project", extensions: ["json"] }],
      defaultPath: projectFolderStore.path ?? undefined,
    });
    if (selected && typeof selected === "string") {
      return this.open(selected);
    }
    return false;
  }

  /**
   * `STUDIO_PLAN.md` Phase D19: the real "Save Project As…" dialog+save
   * flow, factored out here (byte-for-byte the same steps `TopBar.svelte`'s
   * own File-menu "Save Project As…" handler already runs inline) so a
   * second real call site — `WorkspaceSimple.svelte`'s new Ctrl+S shortcut —
   * can trigger the exact same real flow without duplicating it, mirroring
   * this store's own `browseAndOpen()` precedent for "open". `TopBar.svelte`
   * itself is left as-is (not refactored to call this) to keep this pass's
   * diff minimal and avoid touching a file a concurrent pass might also be
   * editing; the two call sites' logic is intentionally identical, not
   * accidentally duplicated.
   *
   * This app tracks no "already has a known save path" state for the
   * current session project — `timeline`'s own `saveProjectAs` doc comment
   * confirms every save, including this one, always goes through the native
   * picker (there is no `load_timeline_project`/`ProjectV1` field recording
   * "the path this session's project was last saved to or opened from" to
   * silently reuse). So Ctrl+S always takes this same picker path rather
   * than trying to guess a path and silently no-op-ing when it can't.
   * Returns `true` only on a real, successful save.
   */
  async saveCurrentProjectAs(): Promise<boolean> {
    const project = timeline.project;
    if (!project) return false;
    const chosen = await save({
      filters: [{ name: "Project", extensions: ["json"] }],
      defaultPath: defaultSavePath(`${project.project.name}.json`),
    });
    if (!chosen) return false;
    const ok = await timeline.saveProjectAs(chosen);
    if (ok) this.record(chosen, project.project.name);
    return ok;
  }
}

export const recentProjectsStore = new RecentProjectsStore();
