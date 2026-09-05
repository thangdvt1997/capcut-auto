// Svelte 5 runes-based store for the Phase 12 First-Run Wizard's "Project
// Folder" step (master prompt §58).
//
// ## What this genuinely is, and what it is NOT
//
// There is no Project Manager anywhere in this codebase yet — confirmed by
// `src-tauri/src/commands/diagnostics.rs`'s own `SystemInformation::
// project_directory` doc comment: `project::io::ProjectV1::save_atomic`/
// `load` take an arbitrary caller-chosen path, and no "default project
// folder"/"recent projects" concept exists anywhere on the backend. This
// store does NOT invent one. It is only a lightweight, frontend-only,
// `localStorage`-persisted **default save-browsing location** — a path handed
// to `@tauri-apps/plugin-dialog`'s `save()`/`open()` as `defaultPath` so the
// native file picker starts somewhere the user chose during setup, instead of
// wherever the OS/webview defaults to. Nothing on the backend knows this
// value exists; it is never sent over IPC.
//
// Persisted to `localStorage` matching every other non-project, app-level
// setting in this codebase (`stores/aiSettings.svelte.ts`/
// `stores/capcut.svelte.ts`'s own doc comments establish this precedent).

import { open } from "@tauri-apps/plugin-dialog";

const STORAGE_KEY = "ave:projectFolder";

function loadPath(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    // localStorage may be unavailable (private browsing, disabled storage).
    return null;
  }
}

function savePath(path: string | null): void {
  try {
    if (path) {
      localStorage.setItem(STORAGE_KEY, path);
    } else {
      localStorage.removeItem(STORAGE_KEY);
    }
  } catch {
    /* storage may be disabled — the preference simply won't survive a restart */
  }
}

/** Whichever path separator `path` already uses (falls back to `\` — this
 * app is Windows-only in scope) — same precedent as
 * `stores/capcut.svelte.ts`'s own `separatorOf`/`joinPath` helpers, not
 * imported from there since that module doesn't export them. */
function separatorOf(path: string): "\\" | "/" {
  return path.includes("/") && !path.includes("\\") ? "/" : "\\";
}

function joinPath(root: string, name: string): string {
  const sep = separatorOf(root);
  const trimmedRoot = root.endsWith(sep) ? root.slice(0, -1) : root;
  return `${trimmedRoot}${sep}${name}`;
}

class ProjectFolderStore {
  /** Seeded from `localStorage` at module load — `null` means "no default
   * chosen yet", which every consumer treats as "let the native picker fall
   * back to its own default" rather than fabricating a path. */
  path = $state<string | null>(loadPath());

  setPath(next: string | null): void {
    const trimmed = next?.trim() ?? "";
    this.path = trimmed === "" ? null : trimmed;
    savePath(this.path);
  }

  async browse(): Promise<void> {
    const selected = await open({ directory: true, defaultPath: this.path ?? undefined });
    if (selected && typeof selected === "string") {
      this.setPath(selected);
    }
  }

  clear(): void {
    this.setPath(null);
  }

  /** `filename` joined onto the chosen default folder, for handing to
   * `@tauri-apps/plugin-dialog`'s `save()` as `defaultPath` — or just
   * `filename` unchanged when no default folder has been chosen. Exported as
   * a plain function (not a method) so any save-dialog call site can use it
   * without importing the whole store class shape, matching
   * `stores/modelManager.svelte.ts`'s own `openModelManager()` convenience-
   * export precedent. */
  defaultSavePath(filename: string): string {
    return this.path ? joinPath(this.path, filename) : filename;
  }
}

export const projectFolderStore = new ProjectFolderStore();

export function defaultSavePath(filename: string): string {
  return projectFolderStore.defaultSavePath(filename);
}
