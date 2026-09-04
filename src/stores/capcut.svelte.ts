// Svelte 5 runes-based store for Phase 9's two CapCut-adapter frontend
// pieces: the CapCut/Jianying Settings panel (master prompt §30 — detected
// version/path/draft directory, manual override, "never overwrite user
// drafts without confirmation") and the "Export to CapCut" dialog (master
// prompt §31 — Create New Draft / Update Existing Draft, feature-
// compatibility warnings). Structurally mirrors `stores/render.svelte.ts`
// (detect capability -> let the user pick/override -> confirm -> fire a
// one-shot backend call) and borrows `stores/modelManager.svelte.ts`'s
// `pendingDeleteId` two-step-confirmation precedent for the destructive
// "this may overwrite a draft" step.
//
// `.svelte.ts` (not `.ts`) is required for `$state` to work outside a
// `.svelte` file — same reasoning as `stores/media.svelte.ts`.
//
// ## Where things are persisted, and why
//
// - **Manual draft-root override** (§30): this app has no general Settings-
//   persistence backend yet (`ModelManagerDialog.svelte`'s own doc comment
//   notes the same gap for master prompt §46's full Settings surface), so
//   this is persisted to `localStorage` directly — a machine-wide, not
//   per-project, choice ("where does *this machine's* CapCut/Jianying keep
//   its drafts"), matching `lib/i18n.svelte.ts`'s own `localStorage`
//   precedent for a similarly machine-wide preference (there, the UI
//   locale). It survives an app restart; it does not sync across machines
//   or ship inside `project.json` (correctly — a draft root is a property
//   of the machine CapCut is installed on, not of the project).
// - **Last CapCut export target** (reused to default "Update Existing
//   Draft"): this genuinely *is* per-project, and `ProjectV1.export.
//   last_capcut_draft_path` (`docs/project-format.md`) already exists in
//   the schema for exactly this purpose — so `confirmExport` below writes
//   into that field directly (a plain mutation of the reactive project
//   object, not a `timeline::command.rs` undo-tracked primitive: which
//   draft a project was last exported to is bookkeeping metadata, not
//   timeline content the user would ever want to undo). Nothing currently
//   saves `project.json` to disk at all (same gap Phase 7 already
//   documented), so today this only survives for the lifetime of the
//   in-memory project object — a real save/load pass will make it durable
//   for free, no further change needed here.
//
// ## Why "Update Existing Draft" browses instead of enumerating
//
// The task brief allows either "enumerate subfolders of the draft root" or
// "let the user browse to one". This picks browsing: this is a frontend-
// only pass and the app has no filesystem-listing capability at all today
// (no `@tauri-apps/plugin-fs` dependency, no generic "list directory"/"path
// exists" Tauri command) — adding either would mean introducing new surface
// area (a new npm dependency, or new `src-tauri/` command) that a frontend-
// only task shouldn't invent. `@tauri-apps/plugin-dialog`'s native
// directory picker already lets the user browse the real filesystem
// (including under the detected/overridden draft root, via `defaultPath`)
// to pick a real existing folder — functionally equivalent for the user,
// with no new backend surface.
//
// ## Why every export (not just "Update Existing Draft") gets a confirm step
//
// The same missing filesystem-read capability means this store cannot check
// whether a freshly-typed "Create New Draft" name happens to collide with a
// real existing folder before writing to it. Rather than silently trust
// "Create" mode is always non-destructive, `requestExport`/`confirmExport`
// require an explicit confirmation click for *both* modes, and the
// confirmation copy says plainly that an existing draft at that exact path
// would be overwritten — an honest, conservative reading of master prompt
// §30's "never overwrite user drafts without confirmation" given what this
// pass can actually verify from the frontend.

import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../types/bindings";
import type { CapCutRegistryHint, DetectedCapCutInstallation, ProjectV1 } from "../types/bindings";
import { computeCapcutCompatWarnings, type CapcutCompatWarning } from "../capcut/compat";
import { timeline } from "./timeline.svelte";

/** Plain-object copy of a Svelte 5 `$state` reactive value, safe to hand to
 * an IPC call. Same trick (and same reason it's duplicated rather than
 * imported) as `stores/render.svelte.ts`'s own `snap()`. */
function snap<T>(value: T): T {
  return $state.snapshot(value) as T;
}

const DRAFT_ROOT_OVERRIDE_STORAGE_KEY = "ave:capcut:draftRootOverride";

function loadDraftRootOverride(): string | null {
  try {
    return localStorage.getItem(DRAFT_ROOT_OVERRIDE_STORAGE_KEY);
  } catch {
    // localStorage may be unavailable (private browsing, disabled storage).
    return null;
  }
}

function saveDraftRootOverride(path: string | null): void {
  try {
    if (path) {
      localStorage.setItem(DRAFT_ROOT_OVERRIDE_STORAGE_KEY, path);
    } else {
      localStorage.removeItem(DRAFT_ROOT_OVERRIDE_STORAGE_KEY);
    }
  } catch {
    /* storage may be disabled — override simply won't survive a restart */
  }
}

/** Whichever path separator `path` already uses (falls back to `\` — this
 * app is Windows-only in scope) — used so a fresh draft-name subfolder is
 * joined onto a detected/overridden root with the same separator style
 * that root already uses, rather than assuming one. */
function separatorOf(path: string): "\\" | "/" {
  return path.includes("/") && !path.includes("\\") ? "/" : "\\";
}

function joinPath(root: string, name: string): string {
  const sep = separatorOf(root);
  const trimmedRoot = root.endsWith(sep) ? root.slice(0, -1) : root;
  return `${trimmedRoot}${sep}${name}`;
}

/** Characters reserved/illegal in a Windows path segment — stripped from a
 * user-typed draft name so "Create New Draft" can never be pointed outside
 * the chosen draft root or produce an invalid folder name. Spaces/hyphens
 * are left alone. */
const INVALID_DRAFT_NAME_CHARS = /["*/:<>?|]|\\/g;

function sanitizeDraftName(name: string): string {
  return name.replace(INVALID_DRAFT_NAME_CHARS, "").trim();
}

export type CapcutExportMode = "create" | "update";

class CapCutStore {
  // -------------------------------------------------------------------
  // Detection & settings (master prompt §30)
  // -------------------------------------------------------------------

  settingsOpen = $state(false);

  detectLoading = $state(false);
  detectError = $state<string | null>(null);
  /** Whether a detection pass has completed at least once — lets
   * `ensureDetected()` skip re-running on every dialog open, matching
   * `renderStore.ensurePresetsLoaded()`'s own "loaded once, reused after"
   * precedent. `rescan()` forces a fresh pass regardless. */
  detectedOnce = $state(false);

  installations = $state<DetectedCapCutInstallation[]>([]);
  registryHints = $state<CapCutRegistryHint[]>([]);

  /** User-typed/browsed override — takes priority over anything detected.
   * Seeded from `localStorage` at module load (see class doc comment). */
  manualDraftRoot = $state<string | null>(loadDraftRootOverride());

  /** The draft root the export flow actually resolves against: the manual
   * override when set, else the first detected installation (`detect.rs::
   * scan_users_root`'s own documented display-order preference — fast-path
   * home first, Jianying before CapCut for a given profile), else `null`
   * when nothing is known yet. */
  effectiveDraftRoot = $derived(this.manualDraftRoot ?? this.installations[0]?.draft_root ?? null);

  openSettings(): void {
    this.settingsOpen = true;
    void this.ensureDetected();
  }

  closeSettings(): void {
    this.settingsOpen = false;
  }

  async ensureDetected(): Promise<void> {
    if (this.detectLoading || this.detectedOnce) return;
    await this.rescan();
  }

  /** Forces a fresh detection pass regardless of `detectedOnce` — wired to
   * the settings panel's explicit "Re-scan" button (installing CapCut while
   * the app is open, or plugging in a different user profile's drive, are
   * real cases a one-shot "detect on first open" wouldn't ever notice). */
  async rescan(): Promise<void> {
    if (this.detectLoading) return;
    this.detectLoading = true;
    this.detectError = null;
    try {
      const [installations, registryHints] = await Promise.all([
        commands.detectCapcutInstallations(),
        commands.detectCapcutRegistryHints(),
      ]);
      this.installations = installations;
      this.registryHints = registryHints;
      this.detectedOnce = true;
    } catch (err) {
      // Both commands are documented to always succeed (empty `Vec` rather
      // than an error), but the IPC call itself could still reject (e.g. a
      // malformed invoke) — surfaced rather than swallowed.
      this.detectError = String(err);
    } finally {
      this.detectLoading = false;
    }
  }

  setManualDraftRoot(path: string | null): void {
    const trimmed = path?.trim() ?? "";
    this.manualDraftRoot = trimmed === "" ? null : trimmed;
    saveDraftRootOverride(this.manualDraftRoot);
  }

  async browseManualDraftRoot(): Promise<void> {
    const selected = await open({ directory: true, defaultPath: this.effectiveDraftRoot ?? undefined });
    if (selected && typeof selected === "string") {
      this.setManualDraftRoot(selected);
    }
  }

  // -------------------------------------------------------------------
  // Export to CapCut (master prompt §31)
  // -------------------------------------------------------------------

  exportOpen = $state(false);
  mode = $state<CapcutExportMode>("create");
  draftName = $state("Untitled Draft");
  /** Only meaningful in `"update"` mode — a real, existing folder the user
   * browsed to (see class doc comment for why this is "browse", not
   * "enumerate"). */
  existingDraftPath = $state<string | null>(null);

  /** Armed by `requestExport()`, cleared by `cancelExportConfirm()` or a
   * successful/failed `confirmExport()` — see class doc comment for why
   * this two-step gate applies to both modes. */
  confirmingExport = $state(false);

  exporting = $state(false);
  exportError = $state<string | null>(null);
  exportedPath = $state<string | null>(null);

  /** Opens the dialog, lazily (re)using detection the settings panel may
   * already have loaded, and — if this project was exported to CapCut
   * before (`project.export.last_capcut_draft_path`) — defaults to "Update
   * Existing Draft" pointed at that same folder, since re-exporting to the
   * same draft is the more likely next action than starting a fresh one. */
  openExport(): void {
    this.exportOpen = true;
    this.exportError = null;
    this.exportedPath = null;
    this.confirmingExport = false;
    void this.ensureDetected();

    const lastPath = timeline.project?.export.last_capcut_draft_path ?? null;
    if (lastPath) {
      this.mode = "update";
      this.existingDraftPath = lastPath;
    }
  }

  closeExport(): void {
    this.exportOpen = false;
  }

  setMode(next: CapcutExportMode): void {
    this.mode = next;
    this.confirmingExport = false;
  }

  async browseExistingDraft(): Promise<void> {
    const selected = await open({ directory: true, defaultPath: this.effectiveDraftRoot ?? undefined });
    if (selected && typeof selected === "string") {
      this.existingDraftPath = selected;
      this.confirmingExport = false;
    }
  }

  /** The resolved on-disk folder this export will write to: the chosen
   * existing folder in `"update"` mode, or a fresh subfolder named after
   * `draftName` under `effectiveDraftRoot` in `"create"` mode. `null` when
   * nothing is resolvable yet (no draft root known and no existing folder
   * chosen), which disables the Export button rather than sending a
   * command with an empty path. */
  targetPath = $derived.by((): string | null => {
    if (this.mode === "update") {
      return this.existingDraftPath;
    }
    const root = this.effectiveDraftRoot;
    const name = sanitizeDraftName(this.draftName);
    if (!root || name === "") return null;
    return joinPath(root, name);
  });

  compatWarnings = $derived.by((): CapcutCompatWarning[] =>
    timeline.project ? computeCapcutCompatWarnings(timeline.project) : [],
  );

  canExport = $derived(
    timeline.project !== null && this.targetPath !== null && !this.exporting,
  );

  /** First click: arms the overwrite-confirmation step instead of exporting
   * immediately — see class doc comment for why this applies uniformly to
   * both modes. Mirrors `modelManager.svelte.ts`'s `pendingDeleteId`
   * two-step-confirm precedent (arm, then a second explicit click actually
   * performs the destructive action). */
  requestExport(): void {
    if (!this.canExport) return;
    this.confirmingExport = true;
  }

  cancelExportConfirm(): void {
    this.confirmingExport = false;
  }

  async confirmExport(): Promise<void> {
    const project: ProjectV1 | null = timeline.project;
    const path = this.targetPath;
    if (!project || !path || this.exporting) return;
    this.exporting = true;
    this.exportError = null;
    try {
      const result = await commands.exportProjectToCapcutDraft(snap(project), path);
      if (result.status === "ok") {
        this.exportedPath = path;
        this.confirmingExport = false;
        // Persist onto the project itself (`ProjectV1.export.
        // last_capcut_draft_path`, see class doc comment) — a direct
        // mutation of the reactive project object, since this is
        // bookkeeping metadata, not a timeline edit that goes through
        // `timeline::command.rs`'s undo-tracked primitives.
        project.export.last_capcut_draft_path = path;
      } else {
        this.exportError = result.error.message;
      }
    } catch (err) {
      this.exportError = String(err);
    } finally {
      this.exporting = false;
    }
  }

  /** Clears the finished export's result so the dialog is usable for a
   * fresh export without closing/reopening it — mirrors `render.svelte.ts`'s
   * `startNewExport()`. */
  startNewExport(): void {
    this.exportedPath = null;
    this.exportError = null;
  }
}

export const capcutStore = new CapCutStore();
